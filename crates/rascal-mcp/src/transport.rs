//! The transport: `rmcp` over stdio (the server) and over a child process (the packs), at one
//! protocol revision.

use std::borrow::Cow;

pub use rmcp;
use rmcp::model::ProtocolVersion;

/// The protocol revision rascal speaks, as the spec fixes it.
pub const PROTOCOL: &str = "2025-11-25";

/// The revisions a rascal server advertises and a rascal client offers: exactly one.
pub fn supported_versions() -> Cow<'static, [ProtocolVersion]> {
    Cow::Borrowed(&[ProtocolVersion::V_2025_11_25])
}

/// True when a peer's negotiated revision is the one rascal speaks.
pub fn accepts(version: &ProtocolVersion) -> bool {
    *version == ProtocolVersion::V_2025_11_25
}

/// rascal's implementation identity on the wire.
pub fn implementation() -> rmcp::model::Implementation {
    rmcp::model::Implementation::new("rascal", env!("CARGO_PKG_VERSION"))
}
