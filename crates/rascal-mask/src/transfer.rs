//! Export and import (CONTRACT 5.1; PLAN.md 10.10): the reserved MCP tools `export` and
//! `import`, and `rascal export --json <file|->` and `rascal import --json <file|->`, which read
//! the same request and print the same document. They are controls, not runs: they write no run,
//! are never in the description's `actions` and never wait for the call permit.
//!
//! - **Export** takes an artifact reference (3.4), binds it in the store by identity (port
//!   `artifact`; a reference the store does not hold is the `binding` refusal) and writes its
//!   portable form into rascal's export area, `$XDG_DATA_HOME/rascal/exports/`, never at a path a
//!   caller names: a `rascal:Mask` as its bundle (ADR 0022, `application/x-tar`), a `location`
//!   as the contract's location JSON (9, `application/json`). The file is named by the
//!   artifact's identity, so the same artifact always lands in the same file with the same
//!   bytes, and a file already there with those bytes is left as it is.
//! - **Import** takes a file by path (under the path rule, 5.2) or its bytes inline (up to the
//!   description's `limits.inline_bytes`), checks an optional SHA-256 against the bytes, tells a
//!   mask bundle from a location by its content, checks it whole before recording anything, and
//!   answers with the stored artifact's reference and `already_present`. A location is checked
//!   against an image the store holds by its file digest. Anything else (a PNG, a JPEG, a
//!   rendering of a Mask) is refused naming what it is.
//!
//! Every wrong request is a `-32602` refusal (`invalid_arguments` with code `usage` or `input`,
//! never retryable, or `binding`); on the command line it is the refusal JSON on stdout and exit
//! 64. A failure of rascal's own (the store could not be read or written) is neither.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::{ArtifactType, Portable};
use rascal_core::binding::{self, Given};
use rascal_core::config::ConfigState;
use rascal_core::contract::{self, table_manners};
use rascal_core::paths::{self, PathRule};
use rascal_core::workspace::Workspace;
use rascal_mask::describe::INLINE_BYTES;
use rascal_mcp::server::{ActionResult, CallError, McpTool as Tool};
use serde_json::{Map, Value, json};
use table_manners::operations::{
    ArtifactRef, Binding, BindingProblem, ExportResponse, ExportedFile, ImportResponse,
};
use table_manners::{ErrorCode, ErrorInfo, Refusal, RefusalReason, Tool as TmTool};

/// A mask bundle's media type (ADR 0022).
pub const BUNDLE_MEDIA_TYPE: &str = "application/x-tar";
/// A location's media type.
pub const JSON_MEDIA_TYPE: &str = "application/json";

/// The tools' names.
pub const NAMES: [&str; 2] = ["export", "import"];

/// Why a request was not answered.
#[derive(Debug)]
pub enum Failure {
    /// The reference does not bind (CONTRACT 4.7): the `binding` refusal.
    Binding(Vec<Binding>),
    /// A wrong request: the `invalid_arguments` refusal, its code `usage` for an
    /// [`Error::Usage`] and `input` for anything else.
    Invalid(Error),
    /// rascal's own failure (its store or its export area): not the caller's.
    Internal(Error),
}

impl Failure {
    /// What went wrong, in a sentence.
    pub fn message(&self) -> String {
        match self {
            Failure::Binding(bindings) => {
                let text: Vec<String> = bindings
                    .iter()
                    .map(|b| {
                        format!(
                            "{}: {} ({})",
                            b.port.as_deref().unwrap_or("artifact"),
                            serde_json::to_value(b.problem)
                                .ok()
                                .and_then(|p| p.as_str().map(str::to_string))
                                .unwrap_or_default(),
                            b.got
                        )
                    })
                    .collect();
                format!("the reference does not bind: {}", text.join("; "))
            }
            Failure::Invalid(error) | Failure::Internal(error) => error.to_string(),
        }
    }

    /// The refusal the contract names, for a failure that is the caller's.
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            Failure::Binding(bindings) => Some(Refusal {
                reason: RefusalReason::Binding,
                run: None,
                requirements: None,
                error: None,
                bindings: Some(bindings.clone()),
            }),
            Failure::Invalid(error) => Some(Refusal {
                reason: RefusalReason::InvalidArguments,
                run: None,
                requirements: None,
                error: Some(ErrorInfo {
                    code: match error {
                        Error::Usage { .. } => ErrorCode::Usage,
                        _ => ErrorCode::Input,
                    },
                    retryable: false,
                    reason: error.to_string(),
                }),
                bindings: None,
            }),
            Failure::Internal(_) => None,
        }
    }
}

fn usage(message: impl Into<String>) -> Failure {
    Failure::Invalid(Error::usage(message))
}

fn input(message: impl Into<String>) -> Failure {
    Failure::Invalid(Error::Input {
        message: message.into(),
    })
}

/// A store or export area error: rascal's own.
fn internal(error: Error) -> Failure {
    Failure::Internal(error)
}

/// A check of the caller's bytes: its usage errors are what the bytes are (`input`); anything
/// else is rascal's own.
fn checked(error: Error) -> Failure {
    match error {
        Error::Usage { message } | Error::Input { message } => input(message),
        other => internal(other),
    }
}

/// rascal's export area: `$XDG_DATA_HOME/rascal/exports` (else `~/.local/share/...`), outside
/// every run directory and every workspace.
pub fn exports_dir() -> PathBuf {
    crate::actions::data_home().join("rascal/exports")
}

fn tool() -> TmTool {
    TmTool {
        name: rascal_core::artifact::TOOL.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// The request as an object with only `allowed` keys.
fn object<'a>(
    request: &'a Value,
    allowed: &[&str],
    shape: &str,
) -> Result<&'a Map<String, Value>, Failure> {
    let object = request
        .as_object()
        .ok_or_else(|| usage(format!("the request is {shape}")))?;
    if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(usage(format!(
            "the request has an unknown field {key}; it is {shape}"
        )));
    }
    Ok(object)
}

// --- export -----------------------------------------------------------------------------------

/// The types rascal exports, for people.
fn portable_types() -> String {
    ArtifactType::ALL
        .into_iter()
        .filter(|t| matches!(t.portable(), Portable::Json | Portable::Bundle))
        .map(binding::expected)
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Exports the artifact a request names (5.1) into `exports`.
pub fn export(
    store: &Workspace,
    exports: &Path,
    request: &Value,
) -> Result<ExportResponse, Failure> {
    const SHAPE: &str = "{\"artifact\": <artifact reference>}";
    let fields = object(request, &["artifact"], SHAPE)?;
    let value = fields
        .get("artifact")
        .ok_or_else(|| usage(format!("the request is {SHAPE}")))?;
    let reference = binding::reference_in("artifact", value).map_err(Failure::Invalid)?;
    let Some(artifact_type) = ArtifactType::from_contract(&reference.artifact_type) else {
        return Err(Failure::Binding(vec![Binding {
            port: Some("artifact".to_string()),
            index: None,
            problem: BindingProblem::WrongType,
            expected: portable_types(),
            got: format!(
                "{} v{} made by {}",
                reference.artifact_type, reference.schema_version, reference.tool
            ),
            remedy: None,
        }]));
    };
    match artifact_type.portable() {
        Portable::None => {
            return Err(usage(format!(
                "artifact: a {} is portable: none (CONTRACT 3.3): it cannot leave its workspace yet, so it has no export",
                artifact_type.contract_type()
            )));
        }
        Portable::File => {
            return Err(usage(
                "artifact: an image travels as a file, by path or inline (CONTRACT 3.3, 10.1), and has no export",
            ));
        }
        Portable::Json | Portable::Bundle => {}
    }
    let given = Given {
        port: "artifact",
        index: None,
        expected: artifact_type,
        reference: &reference,
    };
    let resolved = match binding::bind(store, given).map_err(internal)? {
        Ok(resolved) => resolved,
        Err(problem) => return Err(Failure::Binding(vec![problem])),
    };
    let reference = contract::artifact_ref(store, &resolved.id).map_err(internal)?;
    let (bytes, ext, media_type) = match artifact_type {
        ArtifactType::Mask => {
            let exported = store.export_mask(&resolved.id).map_err(checked)?;
            (exported.bytes, "mask.tar", BUNDLE_MEDIA_TYPE)
        }
        _ => (
            location_json(store, &resolved.id, &reference)?,
            "location.json",
            JSON_MEDIA_TYPE,
        ),
    };
    let path = write_export(exports, &reference.identity, ext, &bytes).map_err(internal)?;
    Ok(ExportResponse {
        contract: contract::CONTRACT_VERSION.to_string(),
        tool: tool(),
        file: ExportedFile {
            path: path.display().to_string(),
            sha256: contract::file_sha256(&bytes),
            bytes: bytes.len() as u64,
            media_type: media_type.to_string(),
        },
        artifact: reference,
    })
}

/// A stored location as the contract's location JSON (9), on the image it was made on. Only a
/// location in an image's own frame has that form: one in a derived frame (a crop, a cut canvas)
/// is in coordinates no image file has, and one whose image file the store cannot name has no
/// `image` to carry. rascal's `group` (the parts of one shape split at the wrap edge) does not
/// travel: each location leaves as one shape.
fn location_json(store: &Workspace, id: &str, reference: &ArtifactRef) -> Result<Vec<u8>, Failure> {
    let generation = store.snapshot().map_err(internal)?;
    let location = generation
        .location(id)
        .map(|l| l.location.clone())
        .ok_or_else(|| internal(Error::usage(format!("{id}: not a location of the store"))))?;
    let frames = store.frames().map_err(internal)?;
    let frame = frames.get(location.frame()).ok_or_else(|| {
        internal(Error::usage(format!(
            "frame {} is unknown",
            location.frame()
        )))
    })?;
    if frame.parent.is_some() {
        return Err(input(format!(
            "artifact: the location is in a derived frame ({}), whose coordinates no image file has; only a location in an image's own frame leaves as JSON",
            frame.id
        )));
    }
    let on = reference.on.clone().ok_or_else(|| {
        input(
            "artifact: the store cannot tell which image file the location is on, so it has no portable form",
        )
    })?;
    let out = contract::location_out(&[location], on, contract::LocationMeta::default())
        .map_err(checked)?;
    let mut bytes = serde_json::to_vec_pretty(&out)
        .map_err(|e| internal(Error::usage(format!("location: {e}"))))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes `bytes` as `<identity with : as ->.<ext>` in `dir` and returns its absolute path. A
/// file already holding exactly these bytes is left as it is; any other is replaced whole
/// (written beside it, then renamed over it).
fn write_export(
    dir: &Path,
    identity: &str,
    ext: &str,
    bytes: &[u8],
) -> rascal_core::error::Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let dir = std::path::absolute(dir).map_err(|e| Error::io(dir, e))?;
    let name = format!("{}.{ext}", identity.replace(':', "-"));
    let path = dir.join(&name);
    if std::fs::read(&path).is_ok_and(|held| held == bytes) {
        return Ok(path);
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let partial = dir.join(format!(".{name}.{}-{nanos}.partial", std::process::id()));
    std::fs::write(&partial, bytes).map_err(|e| Error::io(&partial, e))?;
    std::fs::rename(&partial, &path).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        Error::io(&path, e)
    })?;
    Ok(path)
}

// --- import -----------------------------------------------------------------------------------

/// What a file's bytes are.
enum Kind {
    Bundle,
    Location,
    /// Neither, with what it is for people.
    Other(&'static str),
}

fn sniff(bytes: &[u8]) -> Kind {
    if bytes.is_empty() {
        return Kind::Other("the file is empty");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Kind::Other("it is a PNG image");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Kind::Other("it is a JPEG image");
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Kind::Other("it is gzip-compressed (a mask bundle is an uncompressed tar)");
    }
    if bytes.get(257..262) == Some(b"ustar".as_slice()) {
        return Kind::Bundle;
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        return Kind::Location;
    }
    Kind::Other("it is neither a tar archive nor a JSON object")
}

/// `sha256` as the request gives it: 64 lowercase hex digits.
fn digest_in(value: Option<&Value>) -> Result<Option<String>, Failure> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value.as_str() {
        Some(hex)
            if hex.len() == 64
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(Some(hex.to_string()))
        }
        _ => Err(usage(format!(
            "sha256: {value} is not 64 lowercase hex digits (the file's SHA-256)"
        ))),
    }
}

/// Reads an admitted file within the bundle limit (the larger of the two portable forms). A read
/// that fails is the path rule's one refusal, which says nothing about the file.
fn read_admitted(slot: &str, path: &Path) -> Result<Vec<u8>, Failure> {
    let limit = rascal_core::bundle::MAX_BUNDLE_BYTES;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|_| Failure::Invalid(paths::refusal(slot)))?;
    if bytes.len() as u64 > limit {
        return Err(input(format!(
            "{slot}: the file is over the limit of {limit} bytes; nothing was imported"
        )));
    }
    Ok(bytes)
}

/// Imports the file a request names (5.1) into the store; paths are read under `rule`.
pub fn import(
    store: &Workspace,
    rule: &PathRule,
    request: &Value,
) -> Result<ImportResponse, Failure> {
    const SHAPE: &str = "{\"file\": {\"path\": <absolute path>}, \"sha256\"?: <hex>} or {\"inline\": {\"media_type\", \"data\"}, \"sha256\"?: <hex>}";
    let fields = object(request, &["file", "inline", "sha256"], SHAPE)?;
    let expect = digest_in(fields.get("sha256"))?;
    let (bytes, declared, named) = match (fields.get("file"), fields.get("inline")) {
        (Some(file), None) => {
            let file = object(file, &["path"], SHAPE)?;
            let path = file
                .get("path")
                .and_then(Value::as_str)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| usage(format!("file: give path; the request is {SHAPE}")))?;
            let admitted = rule
                .admit("file.path", Path::new(path))
                .map_err(Failure::Invalid)?;
            (read_admitted("file.path", &admitted)?, None, "file")
        }
        (None, Some(inline)) => {
            let inline = object(inline, &["media_type", "data"], SHAPE)?;
            let (Some(media_type), Some(data)) = (
                inline.get("media_type").and_then(Value::as_str),
                inline.get("data").and_then(Value::as_str),
            ) else {
                return Err(usage(format!(
                    "inline: give media_type and data; the request is {SHAPE}"
                )));
            };
            let bytes = rascal_mcp::jsonrpc::base64_decode(data)
                .ok_or_else(|| usage("inline: data is not standard base64"))?;
            if bytes.len() as u64 > INLINE_BYTES {
                return Err(usage(format!(
                    "inline: {} bytes, over the limit of {INLINE_BYTES} (limits.inline_bytes); pass a path",
                    bytes.len()
                )));
            }
            (bytes, Some(media_type.to_string()), "inline")
        }
        _ => {
            return Err(usage(format!(
                "give exactly one of file and inline; the request is {SHAPE}"
            )));
        }
    };
    let digest = contract::file_sha256(&bytes);
    if let Some(expect) = expect
        && expect != digest
    {
        return Err(input(format!(
            "{named}: the bytes' sha256 is {digest}, not {expect}; nothing was imported"
        )));
    }
    let kind = sniff(&bytes);
    let media_type = match kind {
        Kind::Bundle => BUNDLE_MEDIA_TYPE,
        Kind::Location => JSON_MEDIA_TYPE,
        Kind::Other(what) => {
            return Err(input(format!(
                "{named}: neither a mask bundle nor a location: {what}; a rendering of a Mask is an image, never the Mask (CONTRACT 5.1); nothing was imported"
            )));
        }
    };
    if let Some(declared) = declared
        && declared != media_type
    {
        return Err(usage(format!(
            "inline: media_type is {declared} but the data is {}; nothing was imported",
            match kind {
                Kind::Bundle => "a mask bundle (application/x-tar)",
                _ => "a location (application/json)",
            }
        )));
    }
    let (id, already_present) = match kind {
        Kind::Bundle => {
            let imported = store
                .import_mask_bytes(&bytes, named, named, None)
                .map_err(checked)?;
            (imported.id, imported.already_present)
        }
        _ => import_location(store, named, &bytes)?,
    };
    Ok(ImportResponse {
        contract: contract::CONTRACT_VERSION.to_string(),
        tool: tool(),
        artifact: contract::artifact_ref(store, &id).map_err(internal)?,
        already_present,
    })
}

/// A contract location (9), checked against the image the store holds under its file digest
/// and recorded in that image's frame. The id, and whether the store held it already.
fn import_location(
    store: &Workspace,
    named: &str,
    bytes: &[u8],
) -> Result<(String, bool), Failure> {
    let limit = contract::MAX_LOCATION_BYTES;
    if bytes.len() as u64 > limit {
        return Err(input(format!(
            "{named}: {} bytes, over the limit of {limit} for a location; nothing was imported",
            bytes.len()
        )));
    }
    let location: table_manners::images::Location = serde_json::from_slice(bytes).map_err(|e| {
        input(format!(
            "{named}: not a table-manners location (CONTRACT 9), as export writes one: {e}; nothing was imported"
        ))
    })?;
    let image = &location.image;
    let Some(held) = store
        .resolve(
            ArtifactType::Image,
            &format!("sha256:{}", image.file_sha256),
        )
        .map_err(internal)?
    else {
        return Err(input(format!(
            "{named}: the location is on the image file with sha256 {} ({}x{}), which this store does not hold: pass that image to any action first (by path or inline), then import the location again; nothing was imported",
            image.file_sha256, image.width, image.height
        )));
    };
    let frame = store
        .frames()
        .map_err(internal)?
        .get(&held.frame)
        .cloned()
        .ok_or_else(|| internal(Error::usage(format!("frame {} is unknown", held.frame))))?;
    let given = contract::image_reference(store, &held.id).map_err(internal)?;
    let ours =
        contract::location_in(&location, &given, &frame).map_err(|e| checked(label(named, e)))?;
    let identity = ours.identity(&frame.identity);
    if let Some(found) = store
        .resolve(ArtifactType::Location, identity.as_str())
        .map_err(internal)?
    {
        return Ok((found.id, true));
    }
    let (id, _) = store.add_location(ours).map_err(checked)?;
    Ok((id, false))
}

fn label(named: &str, error: Error) -> Error {
    match error {
        Error::Usage { message } => Error::usage(format!("{named}: {message}")),
        other => other,
    }
}

// --- serve ------------------------------------------------------------------------------------

/// Whether `name` is one of the two controls.
pub fn handles(name: &str) -> bool {
    NAMES.contains(&name)
}

/// The two tools, served whenever any type is portable (always, for rascal: a location and a
/// Mask are).
pub fn tools() -> Vec<Tool> {
    let reference = format!(
        "An artifact reference (CONTRACT 3.4) of {}, as a result gave it.",
        portable_types()
    );
    let export = json!({
        "type": "object",
        "properties": {
            "artifact": { "type": "object", "description": reference },
        },
        "required": ["artifact"],
        "additionalProperties": false,
    });
    let import = json!({
        "type": "object",
        "properties": {
            "file": {
                "type": "object",
                "properties": { "path": { "type": "string", "description": "An absolute path inside the directories rascal serve allows (CONTRACT 5.2)." } },
                "required": ["path"],
                "additionalProperties": false,
            },
            "inline": {
                "type": "object",
                "properties": {
                    "media_type": { "type": "string", "description": "application/x-tar for a mask bundle, application/json for a location." },
                    "data": { "type": "string", "description": "The file's bytes, standard base64." },
                },
                "required": ["media_type", "data"],
                "additionalProperties": false,
            },
            "sha256": { "type": "string", "description": "The file's SHA-256, lowercase hex, checked before anything is recorded." },
        },
        "additionalProperties": false,
    });
    let schema = |value: Value| Arc::new(value.as_object().cloned().unwrap_or_default());
    vec![
        Tool::new(
            "export",
            "Writes an artifact's portable form (a Mask's bundle, a location's JSON) into rascal's data directory and returns the file's path, SHA-256, size and media type (table-manners 5.1); runs nothing.",
            schema(export),
        ),
        Tool::new(
            "import",
            "Records an exported artifact (a mask bundle or a location JSON, by path or inline) in rascal's store and returns its reference and whether it was already there (table-manners 5.1); runs nothing.",
            schema(import),
        ),
    ]
}

/// Answers a call of one of the two controls.
pub fn call(
    name: &str,
    args: &Map<String, Value>,
    store: &Workspace,
    rule: &PathRule,
) -> Result<ActionResult, CallError> {
    let request = Value::Object(args.clone());
    let answered = match name {
        "export" => export(store, &exports_dir(), &request).and_then(|r| to_value(&r)),
        _ => import(store, rule, &request).and_then(|r| to_value(&r)),
    };
    match answered {
        Ok(structured) => Ok(ActionResult {
            structured,
            is_error: false,
            images: Vec::new(),
        }),
        Err(failure) => Err(match failure.refusal() {
            Some(refusal) => CallError::refused(
                format!("{name}: {}", failure.message()),
                serde_json::to_value(&refusal).unwrap_or(Value::Null),
            ),
            None => CallError::from(format!("{name}: {}", failure.message())),
        }),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|e| internal(Error::usage(e.to_string())))
}

// --- the command line -------------------------------------------------------------------------

/// Which command.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Export,
    Import,
}

/// The directories `rascal import` reads paths in (CONTRACT 5.2): its `--allow-dir` flags,
/// resolved, narrowed by the configuration's `[serve] allow_dirs` as `serve` narrows them; a
/// configuration that does not load allows nothing. With no flag, every path is refused.
pub fn allowed(flags: &[PathBuf], state: &ConfigState) -> rascal_core::error::Result<PathRule> {
    let flags = paths::resolve_flags(flags)?;
    let configured = match state {
        ConfigState::Invalid { .. } => Some(Vec::new()),
        _ => state
            .config()
            .and_then(|c| c.serve.as_ref())
            .and_then(|s| s.allow_dirs.as_ref())
            .map(|dirs| paths::resolve_configured(dirs).0),
    };
    Ok(PathRule::Allowed(paths::overlap(
        &flags,
        configured.as_deref(),
    )))
}

/// `rascal export --json <file|->` and `rascal import --json <file|->`: the request read from
/// the file (or stdin), answered against the workspace (`--workspace`, else `serve`'s default
/// for the configuration), the document printed. Exit 0; 64 with the refusal JSON on stdout for
/// a wrong request; 3 for a failure of rascal's own.
pub fn command(
    direction: Direction,
    request: &Path,
    workspace: Option<&Path>,
    config: Option<&Path>,
    allow_dirs: &[PathBuf],
) -> i32 {
    let verb = match direction {
        Direction::Export => "export",
        Direction::Import => "import",
    };
    let state = ConfigState::read(config);
    // The flags are the person's own argv, not a request: an error naming them, no refusal.
    let rule = match direction {
        Direction::Import => match allowed(allow_dirs, &state) {
            Ok(rule) => rule,
            Err(error) => {
                eprintln!("rascal {verb}: {error}");
                return crate::exit::code_for(&error);
            }
        },
        Direction::Export => PathRule::Allowed(Vec::new()),
    };
    let answered = read_request(request).and_then(|request| {
        let dir = workspace
            .map(Path::to_path_buf)
            .unwrap_or_else(|| crate::serve::default_workspace(&state));
        let store = Workspace::open(&dir).map_err(internal)?;
        match direction {
            Direction::Export => {
                export(&store, &exports_dir(), &request).and_then(|r| to_value(&r))
            }
            Direction::Import => import(&store, &rule, &request).and_then(|r| to_value(&r)),
        }
    });
    match answered {
        Ok(document) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&document).unwrap_or_default()
            );
            crate::exit::OK
        }
        Err(failure) => {
            eprintln!("rascal {verb}: {}", failure.message());
            match failure.refusal() {
                Some(refusal) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&refusal).unwrap_or_default()
                    );
                    crate::exit::USAGE
                }
                None => crate::exit::OPERATIONAL,
            }
        }
    }
}

/// The request file (or stdin for `-`) as JSON. It is the person's own file, so any failure is
/// the request's (`usage`).
fn read_request(path: &Path) -> Result<Value, Failure> {
    let text = if path == Path::new("-") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| usage(format!("the request on stdin: {e}")))?;
        text
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| usage(format!("the request {}: {e}", path.display())))?
    };
    serde_json::from_str(&text).map_err(|e| usage(format!("the request is not JSON: {e}")))
}
