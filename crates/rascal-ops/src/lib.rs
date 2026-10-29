//! The rascal operations: the pixel primitives, every op in the catalogue as a pure function
//! plus a thin `Op` implementation, the solvers, and the embedded constant sets.
//!
//! `refine` holds what every core-replacing refinement (`grow`, `shrink`, `close`, later
//! `select` and `tighten`) shares.

pub mod apply;
pub mod close;
pub mod cut;
pub mod fan;
pub mod feather;
pub mod grow;
pub mod inspect;
pub mod invert;
pub mod line;
pub mod measure;
pub mod paste;
pub mod primitives;
pub mod profile;
pub mod put;
pub mod ramp;
pub mod rasterize;
pub mod rect;
pub(crate) mod refine;
pub mod render;
pub mod select;
pub mod shrink;
pub mod stroke;
pub mod subtract;
pub mod tighten;
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
    registry.register(Box::new(measure::Measure))?;
    registry.register(Box::new(grow::Grow))?;
    registry.register(Box::new(shrink::Shrink))?;
    registry.register(Box::new(close::Close))?;
    registry.register(Box::new(subtract::Subtract))?;
    registry.register(Box::new(select::Select))?;
    registry.register(Box::new(invert::Invert))?;
    registry.register(Box::new(tighten::Tighten))?;
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

/// Reads an integer-list parameter; absent is empty.
pub(crate) fn integers(params: &toml::Table, name: &str) -> Vec<i64> {
    params
        .get(name)
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|i| i.as_integer()).collect())
        .unwrap_or_default()
}

/// Reads a box-list parameter; absent is empty.
pub(crate) fn boxes(params: &toml::Table, name: &str) -> Vec<[f64; 4]> {
    params
        .get(name)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|b| {
                    let a = b.as_array()?;
                    let n =
                        |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
                    Some([n(a.first()?)?, n(a.get(1)?)?, n(a.get(2)?)?, n(a.get(3)?)?])
                })
                .collect()
        })
        .unwrap_or_default()
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
