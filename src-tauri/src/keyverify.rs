//! Live provider API-key verification for the AI Console sidecar (CPE-347).
//!
//! The sandboxed sidecar has no TLS client and must not reach the network directly, so it asks
//! the host (`host.verify_key`) to confirm a key. The host makes **one** authenticated GET to a
//! provider endpoint chosen *here* — never a URL supplied by the sidecar. That is the whole point
//! of the allow-list below: `host.verify_key` can only ever hit these three endpoints, so it is a
//! narrow key-check, not a general fetch primitive (no SSRF). It complements the offline shape
//! check in the sidecar's `keycheck` module: shape first (cheap, no network), then this.

/// A provider's key-check endpoint: a lightweight authenticated GET that returns 2xx iff the key
/// is accepted. `auth_header` carries the key (prefixed by `auth_prefix`); `extra` are any
/// constant headers the provider requires.
pub struct VerifyEndpoint {
    pub url: &'static str,
    pub auth_header: &'static str,
    pub auth_prefix: &'static str,
    pub extra: &'static [(&'static str, &'static str)],
}

/// The allow-listed endpoint for `provider`, or `None` when we have no live check for it (matched
/// case-insensitively and by leading segment, so `openrouter-free` still maps to `openrouter`).
pub fn verify_endpoint(provider: &str) -> Option<VerifyEndpoint> {
    let p = provider.to_ascii_lowercase();
    let matches = |needle: &str| p == needle || p.starts_with(&format!("{needle}-"));
    if matches("openrouter") {
        Some(VerifyEndpoint {
            url: "https://openrouter.ai/api/v1/key",
            auth_header: "authorization",
            auth_prefix: "Bearer ",
            extra: &[],
        })
    } else if matches("openai") {
        Some(VerifyEndpoint {
            url: "https://api.openai.com/v1/models",
            auth_header: "authorization",
            auth_prefix: "Bearer ",
            extra: &[],
        })
    } else if matches("anthropic") {
        Some(VerifyEndpoint {
            url: "https://api.anthropic.com/v1/models",
            auth_header: "x-api-key",
            auth_prefix: "",
            extra: &[("anthropic-version", "2023-06-01")],
        })
    } else {
        None
    }
}

/// The verdict passed back to the sidecar: `(valid, live, detail)`.
///
/// `live` = "the provider gave a definitive answer". Only `live && !valid` is a real rejection
/// (show it in red). A non-definitive outcome — rate-limit, transient error, unexpected status —
/// is reported as `live:false` so a hiccup never blocks the user from saving a key they believe
/// is correct; `valid` is left `true` there and the UI treats it as "format OK, not confirmed".
pub type Verdict = (bool, bool, String);

/// Turn an HTTP status into a [`Verdict`]. Pure, so the mapping is unit-tested without a network.
pub fn interpret_status(status: u16) -> Verdict {
    match status {
        200..=299 => (true, true, "Key verified with the provider.".into()),
        401 | 403 => (false, true, "Provider rejected this key (unauthorized).".into()),
        429 => (true, false, "Couldn't verify — the provider rate-limited the check.".into()),
        s => (true, false, format!("Couldn't verify — the provider returned HTTP {s}.")),
    }
}

/// Ask the provider whether `key` is valid, making the single allow-listed GET. Returns a
/// [`Verdict`]; `live:false` when there is no endpoint for `provider` or the call couldn't
/// complete. Only compiled with the sidecar platform (needs the optional `ureq` dependency).
#[cfg(feature = "sidecar-platform")]
pub fn verify_live(provider: &str, key: &str) -> Verdict {
    let Some(ep) = verify_endpoint(provider) else {
        return (true, false, format!("No live check available for \"{provider}\"."));
    };
    let mut req = ureq::get(ep.url)
        .timeout(std::time::Duration::from_secs(12))
        .set(ep.auth_header, &format!("{}{}", ep.auth_prefix, key));
    for (k, v) in ep.extra {
        req = req.set(k, v);
    }
    match req.call() {
        Ok(resp) => interpret_status(resp.status()),
        // ureq surfaces non-2xx as Status; a 401 here is a decisive "invalid key".
        Err(ureq::Error::Status(code, _)) => interpret_status(code),
        Err(ureq::Error::Transport(t)) => {
            (true, false, format!("Couldn't reach the provider ({}).", transport_reason(&t)))
        }
    }
}

/// A short, user-facing reason from a ureq transport error, without leaking the target URL.
#[cfg(feature = "sidecar-platform")]
fn transport_reason(t: &ureq::Transport) -> String {
    t.message().unwrap_or("network error").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_allow_list_covers_known_providers_and_leading_segments() {
        assert_eq!(verify_endpoint("openrouter").unwrap().url, "https://openrouter.ai/api/v1/key");
        assert_eq!(verify_endpoint("OpenAI").unwrap().url, "https://api.openai.com/v1/models");
        // Leading-segment match, mirroring keycheck's prefix rule.
        assert!(verify_endpoint("openrouter-free").is_some());
        // Anthropic needs its version header carried through.
        let a = verify_endpoint("anthropic").unwrap();
        assert_eq!(a.auth_header, "x-api-key");
        assert_eq!(a.extra, &[("anthropic-version", "2023-06-01")]);
    }

    #[test]
    fn unknown_provider_has_no_live_endpoint() {
        assert!(verify_endpoint("mistral").is_none());
        assert!(verify_endpoint("some-local-thing").is_none());
    }

    #[test]
    fn status_2xx_is_a_definitive_valid() {
        assert_eq!(interpret_status(200), (true, true, "Key verified with the provider.".into()));
    }

    #[test]
    fn status_401_and_403_are_definitive_rejections() {
        for code in [401u16, 403] {
            let (valid, live, _) = interpret_status(code);
            assert!(!valid && live, "HTTP {code} should be a live rejection");
        }
    }

    #[test]
    fn transient_statuses_are_not_treated_as_rejections() {
        // Rate-limit / unexpected codes must not block a save: not-live, and not-invalid.
        for code in [429u16, 500, 502] {
            let (valid, live, _) = interpret_status(code);
            assert!(valid && !live, "HTTP {code} should be inconclusive, not a rejection");
        }
    }
}
