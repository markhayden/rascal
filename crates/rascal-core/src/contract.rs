//! table-manners, the shared contract of the waffle tools (ADR 0020): rascal's side of it. The
//! contract's types come from the `table-manners` crate, pinned exactly; this module builds them
//! from rascal's records and reads them back. Section numbers are CONTRACT.md's.

use sha2::{Digest, Sha256};
pub use table_manners;
use table_manners::images::{ImageReference, Projection as TmProjection, Topology as TmTopology};

use crate::frame::{Frame, Projection, Topology};
use crate::image::Image;
use crate::workspace::record::OperationRecord;

/// The contract version rascal conforms to.
pub const CONTRACT_VERSION: &str = table_manners::CONTRACT_VERSION;

/// The parameter an `import` record carries: the SHA-256 of the caller's file, lowercase hex.
pub const FILE_SHA256: &str = "file_sha256";

/// SHA-256 of bytes as lowercase hex, the contract's form for `file_sha256` (8.1).
pub fn file_sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// A frame's projection in the contract's words.
pub fn projection(frame: &Frame) -> TmProjection {
    match frame.projection {
        Projection::Flat => TmProjection::Flat,
        Projection::Equirectangular => TmProjection::Equirectangular,
    }
}

/// A frame's topology in the contract's words.
pub fn topology(frame: &Frame) -> TmTopology {
    match frame.topology {
        Topology::Wrap => TmTopology::Wrap,
        Topology::Bounded => TmTopology::Bounded,
    }
}

/// The reference for an image in its frame, given the digest of the file it came from or was
/// written to (8.1). The pixel digest is rascal's own recipe, which is the contract's (8.2).
pub fn reference(image: &Image, frame: &Frame, file_sha256: String) -> ImageReference {
    let (width, height) = image.dimensions();
    ImageReference {
        file_sha256,
        width,
        height,
        projection: projection(frame),
        topology: topology(frame),
        pixel_sha256: Some(bare(&image.record.pixel_digest)),
    }
}

/// `sha256:<hex>` to `<hex>`.
pub fn bare(identity: &str) -> String {
    identity
        .strip_prefix("sha256:")
        .unwrap_or(identity)
        .to_string()
}

/// The digest of the file an imported image came from: the `import` record that produced the
/// image (an external image's identity is in its envelope's artifact references). `None` for an image rascal made itself, or imported before rascal recorded it.
pub fn import_file_sha256(records: &[OperationRecord], image_identity: &str) -> Option<String> {
    records
        .iter()
        .rev()
        .filter(|r| r.op == "import")
        .find(|r| {
            r.outputs.values().any(|o| o == image_identity)
                || r.envelope
                    .artifacts
                    .values()
                    .any(|a| a.identity == image_identity)
        })
        .and_then(|r| r.params.get(FILE_SHA256))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// The reference for an image in a workspace (8.1): an imported image names the caller's file,
/// so a reference handed in by a caller matches; an image rascal made names the PNG it wrote.
pub fn image_reference(
    workspace: &crate::workspace::Workspace,
    id_or_identity: &str,
) -> crate::error::Result<ImageReference> {
    use crate::artifact::Artifact;
    use crate::error::Error;
    let generation = workspace.snapshot()?;
    let entry = generation
        .artifact(id_or_identity)
        .ok_or_else(|| Error::usage(format!("no artifact {id_or_identity} in the workspace")))?
        .clone();
    let Artifact::Image(image) = workspace.load(&entry.id)? else {
        return Err(Error::usage(format!("{} is not an image", entry.id)));
    };
    let frame = generation
        .frames
        .iter()
        .find(|f| f.id == entry.frame)
        .cloned()
        .ok_or_else(|| Error::usage(format!("frame {} is not in the workspace", entry.frame)))?;
    let file = match import_file_sha256(&workspace.records()?, &entry.identity) {
        Some(digest) => digest,
        None => {
            let path = workspace.root.join(&entry.path);
            file_sha256(&std::fs::read(&path).map_err(|e| Error::io(&path, e))?)
        }
    };
    Ok(reference(&image, &frame, file))
}

/// The most vertices rascal's own polygon holds (its location schema is the copied envelope's,
/// ADR 0009); a contract polygon with more is refused at the boundary, not resampled.
pub const RASCAL_POLYGON_MAX: usize = 64;

/// Whether two references name the same file (8.1): the digest and the dimensions. Projection
/// and topology are the receiver's to decide from its own import.
pub fn same_file(a: &ImageReference, b: &ImageReference) -> bool {
    a.file_sha256 == b.file_sha256 && a.width == b.width && a.height == b.height
}

/// A contract location (9) as rascal's, in `frame`, after checking that it is on the image the
/// caller gave (`given`). Refusals are usage errors naming what is wrong: another image, a shape
/// in several parts (rascal holds one shape per location), more vertices than rascal's polygon
/// holds, a `mask` in a location slot, a kind whose part does not match it.
pub fn location_in(
    location: &table_manners::images::Location,
    given: &ImageReference,
    frame: &Frame,
) -> crate::error::Result<crate::location::Location> {
    use crate::error::Error;
    use crate::location::Location;
    use table_manners::images::{LocationKind as K, Part as P};
    if !same_file(&location.image, given) {
        return Err(Error::usage(format!(
            "the location is on another image: it names file_sha256 {} ({}x{}), the image given is {} ({}x{})",
            location.image.file_sha256,
            location.image.width,
            location.image.height,
            given.file_sha256,
            given.width,
            given.height
        )));
    }
    let f = frame.id.clone();
    let one = |what: &str| -> crate::error::Result<&P> {
        match location.parts.as_slice() {
            [part] => Ok(part),
            [] => Err(Error::usage(format!(
                "a {what} location needs one part, got none"
            ))),
            _ => Err(Error::usage(format!(
                "a {what} location in {} parts (straddles_wrap: {}): rascal holds one shape per location; send each part as its own location",
                location.parts.len(),
                location.straddles_wrap
            ))),
        }
    };
    let mismatch =
        |kind: &str| Error::usage(format!("a {kind} location whose part is not a {kind}"));
    let out = match location.kind {
        K::Bbox => match one("bbox")? {
            P::Bbox(p) => Location::Bbox {
                frame: f,
                bbox: p.bbox,
            },
            _ => return Err(mismatch("bbox")),
        },
        K::Polygon => match one("polygon")? {
            P::Polygon(p) if p.polygon.len() > RASCAL_POLYGON_MAX => {
                return Err(Error::usage(format!(
                    "a polygon of {} vertices: rascal holds at most {RASCAL_POLYGON_MAX}",
                    p.polygon.len()
                )));
            }
            P::Polygon(p) => Location::Polygon {
                frame: f,
                polygon: p.polygon.clone(),
            },
            _ => return Err(mismatch("polygon")),
        },
        K::Line => match one("line")? {
            P::Column(p) => Location::Line {
                frame: f,
                column: Some(p.column),
                path: None,
                width_px: p.width_px,
                group: None,
            },
            P::Path(p) => Location::Line {
                frame: f,
                column: None,
                path: Some(p.path.clone()),
                width_px: p.width_px,
                group: None,
            },
            _ => return Err(mismatch("line")),
        },
        K::Stroke => match one("stroke")? {
            P::Stroke(p) => Location::Stroke {
                frame: f,
                path: p.path.clone(),
                radius_px: p.radius_px,
                group: None,
            },
            _ => return Err(mismatch("stroke")),
        },
        K::View => match one("view")? {
            P::View(p) => Location::View {
                frame: f,
                yaw: p.yaw,
                pitch: p.pitch,
                fov: p.fov,
            },
            _ => return Err(mismatch("view")),
        },
        K::WholeImage => Location::WholeImage { frame: f },
        K::Absent => Location::Absent { frame: f },
        K::Mask => {
            return Err(Error::usage(
                "a mask location goes to a mask slot (--mask), not a location slot",
            ));
        }
    };
    out.validate(frame)?;
    Ok(out)
}

/// What else a location leaving rascal carries besides its shape.
#[derive(Debug, Clone, Default)]
pub struct LocationMeta {
    pub label: Option<String>,
    pub score: Option<f64>,
    pub score_kind: Option<String>,
    pub reason: Option<String>,
    pub source: Option<table_manners::images::Source>,
}

/// rascal's locations as one contract location (9) on the image `reference` names. Several
/// locations are the parts of one shape split at the wrap edge (rascal's `group`), and must share
/// a kind; one location is one part. `view` never leaves rascal (9.2): a view is refused here.
pub fn location_out(
    parts: &[crate::location::Location],
    reference: ImageReference,
    meta: LocationMeta,
) -> crate::error::Result<table_manners::images::Location> {
    use crate::error::Error;
    use crate::location::Location;
    use table_manners::images::Part as P;
    use table_manners::images::{
        BboxPart, ColumnPart, LocationKind as K, PathPart, PolygonPart, StrokePart,
    };
    let first = parts
        .first()
        .ok_or_else(|| Error::usage("a location needs at least one part"))?;
    if parts.iter().any(|p| p.kind() != first.kind()) {
        return Err(Error::usage("the parts of one location share a kind"));
    }
    let mut out = Vec::new();
    for part in parts {
        match part {
            Location::Bbox { bbox, .. } => out.push(P::Bbox(BboxPart { bbox: *bbox })),
            Location::Polygon { polygon, .. } => out.push(P::Polygon(PolygonPart {
                polygon: polygon.clone(),
            })),
            Location::Line {
                column: Some(c),
                width_px,
                ..
            } => out.push(P::Column(ColumnPart {
                column: *c,
                width_px: *width_px,
            })),
            Location::Line {
                path: Some(path),
                width_px,
                ..
            } => out.push(P::Path(PathPart {
                path: path.clone(),
                width_px: *width_px,
            })),
            Location::Line { .. } => {
                return Err(Error::usage("a line with neither column nor path"));
            }
            Location::Stroke {
                path, radius_px, ..
            } => out.push(P::Stroke(StrokePart {
                path: path.clone(),
                radius_px: *radius_px,
            })),
            Location::WholeImage { .. } | Location::Absent { .. } => {}
            Location::View { .. } => {
                return Err(Error::usage(
                    "a view never leaves a tool (CONTRACT.md 9.2): its footprint does, as polygons",
                ));
            }
        }
    }
    let kind = match first {
        Location::Bbox { .. } => K::Bbox,
        Location::Polygon { .. } => K::Polygon,
        Location::Line { .. } => K::Line,
        Location::Stroke { .. } => K::Stroke,
        Location::WholeImage { .. } => K::WholeImage,
        Location::Absent { .. } => K::Absent,
        Location::View { .. } => K::View,
    };
    let straddles_wrap = out.len() > 1 && reference.topology == TmTopology::Wrap;
    Ok(table_manners::images::Location {
        image: reference,
        kind,
        parts: out,
        straddles_wrap,
        label: meta.label,
        disposition: None,
        confidence: None,
        evidence: None,
        score: meta.score,
        score_kind: meta.score_kind,
        reason: match kind {
            K::Absent => Some(meta.reason.unwrap_or_else(|| "not found".to_string())),
            _ => meta.reason,
        },
        source: meta.source,
    })
}
