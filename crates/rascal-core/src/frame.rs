//! Frames: coordinate spaces with a size, a projection and a topology, and for a derived
//! frame its parent and the mapping to it. A frame has no pixels and knows nothing of pixel
//! content: two images of the same size and projection are in the same raw frame.
//!
//! The three v1 kinds: `raw:<W>x<H>:<projection>` (a root), `cut:<image-id>:<plan-id>` (the
//! two edge strips of a wrapped image side by side, so the wrap edge is the centre column)
//! and `crop:<n>` (an integer bbox of the parent). Identities follow SPEC.md "Artifacts and
//! identity": a raw frame's involves no image; a derived frame's is its parent's identity plus
//! the mapping parameters.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::identity::Identity;

/// How an image's pixels map onto the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// An ordinary photograph.
    Flat,
    /// A full 360 by 180 panorama whose left and right edges meet.
    Equirectangular,
}

impl Projection {
    /// The name used in frame ids.
    pub fn name(self) -> &'static str {
        match self {
            Projection::Flat => "flat",
            Projection::Equirectangular => "equirectangular",
        }
    }
}

/// Whether the frame's horizontal edges meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    /// The left and right edges are the same column: a complete equirectangular frame.
    Wrap,
    /// The edges are edges.
    Bounded,
}

/// The cut plan: how a `cut:` canvas was taken from its parent and how `put` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    /// The parent's width.
    pub width: u32,
    /// The parent's height.
    pub height: u32,
    /// The strip width: even, at least 2, `2s <= width`.
    pub strip: u32,
    /// The parent columns `[start, end)` the canvas's left half came from: the rightmost `s`.
    pub left_span: [u32; 2],
    /// The parent columns the canvas's right half came from: the leftmost `s`.
    pub right_span: [u32; 2],
    /// `sha256:` over the parent frame's identity, width, height, strip and the spans.
    pub identity: String,
}

impl Plan {
    /// The strip width for a parent of width `w` and a strip fraction `f` in `(0, 0.5]`:
    /// `s = min(max(2, 2 · round_half_up(f · W / 2)), 2 · floor(W / 4))`.
    pub fn strip_width(width: u32, fraction: f64) -> u32 {
        let half = f64::from(width) * fraction / 2.0;
        let rounded = (half + 0.5).floor() as u32;
        (2 * rounded).max(2).min(2 * (width / 4))
    }

    /// The plan for a parent frame and a strip fraction.
    pub fn new(parent: &Frame, fraction: f64) -> Result<Plan> {
        if parent.width < 4 {
            return Err(Error::op(
                "cut",
                format!(
                    "the image is {} wide; cut needs at least 4 columns",
                    parent.width
                ),
            ));
        }
        if !(fraction > 0.0 && fraction <= 0.5) {
            return Err(Error::usage(format!(
                "strip must be in (0, 0.5], got {fraction}"
            )));
        }
        let s = Plan::strip_width(parent.width, fraction);
        let mut plan = Plan {
            width: parent.width,
            height: parent.height,
            strip: s,
            left_span: [parent.width - s, parent.width],
            right_span: [0, s],
            identity: String::new(),
        };
        plan.identity = canonical::identity_of(
            "plan",
            &serde_json::json!({
                "parent": parent.identity,
                "width": plan.width, "height": plan.height, "strip": plan.strip,
                "left_span": plan.left_span, "right_span": plan.right_span,
            }),
        )
        .as_str()
        .to_string();
        Ok(plan)
    }
}

/// The kinds of frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// A root: the frame of an imported image.
    Raw,
    /// The two edge strips of a wrapped parent, side by side; the centre column is the
    /// parent's wrap edge. Carries `parent`, `parent_identity` and `plan`.
    Cut,
    /// An integer bbox of the parent. Carries `parent`, `parent_identity` and `bbox`.
    Crop,
}

/// A frame record, as written to `frames.json`. The fields a kind needs are set; the others
/// are absent (`serde(flatten)` and `deny_unknown_fields` do not combine, so the derived
/// frames' fields are optional rather than a tagged union).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// The record's version.
    pub frame_version: u32,
    /// The semantic id: `raw:<W>x<H>:<projection>`, `cut:<image-id>:<plan-id>`, `crop:<n>`.
    pub id: String,
    pub kind: FrameKind,
    /// The parent frame id, for a derived frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The parent's identity, for a derived frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_identity: Option<String>,
    /// The cut plan, for a `cut:` frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    /// `[left, top, right, bottom)` in parent pixels, for a `crop:` frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[u32; 4]>,
    pub width: u32,
    pub height: u32,
    pub projection: Projection,
    pub topology: Topology,
    /// `sha256:` over the kind, size, projection and topology, and for a derived frame its
    /// parent's identity and the mapping parameters; no image is involved.
    pub identity: String,
}

impl Frame {
    /// The raw frame of an image of this size and projection.
    pub fn raw(width: u32, height: u32, projection: Projection) -> Frame {
        let topology = match projection {
            Projection::Equirectangular => Topology::Wrap,
            Projection::Flat => Topology::Bounded,
        };
        let identity = canonical::identity_of(
            "frame",
            &serde_json::json!({
                "kind": "raw", "width": width, "height": height,
                "projection": projection.name(), "topology": topology_name(topology),
            }),
        );
        Frame {
            frame_version: 1,
            id: format!("raw:{width}x{height}:{}", projection.name()),
            kind: FrameKind::Raw,
            parent: None,
            parent_identity: None,
            plan: None,
            bbox: None,
            width,
            height,
            projection,
            topology,
            identity: identity.as_str().to_string(),
        }
    }

    /// The `cut:` frame of a parent image under a plan. The canvas is `2s × H`, bounded, flat.
    pub fn cut(parent: &Frame, image_id: &str, plan: &Plan) -> Result<Frame> {
        if parent.topology != Topology::Wrap {
            return Err(Error::op(
                "cut",
                format!(
                    "frame {} does not wrap; cut needs an equirectangular raw frame",
                    parent.id
                ),
            ));
        }
        if plan.width != parent.width || plan.height != parent.height {
            return Err(Error::op("cut", "the plan was made for a different frame"));
        }
        let identity = canonical::identity_of(
            "frame",
            &serde_json::json!({
                "kind": "cut", "parent": parent.identity,
                "width": plan.width, "height": plan.height, "strip": plan.strip,
            }),
        );
        Ok(Frame {
            frame_version: 1,
            id: format!("cut:{image_id}:{}", &plan.identity[7..19]),
            kind: FrameKind::Cut,
            parent: Some(parent.id.clone()),
            parent_identity: Some(parent.identity.clone()),
            plan: Some(plan.clone()),
            bbox: None,
            width: 2 * plan.strip,
            height: plan.height,
            projection: Projection::Flat,
            topology: Topology::Bounded,
            identity: identity.as_str().to_string(),
        })
    }

    /// The `crop:` frame of an integer bbox `[left, top, right, bottom)` of the parent.
    pub fn crop(parent: &Frame, n: u64, bbox: [u32; 4]) -> Result<Frame> {
        let [l, t, r, b] = bbox;
        if l >= r || t >= b || r > parent.width || b > parent.height {
            return Err(Error::op(
                "window",
                format!(
                    "crop {bbox:?} is not inside the {}x{} frame {}",
                    parent.width, parent.height, parent.id
                ),
            ));
        }
        let identity = canonical::identity_of(
            "frame",
            &serde_json::json!({ "kind": "crop", "parent": parent.identity, "bbox": bbox }),
        );
        Ok(Frame {
            frame_version: 1,
            id: format!("crop:{n}"),
            kind: FrameKind::Crop,
            parent: Some(parent.id.clone()),
            parent_identity: Some(parent.identity.clone()),
            plan: None,
            bbox: Some(bbox),
            width: r - l,
            height: b - t,
            projection: Projection::Flat,
            topology: Topology::Bounded,
            identity: identity.as_str().to_string(),
        })
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.identity)
    }

    /// The parent frame id, for a derived frame.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// True for a root frame.
    pub fn is_raw(&self) -> bool {
        self.kind == FrameKind::Raw
    }

    /// The fields the kind needs are set and no other.
    pub fn validate(&self) -> Result<()> {
        let ok = match self.kind {
            FrameKind::Raw => {
                self.parent.is_none()
                    && self.parent_identity.is_none()
                    && self.plan.is_none()
                    && self.bbox.is_none()
            }
            FrameKind::Cut => {
                self.parent.is_some()
                    && self.parent_identity.is_some()
                    && self.plan.is_some()
                    && self.bbox.is_none()
            }
            FrameKind::Crop => {
                self.parent.is_some()
                    && self.parent_identity.is_some()
                    && self.plan.is_none()
                    && self.bbox.is_some()
            }
        };
        if ok {
            Ok(())
        } else {
            Err(Error::usage(format!(
                "frame {}: fields do not match its kind",
                self.id
            )))
        }
    }
}

fn topology_name(topology: Topology) -> &'static str {
    match topology {
        Topology::Wrap => "wrap",
        Topology::Bounded => "bounded",
    }
}

/// The frames a run or workspace knows, by id. REPLACED BY 1.7a (the registry generation).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FrameTable {
    pub frames: Vec<Frame>,
}

impl FrameTable {
    /// Adds a frame if its id is new and returns it.
    pub fn insert(&mut self, frame: Frame) -> &Frame {
        if let Some(index) = self.frames.iter().position(|f| f.id == frame.id) {
            return &self.frames[index];
        }
        self.frames.push(frame);
        self.frames
            .last()
            .unwrap_or_else(|| unreachable!("just pushed"))
    }

    /// The frame with this id.
    pub fn get(&self, id: &str) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }

    /// The frame with this id, or an error naming it.
    pub fn require(&self, id: &str) -> Result<&Frame> {
        self.get(id)
            .ok_or_else(|| Error::usage(format!("frame {id} is unknown")))
    }

    /// The chain from a frame up to its root, the frame itself first.
    pub fn chain(&self, id: &str) -> Result<Vec<&Frame>> {
        let mut chain = Vec::new();
        let mut current = self.require(id)?;
        loop {
            chain.push(current);
            match current.parent() {
                None => return Ok(chain),
                Some(parent) => {
                    if chain.len() > 64 {
                        return Err(Error::usage(format!(
                            "frame {id}: the parent chain does not end"
                        )));
                    }
                    current = self.require(parent)?;
                }
            }
        }
    }

    /// The next crop number.
    pub fn next_crop(&self) -> u64 {
        self.frames
            .iter()
            .filter(|f| f.kind == FrameKind::Crop)
            .count() as u64
            + 1
    }
}
