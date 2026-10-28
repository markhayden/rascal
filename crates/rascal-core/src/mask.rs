//! The mask record: `mask.toml` plus the `core`, `blend` and `protect` planes, and the one
//! effective-weight rule every rendering, prescription, refinement and paste uses.
//!
//! Task 1.1 writes and reads the zones and the source. REPLACED BY 1.3 (profile,
//! prescriptions, provenance, refinement postconditions).

use std::path::{Path, PathBuf};

use image::GrayImage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::identity::Identity;

/// The use case a mask was made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UseCase {
    Seam,
    Remove,
    Repair,
    Protect,
    Custom,
}

/// The plane files, relative to `mask.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Zones {
    /// Binary: 255 where a filler may paint; paste weight 1.
    pub core: String,
    /// 8-bit fractional paste weights outside the core; 0 inside it and beyond the feather.
    pub blend: String,
    /// Binary: 255 where nothing may change; effective weight 0 wherever it is set.
    pub protect: String,
}

/// What the mask was made from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Identities of the locations.
    #[serde(default)]
    pub locations: Vec<String>,
    /// Identities of the finder instances.
    #[serde(default)]
    pub instances: Vec<String>,
}

/// `mask.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskRecord {
    /// The record's version.
    pub version: u32,
    /// The local id.
    pub id: String,
    /// The frame id the planes are in.
    pub frame: String,
    /// The frame's identity.
    pub frame_identity: String,
    pub use_case: UseCase,
    /// `sha256:` over the three planes' pixels, the use case, the frame identity and the
    /// source identities.
    pub identity: String,
    pub zones: Zones,
    #[serde(default)]
    pub source: Source,
}

/// A mask: the record and its three planes, each the size of the frame.
#[derive(Debug, Clone)]
pub struct Mask {
    pub record: MaskRecord,
    pub core: GrayImage,
    pub blend: GrayImage,
    pub protect: GrayImage,
}

impl Mask {
    /// A mask with the given core, an empty blend and an empty protect.
    pub fn from_core(
        id: &str,
        frame: &Frame,
        use_case: UseCase,
        core: GrayImage,
        source: Source,
    ) -> Result<Mask> {
        if core.dimensions() != (frame.width, frame.height) {
            return Err(Error::op(
                "mask",
                format!(
                    "core plane is {}x{} but the frame {} is {}x{}",
                    core.width(),
                    core.height(),
                    frame.id,
                    frame.width,
                    frame.height
                ),
            ));
        }
        let blend = GrayImage::new(frame.width, frame.height);
        let protect = GrayImage::new(frame.width, frame.height);
        let identity = mask_identity(&core, &blend, &protect, use_case, &frame.identity, &source);
        Ok(Mask {
            record: MaskRecord {
                version: 1,
                id: id.to_string(),
                frame: frame.id.clone(),
                frame_identity: frame.identity.clone(),
                use_case,
                identity: identity.as_str().to_string(),
                zones: Zones {
                    core: "core.png".to_string(),
                    blend: "blend.png".to_string(),
                    protect: "protect.png".to_string(),
                },
                source,
            },
            core,
            blend,
            protect,
        })
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.record.identity)
    }

    /// Width and height.
    pub fn dimensions(&self) -> (u32, u32) {
        self.core.dimensions()
    }

    /// The effective weight of a pixel: 0 where protected, else 1 in the core, else blend/255.
    pub fn weight(&self, x: u32, y: u32) -> f32 {
        if self.protect.get_pixel(x, y)[0] != 0 {
            0.0
        } else if self.core.get_pixel(x, y)[0] != 0 {
            1.0
        } else {
            f32::from(self.blend.get_pixel(x, y)[0]) / 255.0
        }
    }

    /// True when no pixel has a nonzero weight.
    pub fn is_empty(&self) -> bool {
        let (w, h) = self.dimensions();
        (0..h).all(|y| (0..w).all(|x| self.weight(x, y) == 0.0))
    }

    /// The directory name a mask is stored under: `<identity hex>.mask`.
    pub fn dir_name(&self) -> String {
        format!("{}.mask", self.identity().hex())
    }

    /// Writes `mask.toml` and the three planes into `dir` (created if needed).
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        for (name, plane) in [
            ("core.png", &self.core),
            ("blend.png", &self.blend),
            ("protect.png", &self.protect),
        ] {
            let path = dir.join(name);
            plane
                .save_with_format(&path, image::ImageFormat::Png)
                .map_err(|source| Error::Image { path, source })?;
        }
        let path = dir.join("mask.toml");
        let text = toml::to_string_pretty(&self.record)
            .map_err(|e| Error::usage(format!("{}: {e}", path.display())))?;
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))
    }

    /// Reads a mask directory.
    pub fn read(dir: &Path) -> Result<Mask> {
        let path = dir.join("mask.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let record: MaskRecord = toml::from_str(&text).map_err(|source| Error::Toml {
            path: path.clone(),
            source: Box::new(source),
        })?;
        let plane = |name: &str| -> Result<GrayImage> {
            let p = dir.join(name);
            Ok(image::open(&p)
                .map_err(|source| Error::Image { path: p, source })?
                .to_luma8())
        };
        let core = plane(&record.zones.core)?;
        let blend = plane(&record.zones.blend)?;
        let protect = plane(&record.zones.protect)?;
        if core.dimensions() != blend.dimensions() || core.dimensions() != protect.dimensions() {
            return Err(Error::op(
                "mask",
                format!("{}: the planes differ in size", dir.display()),
            ));
        }
        Ok(Mask {
            record,
            core,
            blend,
            protect,
        })
    }

    /// The path of a mask directory for a record read elsewhere.
    pub fn path_in(&self, artifacts: &Path) -> PathBuf {
        artifacts.join(self.dir_name())
    }
}

/// The three planes' pixels (as pixel digests), the use case, the frame identity and the
/// source identities.
fn mask_identity(
    core: &GrayImage,
    blend: &GrayImage,
    protect: &GrayImage,
    use_case: UseCase,
    frame_identity: &str,
    source: &Source,
) -> Identity {
    let digest = |plane: &GrayImage| {
        canonical::pixel_digest(plane.width(), plane.height(), 1, plane.as_raw())
    };
    canonical::identity_of(
        "mask",
        &serde_json::json!({
            "core": digest(core).as_str(),
            "blend": digest(blend).as_str(),
            "protect": digest(protect).as_str(),
            "use_case": use_case,
            "frame": frame_identity,
            "source": source,
        }),
    )
}
