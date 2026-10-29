//! The shipped recipes, embedded by name, and the loader `run`, `heal` and the lab share.

use std::path::Path;

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::recipe::Recipe;

/// The recipes embedded in the binary, by name.
pub const SHIPPED: &[(&str, &str)] = &[
    ("rect", include_str!("../recipes/rect.toml")),
    ("seam", include_str!("../recipes/seam.toml")),
    ("remove", include_str!("../recipes/remove.toml")),
    (
        "remove-prompt",
        include_str!("../recipes/remove-prompt.toml"),
    ),
    ("repair", include_str!("../recipes/repair.toml")),
    ("repair-area", include_str!("../recipes/repair-area.toml")),
    ("protect", include_str!("../recipes/protect.toml")),
];

/// A shipped recipe by name, or a recipe file by path.
pub fn load_recipe(name_or_path: &str) -> Result<Recipe> {
    if let Some((_, text)) = SHIPPED.iter().find(|(name, _)| *name == name_or_path) {
        return Recipe::parse(text, &format!("shipped recipe {name_or_path}"));
    }
    let path = Path::new(name_or_path);
    if path.is_file() {
        return Recipe::read(path);
    }
    let names: Vec<&str> = SHIPPED.iter().map(|(n, _)| *n).collect();
    Err(Error::usage(format!(
        "{name_or_path}: not a shipped recipe ({}) and not a file",
        names.join(", ")
    )))
}
