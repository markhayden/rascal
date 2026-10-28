//! Typed artifacts: what ops take and return. Every artifact has a type, a local id and an
//! identity. REPLACED BY 1.4 (BandProfile, Series, Plan; the artifact store).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::Identity;
use crate::image::Image;
use crate::location::Location;
use crate::mask::Mask;
use crate::profile::BandProfile;
use crate::series::Series;

/// The artifact types a port can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
        }
    }
}
