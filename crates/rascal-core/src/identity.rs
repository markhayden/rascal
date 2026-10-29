//! Identity: the SHA-256 of an artifact's canonical payload, written `sha256:<lower hex>`.
//! How each payload is built is in `canonical` (ADR 0016).

/// A `sha256:<hex>` identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identity(String);

impl Identity {
    /// From a finished digest.
    pub fn from_digest(digest: &[u8]) -> Self {
        Identity(format!("sha256:{}", hex(digest)))
    }

    /// The identity as `sha256:<hex>`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The hex digits alone, for a file or directory name.
    pub fn hex(&self) -> &str {
        self.0.strip_prefix("sha256:").unwrap_or(&self.0)
    }

    /// Parses `sha256:<64 lower-case hex>`; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix("sha256:")?;
        (hex.len() == 64
            && hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
        .then(|| Identity(text.to_string()))
    }

    /// Parses, or hashes the text itself when it is not an identity, so a caller holding an
    /// identity string from a record always gets a value.
    pub fn parse_or_hash(text: &str) -> Self {
        Identity::parse(text).unwrap_or_else(|| {
            crate::canonical::identity("text", &serde_json::Value::String(text.to_string()))
        })
    }
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for Identity {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Lower-case hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
