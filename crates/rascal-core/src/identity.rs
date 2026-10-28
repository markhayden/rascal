//! Identity: the SHA-256 of an artifact's canonical payload, written `sha256:<lower hex>`.
//!
//! Task 1.1 hashes length-prefixed byte parts in a fixed order per type; task 1.2 replaces
//! this with the spec's canonical serialization and committed test vectors.
//! REPLACED BY 1.2 (canonical serialization, reference substitution, test vectors).

use sha2::{Digest, Sha256};

/// A `sha256:<hex>` identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identity(String);

impl Identity {
    /// The identity of a sequence of byte parts, each length-prefixed so the boundaries are
    /// part of what is hashed.
    pub fn of_parts(parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        Identity(format!("sha256:{}", hex(&digest)))
    }

    /// The identity as `sha256:<hex>`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The hex digits alone, for a file or directory name.
    pub fn hex(&self) -> &str {
        self.0.strip_prefix("sha256:").unwrap_or(&self.0)
    }

    /// Parses `sha256:<64 hex>`; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix("sha256:")?;
        (hex.len() == 64
            && hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
        .then(|| Identity(text.to_string()))
    }
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lower-case hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
