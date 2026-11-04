//! rascal-ops's integration tests in one binary: each file here is a module, so the dependency
//! graph is linked once, not once per file. `cargo nextest run` runs each test in its own
//! process.

mod cut_put;
mod exactly_one_of;
mod fan;
mod fan_thin_widths;
mod frames;
mod groups;
mod grow_shrink_close;
mod measure;
mod paste;
mod poisson;
mod primitives;
mod profile_feather;
mod ramp_apply;
mod rasterize;
mod render;
mod subtract_select_invert;
mod tighten;
mod window;
