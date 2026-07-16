//! Host-brokered model-list egress (CPE-447) — the AI Console fetches a reseller's model catalog
//! through the host, allow-listed, never as a sidecar-supplied URL.
//!
//! Selecting "any model" (CPE-444) means enumerating each reseller's `/models` endpoint. The
//! sandboxed sidecar has no network client, so it asks the host `host.list_models {reseller}`; the
//! host maps `reseller` → an endpoint from the fixed **allow-list** below and performs the GET. That
//! is the SSRF guarantee — the same shape as `keyverify` (CPE-347) and `forge_egress` (CPE-433):
//! the sidecar never supplies a URL, so this can't become a general fetch. Offline/proxy aware,
//! token never logged.

/// A reseller's model-list endpoint: an authenticated GET returning its model list. `auth_header` +
/// `auth_prefix` carry the key; `extra` are any constant headers the reseller requires.
pub struct ModelsEndpoint {
    pub url: &'static str,
    pub auth_header: &'static str,
    pub auth_prefix: &'static str,
    pub extra: &'static [(&'static str, &'static str)],
}

/// The allow-listed model-list endpoint for `reseller` (case-insensitive, leading-segment match so
/// `openrouter-free` still maps to `openrouter`), or `None` when we broker none. This host-owned
/// table — not the sidecar's request — is the security boundary; it mirrors the reseller manifests
/// in `sidecar/ai-console/resellers/*.json` but the host copy is authoritative.
pub fn models_endpoint(reseller: &str) -> Option<ModelsEndpoint> {
    let r = reseller.to_ascii_lowercase();
    let is = |needle: &str| r == needle || r.starts_with(&format!("{needle}-"));
    let bearer = |url| Some(ModelsEndpoint { url, auth_header: "authorization", auth_prefix: "Bearer ", extra: &[] });
    // github-models must be checked before a hypothetical `github` prefix rule; it also needs the
    // API-version header.
    if is("github-models") {
        Some(ModelsEndpoint {
            url: "https://models.github.ai/catalog/models",
            auth_header: "authorization",
            auth_prefix: "Bearer ",
            extra: &[("x-github-api-version", "2026-03-10"), ("accept", "application/vnd.github+json")],
        })
    } else if is("openrouter") {
        bearer("https://openrouter.ai/api/v1/models")
    } else if is("together") {
        bearer("https://api.together.xyz/v1/models")
    } else if is("fireworks") {
        bearer("https://api.fireworks.ai/inference/v1/models")
    } else if is("groq") {
        bearer("https://api.groq.com/openai/v1/models")
    } else if is("deepinfra") {
        bearer("https://api.deepinfra.com/v1/openai/models")
    } else if is("novita") {
        bearer("https://api.novita.ai/v3/openai/models")
    } else if is("aimlapi") {
        bearer("https://api.aimlapi.com/v1/models")
    } else if is("wavespeed") {
        bearer("https://api.wavespeed.ai/v1/models")
    } else if is("cerebras") {
        bearer("https://api.cerebras.ai/v1/models")
    } else if is("sambanova") {
        bearer("https://api.sambanova.ai/v1/models")
    } else if is("nebius") {
        bearer("https://api.studio.nebius.ai/v1/models")
    } else if is("hyperbolic") {
        bearer("https://api.hyperbolic.xyz/v1/models")
    } else {
        None
    }
}

/// Why a model-list fetch was refused before any request ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsEgressError {
    /// No brokered model list for this reseller.
    UnknownReseller,
    /// Offline mode — no outbound call made.
    Offline,
    /// The call could not complete (transport error).
    Transport,
}

/// Fetch a reseller's model list on the sidecar's behalf, returning `(status, body)`. Honours
/// `CPE_OFFLINE` and the proxy env (reusing `keyverify`), attaches the token in the reseller's auth
/// header, and never logs it. Only compiled with the sidecar platform (needs `ureq`).
#[cfg(feature = "sidecar-platform")]
pub fn list_models(reseller: &str, token: Option<&str>) -> Result<(u16, String), ModelsEgressError> {
    use crate::keyverify::{host_of, is_offline, resolve_proxy};
    use std::io::Read;

    if is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Err(ModelsEgressError::Offline);
    }
    let ep = models_endpoint(reseller).ok_or(ModelsEgressError::UnknownReseller)?;
    let mut builder = ureq::AgentBuilder::new();
    if let Some(proxy_url) = resolve_proxy(host_of(ep.url), |k| std::env::var(k).ok()) {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let agent = builder.build();
    let mut req = agent
        .get(ep.url)
        .timeout(std::time::Duration::from_secs(20))
        .set("accept", "application/json");
    if let Some(t) = token {
        req = req.set(ep.auth_header, &format!("{}{}", ep.auth_prefix, t));
    }
    for (k, v) in ep.extra {
        req = req.set(k, v);
    }
    let read_body = |resp: ureq::Response| -> (u16, String) {
        const CAP: u64 = 8 * 1024 * 1024; // bound the response so a hostile reseller can't OOM us
        let status = resp.status();
        let mut buf = String::new();
        let _ = resp.into_reader().take(CAP).read_to_string(&mut buf);
        (status, buf)
    };
    match req.call() {
        Ok(resp) => Ok(read_body(resp)),
        Err(ureq::Error::Status(code, resp)) => Ok((code, read_body(resp).1)),
        Err(ureq::Error::Transport(_)) => Err(ModelsEgressError::Transport),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_covers_known_resellers_and_leading_segments() {
        assert_eq!(models_endpoint("openrouter").unwrap().url, "https://openrouter.ai/api/v1/models");
        assert_eq!(models_endpoint("OpenRouter").unwrap().url, "https://openrouter.ai/api/v1/models");
        assert!(models_endpoint("openrouter-free").is_some()); // leading-segment match
        assert_eq!(models_endpoint("groq").unwrap().url, "https://api.groq.com/openai/v1/models");
        // github-models carries the API-version header and isn't swallowed by a github prefix.
        let gh = models_endpoint("github-models").unwrap();
        assert_eq!(gh.url, "https://models.github.ai/catalog/models");
        assert!(gh.extra.iter().any(|(k, _)| *k == "x-github-api-version"));
    }

    #[test]
    fn every_advertised_reseller_resolves_and_uses_https() {
        for r in [
            "openrouter", "together", "fireworks", "groq", "deepinfra", "novita", "aimlapi",
            "wavespeed", "github-models", "cerebras", "sambanova", "nebius", "hyperbolic",
        ] {
            let ep = models_endpoint(r).unwrap_or_else(|| panic!("{r} should be allow-listed"));
            assert!(ep.url.starts_with("https://"), "{r} must be https");
        }
    }

    #[test]
    fn an_unknown_reseller_has_no_endpoint() {
        assert!(models_endpoint("myspace").is_none());
        assert!(models_endpoint("").is_none());
    }
}
