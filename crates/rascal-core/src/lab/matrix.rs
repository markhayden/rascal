//! The matrix: the corpus and protocol it runs under, the seed, the images with their
//! per-image bindings and overrides, and the arms (a recipe, its overrides, a filler, a spend
//! ceiling). `validate` checks it against the protocol, the corpus and the recipes, and
//! rejects a recipe in which a finder's input derives, through any chain of bindings, from
//! another finder's output, naming the binding: every finder's invocation descriptor must be
//! computable before any finder runs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::error::Result;
use crate::lab::corpus::Corpus;
use crate::lab::protocol::Protocol;
use crate::recipe::Recipe;
pub use crate::recipe::{Override, apply_override};
use crate::registry::OpRegistry;

/// `matrix.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    pub version: u32,
    /// The lab's name; the protocol's must match.
    pub name: String,
    /// The corpus manifest, relative to this file.
    pub corpus: String,
    /// The protocol, relative to this file; required, and its digest is recorded.
    #[serde(default)]
    pub protocol: String,
    /// The draw seed; the protocol's must match.
    pub seed: u64,
    #[serde(default)]
    pub image: Vec<MatrixImage>,
    #[serde(default)]
    pub arm: Vec<Arm>,
}

/// One image of the matrix: a corpus id with what the recipes need for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixImage {
    /// A corpus image id.
    pub id: String,
    /// Recipe input bindings by input name: files relative to this file (a location, a mask).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    /// Per-image parameter overrides, `"<step>.<param>" = value` (a prompt for this image).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Override>,
}

/// One arm: a recipe with overrides, healed through a filler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Arm {
    pub name: String,
    /// A shipped recipe by name or a recipe file relative to this file.
    pub recipe: String,
    /// Parameter overrides, `"<step>.<param>" = value`; `"<step>.constants"` selects a set.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Override>,
    /// The filler entry in the configuration the run is given.
    pub filler: String,
    /// What one filler call may cost; the ledger sums it.
    #[serde(default)]
    pub spend_usd_per_call: f64,
}

/// A validated matrix: the recipes its arms resolved to, with overrides applied.
#[derive(Debug, Clone)]
pub struct Validated {
    pub matrix: Matrix,
    pub protocol: Protocol,
    pub protocol_digest: String,
    pub corpus: Corpus,
    /// Each arm's recipe, in arm order, with the arm's overrides applied.
    pub recipes: Vec<Recipe>,
}

impl Matrix {
    /// Parses matrix text: version 1, a name, non-empty images and arms with unique ids and
    /// names, a spend ceiling that is not negative, override keys of the `step.param` form.
    pub fn parse(text: &str, origin: &str) -> Result<Matrix> {
        let matrix: Matrix = toml::from_str(text).map_err(|source| Error::Toml {
            path: PathBuf::from(origin),
            source: Box::new(source),
        })?;
        let usage = |message: String| Error::usage(format!("{origin}: {message}"));
        if matrix.version != 1 {
            return Err(usage(format!("version {} is not 1", matrix.version)));
        }
        if matrix.name.is_empty() {
            return Err(usage("name is empty".into()));
        }
        if matrix.protocol.is_empty() {
            return Err(usage(
                "protocol is required; the run records its digest".into(),
            ));
        }
        if matrix.image.is_empty() {
            return Err(usage("at least one image is required".into()));
        }
        if matrix.arm.is_empty() {
            return Err(usage("at least one arm is required".into()));
        }
        let mut ids = BTreeSet::new();
        for image in &matrix.image {
            if image.id.is_empty() || !ids.insert(image.id.as_str()) {
                return Err(usage(format!(
                    "image id {:?} is empty or repeated",
                    image.id
                )));
            }
            for key in image.overrides.keys() {
                check_override_key(key).map_err(|m| usage(format!("image {}: {m}", image.id)))?;
            }
        }
        let mut names = BTreeSet::new();
        for arm in &matrix.arm {
            if arm.name.is_empty() || !names.insert(arm.name.as_str()) {
                return Err(usage(format!(
                    "arm name {:?} is empty or repeated",
                    arm.name
                )));
            }
            if arm.recipe.is_empty() || arm.filler.is_empty() {
                return Err(usage(format!(
                    "arm {}: recipe and filler are required",
                    arm.name
                )));
            }
            if arm.spend_usd_per_call.is_nan() || arm.spend_usd_per_call < 0.0 {
                return Err(usage(format!(
                    "arm {}: spend_usd_per_call {} is negative or not a number",
                    arm.name, arm.spend_usd_per_call
                )));
            }
            for key in arm.overrides.keys() {
                check_override_key(key).map_err(|m| usage(format!("arm {}: {m}", arm.name)))?;
            }
        }
        Ok(matrix)
    }

    /// Reads a matrix file.
    pub fn read(path: &Path) -> Result<Matrix> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Matrix::parse(&text, &path.display().to_string())
    }

    /// The arm with this name.
    pub fn arm(&self, name: &str) -> Option<&Arm> {
        self.arm.iter().find(|a| a.name == name)
    }

    /// The matrix against its protocol and corpus, and every arm's recipe against the
    /// registry: names and seeds agree; every protocol arm is a matrix arm; every image is in
    /// the corpus and every protocol image is in the matrix; every override names a step and
    /// a parameter (or `constants`) of its recipe; every per-image input is one `heal` binds
    /// from a file and every required input of every arm's recipe is bound; no finder's input
    /// derives from another finder's output. `finders` names the ops that are pack tools
    /// (everything else in the registry is deterministic); `load` resolves an arm's `recipe`.
    pub fn validate(
        &self,
        protocol: &Protocol,
        protocol_digest: &str,
        corpus: &Corpus,
        registry: &OpRegistry,
        finders: &[&str],
        load: &dyn Fn(&str) -> Result<Recipe>,
    ) -> Result<Validated> {
        let usage = |message: String| Error::usage(format!("matrix {}: {message}", self.name));
        if protocol.name != self.name {
            return Err(usage(format!(
                "the protocol is {} and the matrix is {}; one lab, one name",
                protocol.name, self.name
            )));
        }
        if protocol.seed != self.seed {
            return Err(usage(format!(
                "seed {} differs from the protocol's {}",
                self.seed, protocol.seed
            )));
        }
        for arm in &protocol.arms {
            if self.arm(arm).is_none() {
                return Err(usage(format!(
                    "the protocol compares arm {arm}, which the matrix lacks"
                )));
            }
        }
        for image in &self.image {
            if corpus.image(&image.id).is_none() {
                return Err(usage(format!("image {} is not in the corpus", image.id)));
            }
        }
        for id in protocol.image_ids() {
            if !self.image.iter().any(|i| i.id == id) {
                return Err(usage(format!(
                    "the protocol names image {id}, which the matrix lacks"
                )));
            }
        }
        let mut recipes = Vec::with_capacity(self.arm.len());
        for arm in &self.arm {
            let mut recipe = load(&arm.recipe)
                .map_err(|e| usage(format!("arm {}: recipe {}: {e}", arm.name, arm.recipe)))?;
            for (key, value) in &arm.overrides {
                apply_override(&mut recipe, registry, key, value)
                    .map_err(|m| usage(format!("arm {}: override {key}: {m}", arm.name)))?;
            }
            for image in &self.image {
                for (key, value) in &image.overrides {
                    // Checked against every arm's recipe; applied per cell at run time.
                    apply_override(&mut recipe.clone(), registry, key, value)
                        .map_err(|m| usage(format!("image {}: override {key}: {m}", image.id)))?;
                }
            }
            recipe
                .validate(registry)
                .map_err(|e| usage(format!("arm {}: recipe {}: {e}", arm.name, arm.recipe)))?;
            for image in &self.image {
                for name in image.inputs.keys() {
                    if !FILE_INPUTS.contains(&name.as_str()) {
                        return Err(usage(format!(
                            "image {}: inputs.{name} is not one heal binds from a file ({})",
                            image.id,
                            FILE_INPUTS.join(", ")
                        )));
                    }
                }
                for (name, decl) in &recipe.inputs {
                    if name != "image" && !decl.optional && !image.inputs.contains_key(name) {
                        return Err(usage(format!(
                            "image {}: arm {} needs inputs.{name}, which the image does not bind",
                            image.id, arm.name
                        )));
                    }
                }
            }
            reject_dependent_finders(&recipe, finders)
                .map_err(|m| usage(format!("arm {}: recipe {}: {m}", arm.name, arm.recipe)))?;
            recipes.push(recipe);
        }
        Ok(Validated {
            matrix: self.clone(),
            protocol: protocol.clone(),
            protocol_digest: protocol_digest.to_string(),
            corpus: corpus.clone(),
            recipes,
        })
    }
}

/// The recipe inputs `heal` binds from a file beside the matrix.
pub const FILE_INPUTS: [&str; 4] = ["location", "stroke", "protect", "mask"];

fn check_override_key(key: &str) -> std::result::Result<(), String> {
    match key.split_once('.') {
        Some((step, param)) if !step.is_empty() && !param.is_empty() && !param.contains('.') => {
            Ok(())
        }
        _ => Err(format!("override key {key:?} is not <step>.<param>")),
    }
}

/// The dependent-finder rule: a finder step whose input binds, through any chain of steps, to
/// a finder's output is refused, naming the binding and the finder it derives from.
pub fn reject_dependent_finders(
    recipe: &Recipe,
    finders: &[&str],
) -> std::result::Result<(), String> {
    // For each step id, the finder its outputs derive from, if any.
    let mut derives: BTreeMap<&str, &str> = BTreeMap::new();
    for step in &recipe.step {
        let is_finder = finders.contains(&step.op.as_str());
        let mut from: Option<&str> = None;
        for (port, binding) in &step.inputs {
            let Some((source, _)) = binding.split_once('.') else {
                continue;
            };
            if let Some(finder) = derives.get(source) {
                if is_finder {
                    return Err(format!(
                        "finder {} ({}) takes {port} = {binding}, which derives from finder {finder}; a finder's inputs must be computable before any finder runs",
                        step.id, step.op
                    ));
                }
                from = from.or(Some(finder));
            }
        }
        if is_finder {
            derives.insert(&step.id, &step.id);
        } else if let Some(finder) = from {
            derives.insert(&step.id, finder);
        }
    }
    Ok(())
}
