//! Typed artifacts: what ops take and return. Every artifact has a type and an identity; the
//! workspace gives it a local id when it is committed.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::frame::Plan;
use crate::identity::Identity;
use crate::image::Image;
use crate::location::Location;
use crate::mask::Mask;
use crate::profile::BandProfile;
use crate::series::Series;

/// The artifact types a port can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ArtifactType {
    Image,
    Location,
    Mask,
    BandProfile,
    Series,
    Plan,
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ArtifactType::Image => "Image",
            ArtifactType::Location => "Location",
            ArtifactType::Mask => "Mask",
            ArtifactType::BandProfile => "BandProfile",
            ArtifactType::Series => "Series",
            ArtifactType::Plan => "Plan",
        })
    }
}

/// The schema version of every record rascal writes and reads (CONTRACT 3.3): 1 for each type.
pub const SCHEMA_VERSION: u32 = 1;

/// The tool name a contract type of rascal's own carries as its prefix (CONTRACT 3.3).
pub const TOOL: &str = "rascal";

impl ArtifactType {
    /// Every type, in the order the description lists them.
    pub const ALL: [ArtifactType; 6] = [
        ArtifactType::Image,
        ArtifactType::Location,
        ArtifactType::Mask,
        ArtifactType::BandProfile,
        ArtifactType::Series,
        ArtifactType::Plan,
    ];

    /// The contract's name for the type (CONTRACT 3.3): the shared `image` and `location`
    /// unprefixed, rascal's own as `rascal:<Name>`.
    pub fn contract_type(self) -> &'static str {
        match self {
            ArtifactType::Image => "image",
            ArtifactType::Location => "location",
            ArtifactType::Mask => "rascal:Mask",
            ArtifactType::BandProfile => "rascal:BandProfile",
            ArtifactType::Series => "rascal:Series",
            ArtifactType::Plan => "rascal:Plan",
        }
    }

    /// The type a contract name names, when it is one of rascal's: another tool's prefix, an
    /// unknown name and an unprefixed rascal name (`Mask`) are none.
    pub fn from_contract(name: &str) -> Option<ArtifactType> {
        ArtifactType::ALL
            .into_iter()
            .find(|t| t.contract_type() == name)
    }

    /// How an artifact of the type leaves a workspace (CONTRACT 3.3): `image` travels as a
    /// file, a location as JSON, a Mask as a bundle (ADR 0022); the rest cannot leave yet.
    pub fn portable(self) -> Portable {
        match self {
            ArtifactType::Image => Portable::File,
            ArtifactType::Location => Portable::Json,
            ArtifactType::Mask => Portable::Bundle,
            ArtifactType::BandProfile | ArtifactType::Series | ArtifactType::Plan => Portable::None,
        }
    }

    /// Whether rascal draws an artifact of the type as an image for people (CONTRACT 3.3, 4.7):
    /// a location and a Mask, each over the image it was made on, and a band profile as its band
    /// over the image with a chart of its half-width per row beside it. An image is its own
    /// picture; a series and a plan have no preview.
    pub fn preview(self) -> bool {
        matches!(
            self,
            ArtifactType::Location | ArtifactType::Mask | ArtifactType::BandProfile
        )
    }

    /// What an artifact of the type is, in a sentence for people (CONTRACT 3.3).
    pub fn purpose(self) -> &'static str {
        match self {
            ArtifactType::Image => "An image: pixels in a frame, passed by path or inline.",
            ArtifactType::Location => {
                "Where something is in an image: a box, a polygon, a line, a stroke or the whole image."
            }
            ArtifactType::Mask => {
                "A mask: what to change (core), how it fades (blend) and what to keep (protect), in the frame it was drawn in, with the band profile, sigmas and prescriptions it carries."
            }
            ArtifactType::BandProfile => {
                "A band along a line: its half-width per row, wide on smooth surfaces and thin on detail, from which a mask is drawn."
            }
            ArtifactType::Series => {
                "Values along a mask or a line, one per row or sample (widths, sigmas, measures)."
            }
            ArtifactType::Plan => {
                "How a cut canvas maps back onto the image it was cut from: the strips and where each lies."
            }
        }
    }
}

/// How an artifact of a type leaves the workspace it lives in (CONTRACT 3.3, and 10.1 for an
/// image, which the contract defines whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Portable {
    /// A file passed by path or inline (an image).
    File,
    /// A JSON document (a location).
    Json,
    /// A bundle the tool writes and reads whole (a Mask).
    Bundle,
    /// It cannot leave its workspace yet.
    None,
}

/// An artifact in memory. Images and masks are shared, not copied, between steps.
#[derive(Debug, Clone)]
pub enum Artifact {
    Image(Arc<Image>),
    Location {
        location: Location,
        identity: Identity,
    },
    Mask(Arc<Mask>),
    BandProfile(Arc<BandProfile>),
    Series(Arc<Series>),
    Plan(Arc<Plan>),
}

impl Artifact {
    /// The artifact's type.
    pub fn artifact_type(&self) -> ArtifactType {
        match self {
            Artifact::Image(_) => ArtifactType::Image,
            Artifact::Location { .. } => ArtifactType::Location,
            Artifact::Mask(_) => ArtifactType::Mask,
            Artifact::BandProfile(_) => ArtifactType::BandProfile,
            Artifact::Series(_) => ArtifactType::Series,
            Artifact::Plan(_) => ArtifactType::Plan,
        }
    }

    /// The artifact's identity.
    pub fn identity(&self) -> Identity {
        match self {
            Artifact::Image(image) => image.identity(),
            Artifact::Location { identity, .. } => identity.clone(),
            Artifact::Mask(mask) => mask.identity(),
            Artifact::BandProfile(profile) => profile.identity(),
            Artifact::Series(series) => series.identity(),
            Artifact::Plan(plan) => Identity::parse_or_hash(&plan.identity),
        }
    }

    /// The frame id the artifact is in.
    pub fn frame(&self) -> &str {
        match self {
            Artifact::Image(image) => &image.record.frame,
            Artifact::Location { location, .. } => location.frame(),
            Artifact::Mask(mask) => &mask.record.frame,
            Artifact::BandProfile(profile) => &profile.frame,
            Artifact::Series(series) => &series.frame,
            Artifact::Plan(plan) => &plan.parent,
        }
    }
}
