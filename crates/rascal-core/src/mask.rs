//! The mask record: `mask.toml` plus the `core`, `blend` and `protect` planes, the optional
//! profile and prescriptions, the source and the provenance, and the one effective-weight rule
//! every rendering, prescription, refinement and paste uses (SPEC.md "The mask record").
//!
//! A mask is a directory, `<identity hex>.mask/`, self-contained: the planes, the band profile
//! and every series it references live inside it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::GrayImage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::identity::Identity;
use crate::profile::BandProfile;
use crate::series::Series;

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
    /// 8-bit fractional paste weights outside the core; 0 inside it, under protect and beyond
    /// the feather.
    pub blend: String,
    /// Binary: 255 where nothing may change; effective weight 0 wherever it is set.
    pub protect: String,
}

/// The `[profile]` section: present when the mask is a band along a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileSection {
    /// The identity of the `line` location.
    pub line: String,
    /// The `BandProfile` file, relative to `mask.toml`.
    pub band: String,
    /// The per-row feather sigma `Series` file, relative to `mask.toml`.
    pub sigma_px: String,
}

/// A pixel change that belongs with the mask; `apply` performs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Prescription {
    /// A per-row colour step carried across the band.
    ToneRamp {
        version: u32,
        /// The step `Series` file (3 channels, levels), relative to `mask.toml`.
        step: String,
        /// The share of the step each side moves toward the other.
        share: f64,
    },
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

/// One step that made or changed the mask: a copy of the journal's node, not part of identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceNode {
    pub step: String,
    pub op: String,
    pub version: String,
    /// The resolved parameters.
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
    /// Input identities by port.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    /// Output identities by port.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// The constant set, when the op used one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constants: Option<String>,
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
    /// The planes' size, which is the frame's.
    pub width: u32,
    pub height: u32,
    pub use_case: UseCase,
    /// `sha256:` over the three planes' pixels, the use case, the frame identity, the
    /// profile's line, band and sigma identities, every prescription with its series replaced
    /// by the series' identity, and the source identities. Ids, file names and provenance are
    /// not part of it.
    pub identity: String,
    pub zones: Zones,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prescriptions: Vec<Prescription>,
    #[serde(default)]
    pub source: Source,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ProvenanceNode>,
}

/// The profile in memory: the band and the per-row sigma.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub line: String,
    pub band: BandProfile,
    pub sigma_px: Series,
}

/// A prescription in memory, with its series loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadedPrescription {
    ToneRamp {
        version: u32,
        step: Series,
        share: f64,
    },
}

/// What a refinement reports about what it cleared. Enters the envelope with task 1.4, which
/// gives it a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refinement {
    pub blend_cleared: bool,
    pub profile_cleared: bool,
}

/// A mask: the record, its three planes, and the profile and prescriptions with their files
/// loaded. Every constructor and mutator keeps the invariants and the identity current.
#[derive(Debug, Clone)]
pub struct Mask {
    pub record: MaskRecord,
    pub core: GrayImage,
    pub blend: GrayImage,
    pub protect: GrayImage,
    pub profile: Option<Profile>,
    pub prescriptions: Vec<LoadedPrescription>,
}

const BAND_FILE: &str = "band.profile.json";
const SIGMA_FILE: &str = "sigma.series.json";

/// The effective weight of one pixel: 0 where protected, else 1 in the core, else blend/255.
pub fn effective_weight(core: u8, blend: u8, protect: u8) -> f32 {
    if protect != 0 {
        0.0
    } else if core != 0 {
        1.0
    } else {
        f32::from(blend) / 255.0
    }
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
        let mut mask = Mask {
            record: MaskRecord {
                version: 1,
                id: id.to_string(),
                frame: frame.id.clone(),
                frame_identity: frame.identity.clone(),
                width: frame.width,
                height: frame.height,
                use_case,
                identity: String::new(),
                zones: Zones {
                    core: "core.png".to_string(),
                    blend: "blend.png".to_string(),
                    protect: "protect.png".to_string(),
                },
                profile: None,
                prescriptions: Vec::new(),
                source,
                provenance: Vec::new(),
            },
            core: binarize(core),
            blend: GrayImage::new(frame.width, frame.height),
            protect: GrayImage::new(frame.width, frame.height),
            profile: None,
            prescriptions: Vec::new(),
        };
        mask.refresh_identity();
        Ok(mask)
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.record.identity)
    }

    /// Width and height.
    pub fn dimensions(&self) -> (u32, u32) {
        self.core.dimensions()
    }

    /// The effective weight of a pixel.
    pub fn weight(&self, x: u32, y: u32) -> f32 {
        effective_weight(
            self.core.get_pixel(x, y)[0],
            self.blend.get_pixel(x, y)[0],
            self.protect.get_pixel(x, y)[0],
        )
    }

    /// True when no pixel has a nonzero weight.
    pub fn is_empty(&self) -> bool {
        let (w, h) = self.dimensions();
        (0..h).all(|y| (0..w).all(|x| self.weight(x, y) == 0.0))
    }

    /// The integer bbox `[x0, y0, x1, y1)` of pixels with `w > 0`, or `None` when empty.
    pub fn support(&self) -> Option<[u32; 4]> {
        let (w, h) = self.dimensions();
        let mut bbox: Option<[u32; 4]> = None;
        for y in 0..h {
            for x in 0..w {
                if self.weight(x, y) > 0.0 {
                    bbox = Some(match bbox {
                        None => [x, y, x + 1, y + 1],
                        Some([x0, y0, x1, y1]) => {
                            [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)]
                        }
                    });
                }
            }
        }
        bbox
    }

    /// Counts per zone, for `inspect --json`.
    pub fn statistics(&self) -> ZoneStatistics {
        let (w, h) = self.dimensions();
        let mut s = ZoneStatistics {
            width: w,
            height: h,
            core: 0,
            blend: 0,
            protect: 0,
            painted: 0,
            weight_sum: 0.0,
            support: self.support(),
        };
        for y in 0..h {
            for x in 0..w {
                if self.core.get_pixel(x, y)[0] != 0 {
                    s.core += 1;
                }
                if self.blend.get_pixel(x, y)[0] != 0 {
                    s.blend += 1;
                }
                if self.protect.get_pixel(x, y)[0] != 0 {
                    s.protect += 1;
                }
                let wgt = self.weight(x, y);
                if wgt > 0.0 {
                    s.painted += 1;
                    s.weight_sum += f64::from(wgt);
                }
            }
        }
        s
    }

    // --- refinements ---------------------------------------------------------------------------

    /// Replaces the core (`grow`, `shrink`, `close`, `select`, `tighten`, `rasterize`):
    /// `blend = 0` everywhere, the profile and every prescription removed (they were derived
    /// from the old support), protect preserved and still winning.
    pub fn replace_core(&mut self, core: GrayImage) -> Result<Refinement> {
        if core.dimensions() != self.dimensions() {
            return Err(Error::op("mask", "the new core has a different size"));
        }
        let report = Refinement {
            blend_cleared: true,
            profile_cleared: self.profile.is_some() || !self.prescriptions.is_empty(),
        };
        self.core = binarize(core);
        self.blend = GrayImage::new(self.record.width, self.record.height);
        self.profile = None;
        self.record.profile = None;
        self.prescriptions.clear();
        self.record.prescriptions.clear();
        self.refresh_identity();
        Ok(report)
    }

    /// Sets the blend (`profile`, `feather`): zero is forced inside the core and under protect.
    pub fn set_blend(&mut self, blend: GrayImage) -> Result<()> {
        if blend.dimensions() != self.dimensions() {
            return Err(Error::op("mask", "the new blend has a different size"));
        }
        let mut blend = blend;
        for (x, y, p) in blend.enumerate_pixels_mut() {
            if self.core.get_pixel(x, y)[0] != 0 || self.protect.get_pixel(x, y)[0] != 0 {
                p[0] = 0;
            }
        }
        self.blend = blend;
        self.refresh_identity();
        Ok(())
    }

    /// Adds to protect (`subtract`): core and blend are cleared under it; the profile and the
    /// prescriptions stay (`apply` honours the new protect).
    pub fn protect_union(&mut self, plane: &GrayImage) -> Result<()> {
        if plane.dimensions() != self.dimensions() {
            return Err(Error::op("mask", "the protect plane has a different size"));
        }
        for (x, y, p) in plane.enumerate_pixels() {
            if p[0] != 0 {
                self.protect.put_pixel(x, y, image::Luma([255]));
                self.core.put_pixel(x, y, image::Luma([0]));
                self.blend.put_pixel(x, y, image::Luma([0]));
            }
        }
        self.refresh_identity();
        Ok(())
    }

    /// Sets the profile (`profile`). The band and the sigma series must be in the mask's frame
    /// and have one entry per row.
    pub fn set_profile(
        &mut self,
        line_identity: &str,
        band: BandProfile,
        sigma_px: Series,
    ) -> Result<()> {
        if band.frame_identity != self.record.frame_identity
            || sigma_px.frame_identity != self.record.frame_identity
        {
            return Err(Error::op("mask", "the profile is not in the mask's frame"));
        }
        if band.half.len() != self.record.height as usize
            || sigma_px.rows.len() != self.record.height as usize
        {
            return Err(Error::op(
                "mask",
                "the profile does not have one entry per row",
            ));
        }
        self.record.profile = Some(ProfileSection {
            line: line_identity.to_string(),
            band: BAND_FILE.to_string(),
            sigma_px: SIGMA_FILE.to_string(),
        });
        self.profile = Some(Profile {
            line: line_identity.to_string(),
            band,
            sigma_px,
        });
        self.refresh_identity();
        Ok(())
    }

    /// Adds a tone-ramp prescription (`ramp`).
    pub fn push_tone_ramp(&mut self, step: Series, share: f64) -> Result<()> {
        if step.frame_identity != self.record.frame_identity {
            return Err(Error::op(
                "mask",
                "the ramp series is not in the mask's frame",
            ));
        }
        if step.channels != 3 || step.rows.len() != self.record.height as usize {
            return Err(Error::op(
                "mask",
                "the ramp series needs 3 channels and one entry per row",
            ));
        }
        if !(share.is_finite() && (0.0..=1.0).contains(&share)) {
            return Err(Error::op(
                "mask",
                format!("share must be in [0, 1], got {share}"),
            ));
        }
        let n = self.prescriptions.len();
        self.record.prescriptions.push(Prescription::ToneRamp {
            version: 1,
            step: format!("prescription-{n}.series.json"),
            share,
        });
        self.prescriptions.push(LoadedPrescription::ToneRamp {
            version: 1,
            step,
            share,
        });
        self.refresh_identity();
        Ok(())
    }

    /// Appends a provenance node; not part of identity.
    pub fn append_provenance(&mut self, node: ProvenanceNode) {
        self.record.provenance.push(node);
    }

    /// Renames the local id; not part of identity.
    pub fn with_id(mut self, id: &str) -> Mask {
        self.record.id = id.to_string();
        self
    }

    // --- identity and invariants ----------------------------------------------------------------

    /// Recomputes the identity from the content.
    fn refresh_identity(&mut self) {
        self.record.identity = self.compute_identity().as_str().to_string();
    }

    /// The identity over the content, references by identity.
    pub fn compute_identity(&self) -> Identity {
        let digest = |plane: &GrayImage| {
            canonical::pixel_digest(plane.width(), plane.height(), 1, plane.as_raw())
                .as_str()
                .to_string()
        };
        let profile = self.profile.as_ref().map(|p| {
            serde_json::json!({ "line": p.line, "band": p.band.identity, "sigma_px": p.sigma_px.identity })
        });
        let prescriptions: Vec<serde_json::Value> = self
            .prescriptions
            .iter()
            .map(|p| match p {
                LoadedPrescription::ToneRamp { version, step, share } => {
                    serde_json::json!({ "kind": "tone_ramp", "version": version, "step": step.identity, "share": share })
                }
            })
            .collect();
        canonical::identity_of(
            "mask",
            &serde_json::json!({
                "core": digest(&self.core),
                "blend": digest(&self.blend),
                "protect": digest(&self.protect),
                "use_case": self.record.use_case,
                "frame": self.record.frame_identity,
                "profile": profile,
                "prescriptions": prescriptions,
                "source": self.record.source,
            }),
        )
    }

    /// Every invariant of a mask in memory. Named so a refusal says which one.
    pub fn check(&self) -> Result<()> {
        let (w, h) = self.dimensions();
        let fail = |invariant: &str| Error::MaskInvariant {
            id: self.record.id.clone(),
            invariant: invariant.to_string(),
        };
        if (w, h) != (self.record.width, self.record.height) {
            return Err(fail("the planes are not the record's size"));
        }
        if self.blend.dimensions() != (w, h) || self.protect.dimensions() != (w, h) {
            return Err(fail("the three planes differ in size"));
        }
        for y in 0..h {
            for x in 0..w {
                let (c, b, p) = (
                    self.core.get_pixel(x, y)[0],
                    self.blend.get_pixel(x, y)[0],
                    self.protect.get_pixel(x, y)[0],
                );
                if c != 0 && c != 255 {
                    return Err(fail(&format!("core is binary (value {c} at {x},{y})")));
                }
                if p != 0 && p != 255 {
                    return Err(fail(&format!("protect is binary (value {p} at {x},{y})")));
                }
                if b != 0 && c != 0 {
                    return Err(fail(&format!(
                        "blend is zero inside the core (value {b} at {x},{y})"
                    )));
                }
                if b != 0 && p != 0 {
                    return Err(fail(&format!(
                        "blend is zero under protect (value {b} at {x},{y})"
                    )));
                }
            }
        }
        if let Some(profile) = &self.profile {
            profile.band.check()?;
            profile.sigma_px.check()?;
            if profile.band.frame_identity != self.record.frame_identity
                || profile.sigma_px.frame_identity != self.record.frame_identity
            {
                return Err(fail("the profile is in the mask's frame"));
            }
            if profile.band.half.len() != h as usize || profile.sigma_px.rows.len() != h as usize {
                return Err(fail("the profile has one entry per row"));
            }
            if self.record.profile.as_ref().map(|p| p.line.as_str()) != Some(profile.line.as_str())
            {
                return Err(fail("the record's profile matches the loaded profile"));
            }
        } else if self.record.profile.is_some() {
            return Err(fail("the record names a profile that is not loaded"));
        }
        if self.record.prescriptions.len() != self.prescriptions.len() {
            return Err(fail("every prescription's series is loaded"));
        }
        for p in &self.prescriptions {
            match p {
                LoadedPrescription::ToneRamp { step, .. } => {
                    step.check()?;
                    if step.frame_identity != self.record.frame_identity
                        || step.channels != 3
                        || step.rows.len() != h as usize
                    {
                        return Err(fail(
                            "a tone ramp series has 3 channels, one row each, in the mask's frame",
                        ));
                    }
                }
            }
        }
        if self.compute_identity().as_str() != self.record.identity {
            return Err(fail("the stored identity matches the content"));
        }
        Ok(())
    }

    // --- files ---------------------------------------------------------------------------------

    /// The directory name a mask is stored under: `<identity hex>.mask`.
    pub fn dir_name(&self) -> String {
        format!("{}.mask", self.identity().hex())
    }

    /// The path of this mask under an artifacts directory.
    pub fn path_in(&self, artifacts: &Path) -> PathBuf {
        artifacts.join(self.dir_name())
    }

    /// Writes `mask.toml`, the three planes, the profile files and every prescription's series
    /// into `dir` (created if needed).
    pub fn write(&self, dir: &Path) -> Result<()> {
        self.check()?;
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        for (name, plane) in [
            (&self.record.zones.core, &self.core),
            (&self.record.zones.blend, &self.blend),
            (&self.record.zones.protect, &self.protect),
        ] {
            canonical::write_gray(&dir.join(name), plane)?;
        }
        if let (Some(section), Some(profile)) = (&self.record.profile, &self.profile) {
            profile.band.write(&dir.join(&section.band))?;
            profile.sigma_px.write(&dir.join(&section.sigma_px))?;
        }
        for (record, loaded) in self.record.prescriptions.iter().zip(&self.prescriptions) {
            match (record, loaded) {
                (
                    Prescription::ToneRamp { step: file, .. },
                    LoadedPrescription::ToneRamp { step, .. },
                ) => step.write(&dir.join(file))?,
            }
        }
        let path = dir.join("mask.toml");
        let text = toml::to_string_pretty(&self.record)
            .map_err(|e| Error::usage(format!("{}: {e}", path.display())))?;
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))
    }

    /// Reads a mask directory and checks every invariant, naming the one that fails.
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
        let profile = match &record.profile {
            Some(section) => Some(Profile {
                line: section.line.clone(),
                band: BandProfile::read(&dir.join(&section.band))?,
                sigma_px: Series::read(&dir.join(&section.sigma_px))?,
            }),
            None => None,
        };
        let mut prescriptions = Vec::new();
        for p in &record.prescriptions {
            match p {
                Prescription::ToneRamp {
                    version,
                    step,
                    share,
                } => prescriptions.push(LoadedPrescription::ToneRamp {
                    version: *version,
                    step: Series::read(&dir.join(step))?,
                    share: *share,
                }),
            }
        }
        let mask = Mask {
            record,
            core,
            blend,
            protect,
            profile,
            prescriptions,
        };
        mask.check()?;
        Ok(mask)
    }
}

/// Counts per zone, for `inspect --json`; task 1.9 gives the inspect output its schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneStatistics {
    pub width: u32,
    pub height: u32,
    /// Pixels in the core.
    pub core: u64,
    /// Pixels with a nonzero blend.
    pub blend: u64,
    /// Protected pixels.
    pub protect: u64,
    /// Pixels with `w > 0`.
    pub painted: u64,
    /// The sum of effective weights.
    pub weight_sum: f64,
    /// The integer bbox of `w > 0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<[u32; 4]>,
}

/// Any nonzero sample becomes 255.
fn binarize(mut plane: GrayImage) -> GrayImage {
    for p in plane.pixels_mut() {
        if p[0] != 0 {
            p[0] = 255;
        }
    }
    plane
}
