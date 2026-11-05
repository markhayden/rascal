//! `plug`: the core's small holes filled (SPEC.md "Self-correcting remove", part 1). A hole is a
//! 4-connected region of unset pixels that touches no image edge; its owner is the largest
//! 8-connected blob of the core beside it (ties by the blob's first pixel in row-major order),
//! counted by the blob's own pixels. A hole is filled when its area is at most `max_share` of its
//! owner's. Every label and area is taken from the input before anything fills, so the order
//! of filling changes nothing. A refinement like `close`: the blend is cleared, the profile and
//! the prescriptions removed, and protect preserved (a protected hole stays empty).

use std::collections::VecDeque;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::refine::replaced_core;

/// The op.
pub struct Plug;

const NAME: &str = "plug";

impl Op for Plug {
    fn group(&self) -> rascal_core::op::Group {
        rascal_core::op::Group::Refine
    }

    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "Fills the core's small holes: every enclosed gap no larger than max_share of the shape around it.",
            "On a mask with gaps inside an object (the leather inside a buckle, a ring's middle), after grow and close, before subtract and feather.",
            "To keep a large enclosed area out of the mask (a picture inside a frame stays out at the default), or to make the core larger overall (use grow).",
            "The same mask with each enclosed hole filled whose area is at most max_share of the largest blob beside it; holes touching the image edge are never filled; cleared under protect. The blend is cleared and the profile and prescriptions removed: feather it afterwards.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask whose holes to fill.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with its small holes filled.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "max_share",
                    "The largest hole filled, as a share of the area of the shape around it.",
                    ParamKind::Number,
                    serde_json::json!(0.1),
                )
                .bounded(0.0, 1.0),
            ],
        )
    }

    fn run(&self, _: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let share = crate::float(params, "max_share")
            .ok_or_else(|| Error::op(NAME, "parameter max_share"))?;
        let plugged = plug(&mask.core, share);
        // Pixels a fill set that protect does not take back (replaced_core clears under protect).
        let px = plugged
            .core
            .as_raw()
            .iter()
            .zip(mask.core.as_raw())
            .zip(mask.protect.as_raw())
            .filter(|((new, old), protect)| **new != 0 && **old == 0 && **protect == 0)
            .count();
        let frame = &mask.record.frame;
        Ok(replaced_core(
            mask,
            plugged.core,
            "plug.mask",
            format!(
                "{} of {} holes filled ({} px) at max_share {share}",
                plugged.filled, plugged.found, px
            ),
        )?
        .with_measurement("max_share", share, "ratio", Some(frame))
        .with_measurement("holes_found", plugged.found as f64, "count", Some(frame))
        .with_measurement("holes_filled", plugged.filled as f64, "count", Some(frame))
        .with_measurement("plugged_px", px as f64, "px", Some(frame)))
    }
}

/// What [`plug`] did.
pub struct Plugged {
    /// The core with the qualifying holes set.
    pub core: GrayImage,
    /// Holes found (enclosed, not touching the edge).
    pub found: usize,
    /// Holes filled.
    pub filled: usize,
}

/// Component labels over the pixels where `want(index)` holds, 8- or 4-connected: label 0 is
/// "not in the set"; labels count up in row-major order of each component's first pixel.
fn components(
    width: usize,
    height: usize,
    want: impl Fn(usize) -> bool,
    eight: bool,
) -> (Vec<u32>, Vec<usize>) {
    let mut label = vec![0u32; width * height];
    let mut areas = vec![0usize];
    let mut queue = VecDeque::new();
    for start in 0..width * height {
        if label[start] != 0 || !want(start) {
            continue;
        }
        let id = areas.len() as u32;
        areas.push(0);
        label[start] = id;
        queue.push_back(start);
        while let Some(k) = queue.pop_front() {
            areas[id as usize] += 1;
            let (x, y) = ((k % width) as i64, (k / width) as i64);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if (dx == 0 && dy == 0) || (!eight && dx != 0 && dy != 0) {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                        continue;
                    }
                    let n = ny as usize * width + nx as usize;
                    if label[n] == 0 && want(n) {
                        label[n] = id;
                        queue.push_back(n);
                    }
                }
            }
        }
    }
    (label, areas)
}

/// The core with every hole of at most `max_share` of its owner's area filled.
pub fn plug(core: &GrayImage, max_share: f64) -> Plugged {
    let (width, height) = (core.width() as usize, core.height() as usize);
    let raw = core.as_raw();
    let on = |k: usize| raw[k] != 0;
    let (blob, blob_area) = components(width, height, on, true);
    let (gap, gap_area) = components(width, height, |k| !on(k), false);
    let holes = gap_area.len() - 1;
    // Per gap: does it touch the edge, and its owner (the largest adjacent blob, ties by the
    // lower label, which is the earlier first pixel).
    let mut edge = vec![false; holes + 1];
    let mut owner = vec![0u32; holes + 1];
    for (k, &label) in gap.iter().enumerate() {
        let g = label as usize;
        if g == 0 {
            continue;
        }
        let (x, y) = (k % width, k / width);
        if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
            edge[g] = true;
        }
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                    continue;
                }
                let b = blob[ny as usize * width + nx as usize];
                if b == 0 {
                    continue;
                }
                let current = owner[g];
                let better = current == 0
                    || blob_area[b as usize] > blob_area[current as usize]
                    || (blob_area[b as usize] == blob_area[current as usize] && b < current);
                if better {
                    owner[g] = b;
                }
            }
        }
    }
    let fill: Vec<bool> = (0..=holes)
        .map(|g| {
            g != 0
                && !edge[g]
                && owner[g] != 0
                && gap_area[g] as f64 <= max_share * blob_area[owner[g] as usize] as f64
        })
        .collect();
    let found = (1..=holes).filter(|&g| !edge[g]).count();
    let filled = fill.iter().filter(|&&f| f).count();
    let mut out = core.clone();
    for (k, &label) in gap.iter().enumerate() {
        if fill[label as usize] {
            out.put_pixel((k % width) as u32, (k / width) as u32, Luma([255]));
        }
    }
    Plugged {
        core: out,
        found,
        filled,
    }
}
