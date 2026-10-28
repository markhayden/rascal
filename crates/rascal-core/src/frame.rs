//! Frames: coordinate spaces with a size, a projection and a topology, and for a derived
//! frame its parent and mapping. A frame has no pixels.
//!
//! Task 1.1 has the `raw:` kind only. REPLACED BY 1.2 (`cut:` and `crop:`, mapping).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

/// Whether the frame's horizontal edges meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    /// The left and right edges are the same column: a complete equirectangular frame.
    Wrap,
    /// The edges are edges.
    Bounded,
}

/// The kinds of frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    /// A root: the frame of an imported image.
    Raw,
}

/// A frame record, as written to `frames.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// The record's version.
    pub frame_version: u32,
    /// The semantic id: `raw:<W>x<H>:<projection>`.
    pub id: String,
    pub kind: FrameKind,
    pub width: u32,
    pub height: u32,
    pub projection: Projection,
    pub topology: Topology,
    /// `sha256:` over kind, width, height, projection and topology; no image is involved.
    pub identity: String,
}

impl Frame {
    /// The raw frame of an image of this size and projection.
    pub fn raw(width: u32, height: u32, projection: Projection) -> Frame {
        let topology = match projection {
            Projection::Equirectangular => Topology::Wrap,
            Projection::Flat => Topology::Bounded,
        };
        let projection_name = match projection {
            Projection::Flat => "flat",
            Projection::Equirectangular => "equirectangular",
        };
        let topology_name = match topology {
            Topology::Wrap => "wrap",
            Topology::Bounded => "bounded",
        };
        let identity = Identity::of_parts(&[
            b"raw",
            &width.to_le_bytes(),
            &height.to_le_bytes(),
            projection_name.as_bytes(),
            topology_name.as_bytes(),
        ]);
        Frame {
            frame_version: 1,
            id: format!("raw:{width}x{height}:{projection_name}"),
            kind: FrameKind::Raw,
            width,
            height,
            projection,
            topology,
            identity: identity.as_str().to_string(),
        }
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse(&self.identity)
            .unwrap_or_else(|| Identity::of_parts(&[self.identity.as_bytes()]))
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
}
