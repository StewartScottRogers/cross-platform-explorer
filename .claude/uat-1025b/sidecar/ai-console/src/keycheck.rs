//! Provider API-key format pre-check (CPE-345).
//!
//! A cheap, offline sanity check run before a key is stored, so the user gets immediate
//! feedback on an obviously-wrong paste (empty, or the wrong shape for a provider with a
//! well-known key format). It is deliberately lenient: unknown providers accept any
//! non-empty key. It is NOT a live verification against the provider — that needs the
//! Network capability and is tracked in CPE-347.

/// The well-known key prefix for a provider, if it has one. Matched against the provider
/// id (case-insensitive, and by a leading segment so `openrouter-local` still matches
/// `openrouter`).
fn known_prefix(provider: &str) -> Option<&'static str> {
    let p = provider.to_ascii_lowercase();
    let matches = |needle: &str| p == needle || p.starts_with(&format!("{needle}-"));
    if matches("openrouter") {
        Some("sk-or-")
    } else if matches("anthropic") {
        Some("sk-ant-")
    } else if matches("openai") {
        Some("sk-")
    } else {
        None
    }
}

/// Validate a pasted key's *shape* for `provider`. `Ok(())` means "plausible" (not
/// "verified"); `Err(reason)` is a user-facing explanation of an obvious problem.
pub fn check_key_format(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Key is empty.".into());
    }
    if key.len() < 8 {
        return Err("Key looks too short.".into());
    }
    if key.chars().any(|c| c.is_whitespace()) {
        return Err("Key contains spaces — check the paste.".into());
    }
    if let Some(prefix) = known_prefix(provider) {
        if !key.starts_with(prefix) {
            return Err(format!("A {provider} key normally starts with \"{prefix}\"."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_short_and_spaced_keys() {
        assert!(check_key_format("openai", "").is_err());
        assert!(check_key_format("openai", "sk-1").is_err()); // too short
        assert!(check_key_format("openai", "sk-abc def ghi").is_err()); // space
    }

    #[test]
    fn enforces_known_prefixes() {
        assert!(check_key_format("openrouter", "sk-or-abcdef123456").is_ok());
        assert!(check_key_format("openrouter", "sk-abcdef123456").is_err());
        assert!(check_key_format("anthropic", "sk-ant-abcdef123456").is_ok());
        assert!(check_key_format("anthropic", "nope-abcdef123456").is_err());
        assert!(check_key_format("openai", "sk-abcdef123456").is_ok());
    }

    #[test]
    fn matches_prefix_by_leading_segment() {
        // A provider id like "openrouter-free" still enforces the openrouter prefix.
        assert!(check_key_format("openrouter-free", "sk-or-xyz12345").is_ok());
        assert!(check_key_format("openrouter-free", "bad-xyz12345").is_err());
    }

    #[test]
    fn unknown_providers_accept_any_plausible_key() {
        assert!(check_key_format("mistral", "abcdef123456").is_ok());
        assert!(check_key_format("some-local-thing", "a-very-long-token-value").is_ok());
    }
}
