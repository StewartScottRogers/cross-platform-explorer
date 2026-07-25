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

/// The bare host of a URL (no scheme, userinfo, port, or path). Pure — used to match against
/// `NO_PROXY` and to pick a proxy per request.
pub fn host_of(url: &str) -> &str {
    let after = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host_port = after.split(['/', '?', '#']).next().unwrap_or(after);
    let host_port = host_port.rsplit_once('@').map(|(_, h)| h).unwrap_or(host_port);
    host_port.split(':').next().unwrap_or(host_port)
}

/// True if `host` is excluded from proxying by a `NO_PROXY` list (comma/space separated). Entries
/// match case-insensitively by exact host, by domain suffix (a bare or dot-prefixed domain covers
/// its subdomains), or `*` (everything). Pure.
pub fn host_matches_no_proxy(host: &str, list: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    for raw in list.split([',', ' ']).map(str::trim).filter(|s| !s.is_empty()) {
        if raw == "*" {
            return true;
        }
        let entry = raw.trim_start_matches('.').trim_end_matches('.').to_ascii_lowercase();
        if !entry.is_empty() && (h == entry || h.ends_with(&format!(".{entry}"))) {
            return true;
        }
    }
    false
}

/// Resolve the proxy URL for an HTTPS request to `host` from environment-style lookups, following
/// the de-facto curl convention: `NO_PROXY` excludes a host; otherwise `HTTPS_PROXY` then
/// `ALL_PROXY` (either case) selects the proxy. `None` = connect directly. `env` is injected so
/// this is pure and unit-testable. The key never reaches the proxy in cleartext — HTTPS is
/// tunnelled through a CONNECT, so the proxy sees only the host, not the auth header.
pub fn resolve_proxy<F: Fn(&str) -> Option<String>>(host: &str, env: F) -> Option<String> {
    let no_proxy = env("NO_PROXY").or_else(|| env("no_proxy"));
    if let Some(list) = no_proxy {
        if host_matches_no_proxy(host, &list) {
            return None;
        }
    }
    ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .into_iter()
        .find_map(env)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether an offline-mode value is truthy (`1`/`true`/`yes`/`on`, case-insensitive). Pure.
pub fn is_offline(val: Option<String>) -> bool {
    matches!(
        val.as_deref().map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Ask the provider whether `key` is valid, making the single allow-listed GET. Returns a
/// [`Verdict`]; `live:false` when offline, when there is no endpoint for `provider`, or when the
/// call couldn't complete. Honours `CPE_OFFLINE` and `HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` (CPE-369).
/// Only compiled with the sidecar platform (needs the optional `ureq` dependency).
#[cfg(feature = "sidecar-platform")]
pub fn verify_live(provider: &str, key: &str) -> Verdict {
    // Offline / air-gapped: make no outbound call at all, and say so (never a failed check).
    if is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return (true, false, "Offline mode — key not checked with the provider.".into());
    }
    let Some(ep) = verify_endpoint(provider) else {
        return (true, false, format!("No live check available for \"{provider}\"."));
    };
    let agent = build_agent(host_of(ep.url));
    let mut req = agent
        .get(ep.url)
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

/// A ureq agent that routes through the system proxy for `host` when one is configured (CPE-369).
/// A malformed proxy URL is ignored (connect directly) rather than failing the check.
#[cfg(feature = "sidecar-platform")]
fn build_agent(host: &str) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new();
    if let Some(proxy_url) = resolve_proxy(host, |k| std::env::var(k).ok()) {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    builder.build()
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

    #[test]
    fn host_of_strips_scheme_port_userinfo_and_path() {
        assert_eq!(host_of("https://openrouter.ai/api/v1/key"), "openrouter.ai");
        assert_eq!(host_of("https://user:pw@api.openai.com:443/v1/models"), "api.openai.com");
        assert_eq!(host_of("api.anthropic.com"), "api.anthropic.com");
    }

    #[test]
    fn no_proxy_matches_exact_suffix_and_wildcard() {
        assert!(host_matches_no_proxy("api.openai.com", "api.openai.com"));
        assert!(host_matches_no_proxy("api.openai.com", ".openai.com")); // domain suffix
        assert!(host_matches_no_proxy("api.openai.com", "openai.com")); // bare domain covers subs
        assert!(host_matches_no_proxy("anything", "*"));
        assert!(host_matches_no_proxy("a.corp", "example.com, corp")); // list, spaces
        assert!(!host_matches_no_proxy("api.openai.com", "openai.org"));
        assert!(!host_matches_no_proxy("notopenai.com", "openai.com")); // not a real suffix
    }

    #[test]
    fn resolve_proxy_prefers_https_then_all_and_honours_no_proxy() {
        let env = |map: &[(&str, &str)]| {
            let owned: Vec<(String, String)> =
                map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            move |k: &str| owned.iter().find(|(ek, _)| ek == k).map(|(_, v)| v.clone())
        };
        // HTTPS_PROXY selected.
        assert_eq!(
            resolve_proxy("openrouter.ai", env(&[("HTTPS_PROXY", "http://p:8080")])),
            Some("http://p:8080".into())
        );
        // ALL_PROXY as fallback when no HTTPS_PROXY.
        assert_eq!(
            resolve_proxy("openrouter.ai", env(&[("ALL_PROXY", "http://all:3128")])),
            Some("http://all:3128".into())
        );
        // NO_PROXY excludes the host → direct.
        assert_eq!(
            resolve_proxy(
                "openrouter.ai",
                env(&[("HTTPS_PROXY", "http://p:8080"), ("NO_PROXY", "openrouter.ai")])
            ),
            None
        );
        // Nothing configured → direct.
        assert_eq!(resolve_proxy("openrouter.ai", env(&[])), None);
        // Empty value is ignored.
        assert_eq!(resolve_proxy("openrouter.ai", env(&[("HTTPS_PROXY", "  ")])), None);
    }

    #[test]
    fn offline_flag_parsing() {
        for v in ["1", "true", "YES", "On"] {
            assert!(is_offline(Some(v.into())), "{v} should be offline");
        }
        for v in ["0", "false", "", "off"] {
            assert!(!is_offline(Some(v.into())), "{v} should not be offline");
        }
        assert!(!is_offline(None));
    }
}
