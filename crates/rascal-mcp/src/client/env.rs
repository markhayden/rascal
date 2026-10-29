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
/// replaced. Words are split at whitespace and at the URL and list separators `?`, `&`, `;`
/// and `,`, so a credential inside a query string is found too.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret, "[redacted]");
    }
    let is_separator = |c: char| c.is_whitespace() || matches!(c, '?' | '&' | ';' | ',');
    let mut result = String::with_capacity(out.len());
    let mut previous_bearer = false;
    let mut rest = out.as_str();
    while !rest.is_empty() {
        let end = rest.find(is_separator).unwrap_or(rest.len());
        let (word, tail) = rest.split_at(end);
        let lower = word.to_ascii_lowercase();
        if previous_bearer && !word.is_empty() {
            result.push_str("[redacted]");
        } else if let Some(at) = word.find(['=', ':'])
            && at > 0
            && at + 1 < word.len()
            && credential_shaped(&lower[..at])
        {
            result.push_str(&word[..=at]);
            result.push_str("[redacted]");
        } else {
            result.push_str(word);
        }
        if !word.is_empty() {
            previous_bearer = lower == "bearer";
        }
        // The separator itself, if any.
        let mut chars = tail.chars();
        match chars.next() {
            Some(c) => {
                result.push(c);
                rest = chars.as_str();
            }
            None => rest = tail,
        }
    }
    result
}

/// The parent's environment as strings; a variable that is not Unicode is skipped (it could
/// not be declared by a plain name either).
pub fn parent_environment() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
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
