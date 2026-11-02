//! Binding (CONTRACT 4.7; PLAN.md 10.3): an artifact reference (3.4) given through a port is
//! checked and resolved in the session's store before any run is allocated.
//!
//! [`bind`] checks, in order, the type (another tool's type, an unknown one, or one of rascal's
//! the port does not take is `wrong_type`), the schema version (`wrong_version`; rascal reads
//! version 1 of every type) and resolution by identity in the store (`unresolved`, with a remedy
//! by type). Frames across ports (`wrong_frame`) are the caller's to check, since only it knows
//! which inputs go together. [`bind_all`] binds a call's references and names every problem, in
//! the order given, so one refusal says everything that is wrong.

use table_manners::operations::{ArtifactRef, Binding, BindingProblem};

use crate::artifact::{ArtifactType, Portable, SCHEMA_VERSION, TOOL};
use crate::error::Result;
use crate::workspace::{Resolved, Workspace};

/// What a port of `artifact_type` takes, for people: `rascal:Mask v1`.
pub fn expected(artifact_type: ArtifactType) -> String {
    format!("{} v{SCHEMA_VERSION}", artifact_type.contract_type())
}

/// What a person does when the store does not hold an artifact of `artifact_type`.
pub fn remedy(artifact_type: ArtifactType) -> &'static str {
    match artifact_type.portable() {
        Portable::Bundle => {
            "Import its bundle with the import tool (rascal import), then call again."
        }
        Portable::Json => "Pass the location itself, or import it with the import tool.",
        Portable::File => "Pass the image by path or inline, with this reference as expect.",
        Portable::None => {
            "This artifact cannot move between workspaces; start rascal serve on the workspace that made it."
        }
    }
}

/// An argument's value as an artifact reference (CONTRACT 3.4): the shape the reference schema
/// gives (every required field, nothing else, an identity `sha256:<64 lowercase hex>`, a
/// `<tool>://artifact/` resource). Anything else, a path among them, is a usage error naming the
/// slot: the input schema rejects it, so it is plain invalid params, never a `binding` refusal.
pub fn reference_in(slot: &str, value: &serde_json::Value) -> Result<ArtifactRef> {
    let not = |why: String| {
        crate::Error::usage(format!(
            "{slot}: not an artifact reference (CONTRACT 3.4; type, schema_version, tool, identity, frame, resource): {why}"
        ))
    };
    let reference: ArtifactRef =
        serde_json::from_value(value.clone()).map_err(|e| not(e.to_string()))?;
    if crate::identity::Identity::parse(&reference.identity).is_none() {
        return Err(not(format!(
            "identity {} is not sha256:<64 lowercase hex>",
            reference.identity
        )));
    }
    let scheme_ok = reference
        .resource
        .split_once("://artifact/")
        .is_some_and(|(scheme, _)| {
            scheme.starts_with(|c: char| c.is_ascii_lowercase())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+.-".contains(c))
        });
    if !scheme_ok {
        return Err(not(format!(
            "resource {} is not <tool>://artifact/<identity>",
            reference.resource
        )));
    }
    Ok(reference)
}

/// One reference to bind: the port it came through, its position in a `many` port, the type
/// the port takes, and the reference as given.
#[derive(Debug, Clone, Copy)]
pub struct Given<'a> {
    pub port: &'a str,
    pub index: Option<u32>,
    pub expected: ArtifactType,
    pub reference: &'a ArtifactRef,
}

/// Binds one reference in `store`: the artifact it names, or the problem. The outer error is
/// the store's own (it could not be read), never the caller's.
pub fn bind(store: &Workspace, given: Given<'_>) -> Result<std::result::Result<Resolved, Binding>> {
    let reference = given.reference;
    let problem = |problem: BindingProblem, got: String, remedy: Option<&str>| Binding {
        port: Some(given.port.to_string()),
        index: given.index,
        problem,
        expected: expected(given.expected),
        got,
        remedy: remedy.map(str::to_string),
    };
    let named = format!("{} v{}", reference.artifact_type, reference.schema_version);
    let own_type = reference.artifact_type.contains(':');
    let artifact_type = ArtifactType::from_contract(&reference.artifact_type);
    if artifact_type != Some(given.expected) {
        return Ok(Err(problem(BindingProblem::WrongType, named, None)));
    }
    if own_type && reference.tool != TOOL {
        return Ok(Err(problem(
            BindingProblem::WrongType,
            format!("{named} made by {}", reference.tool),
            None,
        )));
    }
    if reference.schema_version != SCHEMA_VERSION {
        return Ok(Err(problem(BindingProblem::WrongVersion, named, None)));
    }
    Ok(match store.resolve(given.expected, &reference.identity)? {
        Some(resolved) => Ok(resolved),
        None => Err(problem(
            BindingProblem::Unresolved,
            reference.identity.clone(),
            Some(remedy(given.expected)),
        )),
    })
}

/// Binds every reference of a call, in the order given: all of them resolved, or every problem.
pub fn bind_all(
    store: &Workspace,
    given: &[Given<'_>],
) -> Result<std::result::Result<Vec<Resolved>, Vec<Binding>>> {
    let mut resolved = Vec::new();
    let mut problems = Vec::new();
    for item in given {
        match bind(store, *item)? {
            Ok(found) => resolved.push(found),
            Err(problem) => problems.push(problem),
        }
    }
    Ok(if problems.is_empty() {
        Ok(resolved)
    } else {
        Err(problems)
    })
}
