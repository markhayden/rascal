//! The one error type of the core crate. Every variant names the path or thing it is about.

use std::path::PathBuf;

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
    /// A schema file that cannot be compiled into a validator.
    #[error("{path}: not a usable JSON Schema: {message}")]
    Schema { path: PathBuf, message: String },
    /// A `$ref` to a schema URI outside the `urn:rascal:schemas:<dir>:<name>` form.
    #[error("unknown schema reference {uri}")]
    UnknownSchemaUri { uri: String },
    /// A copied schema whose bytes no longer match `schemas/SOURCES.toml`.
    #[error("{path}: sha256 is {actual}, SOURCES.toml records {expected}")]
    CopiedSchemaChanged {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

/// The core's result type.
pub type Result<T> = std::result::Result<T, Error>;
