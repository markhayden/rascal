//! The rascal operations: the pixel primitives, every op in the catalogue as a pure function
//! plus a thin `Op` implementation, the solvers, and the embedded constant sets.
//!
//! Task 1.1 brings `rect`, `rasterize` and `render` in their walking-slice forms; the
//! primitives arrive in 1.8a and the rest of the catalogue from M2 on.

pub mod apply;
pub mod cut;
pub mod fan;
pub mod feather;
pub mod inspect;
pub mod line;
pub mod paste;
pub mod primitives;
pub mod profile;
pub mod put;
pub mod ramp;
pub mod rasterize;
pub mod rect;
pub mod render;
pub mod stroke;
pub mod unwindow;
pub mod window;

use rascal_core::registry::OpRegistry;

/// Registers every op this crate ships; a manifest that fails its check is an error.
pub fn register_all(registry: &mut OpRegistry) -> rascal_core::error::Result<()> {
    registry.register(Box::new(rect::Rect))?;
    registry.register(Box::new(rasterize::Rasterize))?;
    registry.register(Box::new(render::Render))?;
    registry.register(Box::new(cut::Cut))?;
    registry.register(Box::new(put::Put))?;
    registry.register(Box::new(line::Line))?;
    registry.register(Box::new(stroke::Stroke))?;
    registry.register(Box::new(inspect::Inspect))?;
    registry.register(Box::new(fan::Fan))?;
    registry.register(Box::new(profile::Profile))?;
    registry.register(Box::new(feather::Feather))?;
    registry.register(Box::new(ramp::Ramp))?;
    registry.register(Box::new(apply::Apply))?;
    registry.register(Box::new(window::Window))?;
    registry.register(Box::new(unwindow::Unwindow))?;
    registry.register(Box::new(paste::Paste))?;
    Ok(())
}

/// A registry holding the whole catalogue.
pub fn registry() -> rascal_core::error::Result<OpRegistry> {
    let mut registry = OpRegistry::new();
    register_all(&mut registry)?;
    Ok(registry)
}

/// Reads a point-list parameter.
pub(crate) fn points(
    params: &toml::Table,
    name: &str,
    op: &str,
) -> rascal_core::error::Result<Vec<[f64; 2]>> {
    let bad = || {
        rascal_core::Error::op(
            op,
            format!("parameter {name} must be a list of [x, y] points"),
        )
    };
    let items = params
        .get(name)
        .and_then(|v| v.as_array())
        .ok_or_else(bad)?;
    items
        .iter()
        .map(|p| {
            let pair = p.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
            let n = |v: &toml::Value| {
                v.as_float()
                    .or_else(|| v.as_integer().map(|i| i as f64))
                    .ok_or_else(bad)
            };
            Ok([n(&pair[0])?, n(&pair[1])?])
        })
        .collect()
}

/// Reads an optional float parameter.
pub(crate) fn float(params: &toml::Table, name: &str) -> Option<f64> {
    params
        .get(name)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

/// Reads a parameter as a float array of fixed length.
pub(crate) fn float_array<const N: usize>(
    params: &toml::Table,
    name: &str,
    op: &str,
) -> rascal_core::error::Result<[f64; N]> {
    let value = params
        .get(name)
        .ok_or_else(|| rascal_core::Error::op(op, format!("parameter {name} is required")))?;
    let items = value.as_array().ok_or_else(|| {
        rascal_core::Error::op(
            op,
            format!("parameter {name} must be an array of {N} numbers"),
        )
    })?;
    let numbers: Vec<f64> = items
        .iter()
        .map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .collect::<Option<_>>()
        .ok_or_else(|| {
            rascal_core::Error::op(
                op,
                format!("parameter {name} must be an array of {N} numbers"),
            )
        })?;
    numbers.try_into().map_err(|_| {
        rascal_core::Error::op(
            op,
            format!("parameter {name} must have exactly {N} numbers"),
        )
    })
}
