//! The one error type of the core crate. Every variant names the path or thing it is about.
//! The binary maps `Usage`, `InvalidArgs` and `Recipe` to exit 64 and everything else to exit 3.

use std::path::PathBuf;

use crate::op::Envelope;

/// Everything the core can fail with.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a file.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A file that should hold JSON does not.
    #[error("{path}: not valid JSON: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// A file that should hold TOML does not.
    #[error("{path}: not valid TOML: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    /// An image that cannot be decoded or encoded.
    #[error("{path}: {source}")]
    Image {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
    /// A schema file that cannot be compiled into a validator.
    #[error("{path}: not a usable JSON Schema: {message}")]
    Schema { path: PathBuf, message: String },
    /// A `$ref` to a schema URI outside the `urn:rascal:schemas:<dir>:<name>` form.
    #[error("unknown schema reference {uri}")]
    UnknownSchemaUri { uri: String },
    /// A file under `schemas/copied/` that `schemas/SOURCES.toml` does not list.
    #[error("{path}: not listed in SOURCES.toml")]
    UnlistedCopiedSchema { path: PathBuf },
    /// A copied schema whose bytes no longer match `schemas/SOURCES.toml`.
    #[error("{path}: sha256 is {actual}, SOURCES.toml records {expected}")]
    CopiedSchemaChanged {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// The caller asked for something the command line cannot grant: a bad argument, a run
    /// directory that is not empty, an unknown recipe. Exit 64.
    #[error("{message}")]
    Usage { message: String },
    /// An op's parameters failed validation: the `invalid_args` envelope that was journaled,
    /// kept so a server can return it as the result. Exit 64.
    #[error("step {step}: {op}: {}", envelope.reason.clone().unwrap_or_default())]
    InvalidArgs {
        step: String,
        op: String,
        envelope: Box<Envelope>,
    },
    /// A recipe that does not validate, named by step. Exit 64.
    #[error("recipe {recipe}, step {step}: {message}")]
    Recipe {
        recipe: String,
        step: String,
        message: String,
    },
    /// A mask that breaks one of its invariants, named. Exit 3.
    #[error("mask {id}: invariant broken: {invariant}")]
    MaskInvariant { id: String, invariant: String },
    /// An op was given an input it cannot take: a wrong artifact type, a frame mismatch, a
    /// location kind a later task implements. Exit 3.
    #[error("{op}: {message}")]
    Op { op: String, message: String },
}

/// The core's result type.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// A usage error from a message.
    pub fn usage(message: impl Into<String>) -> Self {
        Error::Usage {
            message: message.into(),
        }
    }

    /// An op error from a message.
    /// A usage error about one op: the caller's arguments, not the op, are wrong.
    pub fn usage_for(op: &str, message: impl Into<String>) -> Self {
        Error::Usage {
            message: format!("{op}: {}", message.into()),
        }
    }

    pub fn op(op: &str, message: impl Into<String>) -> Self {
        Error::Op {
            op: op.to_string(),
            message: message.into(),
        }
    }

    /// An io error naming its path.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
