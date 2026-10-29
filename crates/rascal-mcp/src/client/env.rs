//! The environment a pack runs in, and the redaction its captured stderr goes through. The
//! allowlist is the spec's: a short baseline plus the declared names, minus every filler's
//! `key_env` and every credential-shaped name; removal wins over declaration.

use std::collections::BTreeMap;

/// The variables every pack sees when the parent has them.
pub const BASELINE: [&str; 5] = ["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR"];

/// The suffixes that mark a name as a credential, whatever the config declares.
pub const CREDENTIAL_SUFFIXES: [&str; 3] = ["_KEY", "_TOKEN", "_SECRET"];

/// True for a name no pack may see: one a filler uses as `key_env`, or one ending in `_KEY`,
/// `_TOKEN` or `_SECRET` (case-insensitively).
pub fn is_credential(name: &str, key_envs: &[String]) -> bool {
    let upper = name.to_ascii_uppercase();
    key_envs.iter().any(|k| k == name)
        || CREDENTIAL_SUFFIXES
            .iter()
            .any(|suffix| upper.ends_with(suffix))
}

/// The pack's environment from the parent's: the baseline plus `declared`, minus every
/// credential name.
pub fn allowlist(
    declared: &[String],
    key_envs: &[String],
    parent: impl IntoIterator<Item = (String, String)>,
) -> BTreeMap<String, String> {
    parent
        .into_iter()
        .filter(|(name, _)| {
            (BASELINE.contains(&name.as_str()) || declared.iter().any(|d| d == name))
                && !is_credential(name, key_envs)
        })
        .collect()
}

/// The values that must never appear in a journal: every credential-named variable's value
/// in the parent's environment (the pack cannot see them, but a pack may echo what it was
/// told, and a message may quote a request).
pub fn secrets(
    key_envs: &[String],
    parent: impl IntoIterator<Item = (String, String)>,
) -> Vec<String> {
    parent
        .into_iter()
        .filter(|(name, value)| is_credential(name, key_envs) && value.len() >= 4)
        .map(|(_, value)| value)
        .collect()
}

/// `text` with every secret value replaced, and with the value after `Bearer`, or after a
/// `name=` or `name:` whose name is credential-shaped (`token`, `password` and `secret` too),
/// replaced.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret, "[redacted]");
    }
    let mut words: Vec<String> = Vec::new();
    let mut previous_bearer = false;
    for word in out.split(' ') {
        let lower = word.to_ascii_lowercase();
        let redacted = if previous_bearer && !word.is_empty() {
            "[redacted]".to_string()
        } else if let Some(at) = word.find(['=', ':'])
            && at > 0
            && at + 1 < word.len()
            && credential_shaped(&lower[..at])
        {
            format!("{}{}[redacted]", &word[..at], &word[at..=at])
        } else {
            word.to_string()
        };
        previous_bearer = lower == "bearer";
        words.push(redacted);
    }
    words.join(" ")
}

fn credential_shaped(name: &str) -> bool {
    let name = name.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    [
        "key",
        "token",
        "secret",
        "password",
        "passwd",
        "authorization",
    ]
    .iter()
    .any(|w| name.ends_with(w))
}
