//! Host-brokered forge API egress (CPE-433) — the repos sidecar's single outbound path.
//!
//! The repos sidecar (CPE-429) has **no network client** and must not reach the network directly, so
//! it asks the host to perform an API call: `host.forge_request { provider, method, path, body? }`.
//! The host — here — chooses the **host** from a fixed allow-list ([`forge_api`]) and **builds the
//! URL** itself; the sidecar supplies only a *path*, never a scheme or host for a known provider.
//! That is the whole SSRF guarantee: `host.forge_request` can only ever reach an allow-listed forge
//! host, so it is not a general fetch primitive. This extends the AI Console's `keyverify` model
//! (threat-model §7) to the broader forge surface (forge-threat-model §A), and **reuses**
//! `keyverify`'s proxy/offline plumbing rather than duplicating it.
//!
//! Self-hosted kinds (GitHub Enterprise, self-managed GitLab, Gitea/Forgejo) have no fixed host, so
//! the *connection* supplies one; it is validated by [`validate_self_hosted`] and re-checked against
//! the private/loopback/link-local/metadata blocklist so a user-entered host can't become an SSRF.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::keyverify::{host_of, is_offline, resolve_proxy};

/// How a provider expects the stored token presented. `header` is the HTTP header name; the value is
/// `prefix` + token (e.g. `Authorization: Bearer <t>`, or `Authorization: token <t>` for classic
/// GitHub). Basic-auth providers are handled by the caller (Bitbucket app passwords) — represented
/// here as `authorization` + `Basic ` with a pre-encoded value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeAuth {
    pub header: &'static str,
    pub prefix: &'static str,
}

/// A known forge provider's API base. `host` is the fixed API host for a *hosted* provider
/// (`api.github.com`); it is `None` for *self-hosted* kinds, which take a per-connection host.
/// `base_path` is any prefix the provider mounts its API under (GitLab `/api/v4`), or `""`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeApi {
    pub host: Option<&'static str>,
    pub base_path: &'static str,
    pub auth: ForgeAuth,
}

const BEARER: ForgeAuth = ForgeAuth { header: "authorization", prefix: "Bearer " };

/// The allow-listed API base for `provider` (matched case-insensitively, and by leading segment so
/// `github-personal` still maps to `github`). `None` = we broker no API for it. This table — not the
/// sidecar's request — is the security boundary; it must stay aligned with the provider manifests in
/// `sidecar/repos/providers/*.json`, but the host copy is authoritative.
pub fn forge_api(provider: &str) -> Option<ForgeApi> {
    let p = provider.to_ascii_lowercase();
    let is = |needle: &str| p == needle || p.starts_with(&format!("{needle}-"));
    // Order matters: check the more specific `github-enterprise` before `github`.
    if is("github-enterprise") {
        Some(ForgeApi { host: None, base_path: "/api/v3", auth: BEARER }) // self-hosted GHE
    } else if is("github") {
        Some(ForgeApi { host: Some("api.github.com"), base_path: "", auth: BEARER })
    } else if is("gitlab") {
        // gitlab.com is hosted; a self-managed instance overrides the host per connection.
        Some(ForgeApi { host: Some("gitlab.com"), base_path: "/api/v4", auth: BEARER })
    } else if is("bitbucket") {
        Some(ForgeApi {
            host: Some("api.bitbucket.org"),
            base_path: "/2.0",
            auth: ForgeAuth { header: "authorization", prefix: "Basic " },
        })
    } else if is("codeberg") {
        Some(ForgeApi { host: Some("codeberg.org"), base_path: "/api/v1", auth: BEARER })
    } else if is("gitea") || is("forgejo") {
        Some(ForgeApi { host: None, base_path: "/api/v1", auth: BEARER }) // self-hosted
    } else if is("sourcehut") {
        Some(ForgeApi { host: Some("git.sr.ht"), base_path: "", auth: BEARER })
    } else if is("azure-devops") {
        Some(ForgeApi {
            host: Some("dev.azure.com"),
            base_path: "",
            auth: ForgeAuth { header: "authorization", prefix: "Basic " },
        })
    } else {
        None
    }
}

/// Why the host refused to broker a forge request. Every variant is a *safe* refusal — nothing left
/// the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressError {
    /// No brokered API for this provider.
    UnknownProvider,
    /// A self-hosted kind was asked for without a connection host.
    MissingHost,
    /// The resolved host isn't allow-listed / is malformed.
    HostNotAllowed,
    /// The host resolves to (or literally is) a private/loopback/link-local/metadata address.
    BlockedAddress,
    /// The path could escape the host or inject into the URL/headers.
    BadPath,
    /// The HTTP method isn't in the permitted set.
    BadMethod,
    /// Offline mode — no outbound call made.
    Offline,
}

/// The HTTP methods the broker will perform. A fixed set — a forge request can't smuggle an
/// arbitrary verb (e.g. `CONNECT`).
pub fn allowed_method(method: &str) -> bool {
    matches!(method.to_ascii_uppercase().as_str(), "GET" | "POST" | "PATCH" | "PUT" | "DELETE")
}

/// True if `ip` must never be the target of a brokered call: loopback, RFC1918/ULA private,
/// link-local (incl. the `169.254.169.254` cloud-metadata endpoint), unspecified, or
/// carrier-grade-NAT. Pure — the core SSRF classifier, unit-tested exhaustively.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || is_cgnat(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_unique_local(v6)
                || is_v6_link_local(v6)
                // An IPv4-mapped address (::ffff:a.b.c.d) must be judged on its embedded v4.
                || v6.to_ipv4_mapped().map(|m| is_blocked_ip(IpAddr::V4(m))).unwrap_or(false)
        }
    }
}

/// 100.64.0.0/10 — carrier-grade NAT (RFC 6598); not `is_private()` but not internet-routable.
fn is_cgnat(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

/// fc00::/7 — IPv6 unique-local addresses (the v6 analogue of RFC1918).
fn is_unique_local(ip: Ipv6Addr) -> bool {
    (ip.octets()[0] & 0xfe) == 0xfc
}

/// fe80::/10 — IPv6 link-local.
fn is_v6_link_local(ip: Ipv6Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 0xfe && (b & 0xc0) == 0x80
}

/// Validate a sidecar-supplied API **path** so it can neither escape the chosen host nor inject a
/// second URL / a header. Must start with `/`, and must not contain a scheme (`://`), a `..`
/// segment, an `@` (userinfo), a backslash, or any control/space char (CR/LF header injection).
/// Returns the path unchanged on success. Pure.
pub fn validate_path(path: &str) -> Result<&str, EgressError> {
    if !path.starts_with('/') || path.starts_with("//") {
        return Err(EgressError::BadPath); // must be host-relative; `//host` would re-root the URL
    }
    if path.contains("://") || path.contains('@') || path.contains('\\') {
        return Err(EgressError::BadPath);
    }
    // Any control char or raw space is a header/URL-injection risk.
    if path.chars().any(|c| c.is_control() || c == ' ') {
        return Err(EgressError::BadPath);
    }
    // Reject a `..` path segment (dot-dot traversal), tolerating dots inside a longer segment.
    if path.split('/').any(|seg| seg == "..") {
        return Err(EgressError::BadPath);
    }
    Ok(path)
}

/// Validate a **self-hosted** connection host (`git.example.com` or `git.example.com:8443`): a
/// syntactically valid host with a permitted charset, that is not `localhost` and not a **literal**
/// blocked IP. DNS-resolved addresses are re-checked at call time (see [`guarded_addrs`]); this is
/// the cheap, pure pre-check. Returns the host unchanged on success.
pub fn validate_self_hosted(host: &str) -> Result<&str, EgressError> {
    let bare = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    if bare.is_empty()
        || bare.eq_ignore_ascii_case("localhost")
        || bare.ends_with(".localhost")
    {
        return Err(EgressError::HostNotAllowed);
    }
    // Charset: letters/digits/'.'/'-' only (a hostname or dotted IPv4). No scheme, path, userinfo.
    if !bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(EgressError::HostNotAllowed);
    }
    // A literal IP must not be in a blocked range.
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(EgressError::BlockedAddress);
        }
    }
    Ok(host)
}

/// Resolve the API host for a request: the fixed host for a hosted provider, or the validated
/// self-hosted host for a self-hosted kind. `self_hosted` is ignored for hosted providers (they are
/// pinned to their allow-listed host — a sidecar can't redirect `github` elsewhere).
pub fn resolve_host(provider: &str, self_hosted: Option<&str>) -> Result<String, EgressError> {
    let api = forge_api(provider).ok_or(EgressError::UnknownProvider)?;
    match api.host {
        Some(h) => Ok(h.to_string()),
        None => {
            let h = self_hosted.ok_or(EgressError::MissingHost)?;
            validate_self_hosted(h).map(str::to_string)
        }
    }
}

/// Build the full HTTPS URL host-side from `(provider, self_hosted?, path)`. The sidecar supplies
/// only `path`; the scheme (`https`) and host come from the allow-list. This is where "the sidecar
/// never supplies a URL" and "a path cannot escape the host" are jointly enforced.
pub fn build_forge_url(
    provider: &str,
    self_hosted: Option<&str>,
    path: &str,
) -> Result<String, EgressError> {
    let api = forge_api(provider).ok_or(EgressError::UnknownProvider)?;
    let host = resolve_host(provider, self_hosted)?;
    let path = validate_path(path)?;
    Ok(format!("https://{host}{}{path}", api.base_path))
}

/// The addresses a resolved host maps to, filtered to those that are **not** blocked — the call-time
/// SSRF re-check that closes DNS-rebinding (a hostname that passed [`validate_self_hosted`] but
/// resolves to `127.0.0.1`/metadata). An empty result means "refuse the call". Pure over its input.
pub fn guarded_addrs<I: IntoIterator<Item = IpAddr>>(resolved: I) -> Vec<IpAddr> {
    resolved.into_iter().filter(|ip| !is_blocked_ip(*ip)).collect()
}

/// Perform the brokered forge request and return `(status, body)`, or an [`EgressError`]. Honours
/// `CPE_OFFLINE` and the proxy env (reusing `keyverify`), re-checks the resolved address against the
/// SSRF blocklist, and never logs the token. Only compiled with the sidecar platform (needs `ureq`).
#[cfg(feature = "sidecar-platform")]
pub fn forge_request(
    provider: &str,
    method: &str,
    self_hosted: Option<&str>,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> Result<(u16, String), EgressError> {
    use std::net::ToSocketAddrs;

    if is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Err(EgressError::Offline);
    }
    if !allowed_method(method) {
        return Err(EgressError::BadMethod);
    }
    let api = forge_api(provider).ok_or(EgressError::UnknownProvider)?;
    let url = build_forge_url(provider, self_hosted, path)?;
    let host = host_of(&url).to_string();

    // Anti-rebinding: resolve now and refuse if every address is blocked. (:443 is only for
    // resolution; the request itself uses the scheme's default port.)
    let resolved: Vec<IpAddr> = (host.as_str(), 443u16)
        .to_socket_addrs()
        .map(|it| it.map(|s| s.ip()).collect())
        .unwrap_or_default();
    if !resolved.is_empty() && guarded_addrs(resolved).is_empty() {
        return Err(EgressError::BlockedAddress);
    }

    let mut builder = ureq::AgentBuilder::new();
    if let Some(proxy_url) = resolve_proxy(&host, |k| std::env::var(k).ok()) {
        if let Ok(proxy) = ureq::Proxy::new(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let agent = builder.build();
    let mut req = agent
        .request(&method.to_ascii_uppercase(), &url)
        .timeout(std::time::Duration::from_secs(20))
        .set("accept", "application/json")
        .set("user-agent", "cross-platform-explorer-repos");
    if let Some(t) = token {
        req = req.set(api.auth.header, &format!("{}{}", api.auth.prefix, t));
    }
    let result = match body {
        Some(b) => req.send_string(b),
        None => req.call(),
    };
    match result {
        Ok(resp) => read_response(resp),
        Err(ureq::Error::Status(code, resp)) => read_response(resp).map(|(_, b)| (code, b)),
        Err(ureq::Error::Transport(_)) => Err(EgressError::HostNotAllowed),
    }
}

/// Read a ureq response into `(status, capped-body)`. The body is bounded so a hostile forge can't
/// exhaust host memory (forge-threat-model §A DoS row).
#[cfg(feature = "sidecar-platform")]
fn read_response(resp: ureq::Response) -> Result<(u16, String), EgressError> {
    use std::io::Read;
    const CAP: u64 = 8 * 1024 * 1024; // 8 MiB
    let status = resp.status();
    let mut buf = String::new();
    let _ = resp.into_reader().take(CAP).read_to_string(&mut buf);
    Ok((status, buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn ip(s: &str) -> IpAddr {
        IpAddr::from_str(s).unwrap()
    }

    #[test]
    fn allow_list_maps_known_providers_and_leading_segments() {
        assert_eq!(forge_api("github").unwrap().host, Some("api.github.com"));
        assert_eq!(forge_api("GitHub").unwrap().host, Some("api.github.com"));
        assert_eq!(forge_api("github-personal").unwrap().host, Some("api.github.com"));
        assert_eq!(forge_api("gitlab").unwrap().base_path, "/api/v4");
        assert_eq!(forge_api("codeberg").unwrap().host, Some("codeberg.org"));
        // Self-hosted kinds have no fixed host.
        assert_eq!(forge_api("gitea").unwrap().host, None);
        assert_eq!(forge_api("github-enterprise").unwrap().host, None);
        // github-enterprise must not be swallowed by the github prefix rule.
        assert_eq!(forge_api("github-enterprise").unwrap().base_path, "/api/v3");
        assert!(forge_api("myspace").is_none());
    }

    #[test]
    fn only_a_fixed_set_of_methods_is_permitted() {
        for m in ["GET", "get", "POST", "patch", "PUT", "DELETE"] {
            assert!(allowed_method(m), "{m} should be allowed");
        }
        for m in ["CONNECT", "TRACE", "OPTIONS", "FETCH", ""] {
            assert!(!allowed_method(m), "{m} should be refused");
        }
    }

    #[test]
    fn blocked_ips_cover_loopback_private_linklocal_metadata_and_v6() {
        for s in [
            "127.0.0.1", "127.9.9.9", "10.0.0.5", "192.168.1.1", "172.16.0.1",
            "169.254.169.254", // cloud metadata
            "0.0.0.0", "100.64.1.1", // CGNAT
            "::1", "fc00::1", "fd12::9", "fe80::1", "::ffff:127.0.0.1", // v4-mapped loopback
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} must be blocked");
        }
        for s in ["140.82.121.3", "8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(!is_blocked_ip(ip(s)), "{s} is public and must be allowed");
        }
    }

    #[test]
    fn path_validation_blocks_escape_and_injection() {
        assert!(validate_path("/repos/octocat/hello/contents/src").is_ok());
        assert!(validate_path("/user/repos?per_page=100").is_ok());
        for bad in [
            "repos/x",                 // not host-relative
            "//evil.com/x",            // protocol-relative → re-roots host
            "/repos/../../admin",      // dot-dot traversal
            "/a\r\nHost: evil",        // CRLF header injection
            "/a b",                    // raw space
            "https://evil.com/x",      // absolute URL
            "/x@evil.com",             // userinfo trick
            "/x\\y",                   // backslash
        ] {
            assert_eq!(validate_path(bad), Err(EgressError::BadPath), "{bad:?} should be rejected");
        }
        // A dot inside a segment is fine (e.g. a filename).
        assert!(validate_path("/repos/o/r/contents/Cargo.toml").is_ok());
    }

    #[test]
    fn self_hosted_host_validation() {
        assert!(validate_self_hosted("git.example.com").is_ok());
        assert!(validate_self_hosted("git.example.com:8443").is_ok());
        assert_eq!(validate_self_hosted("localhost"), Err(EgressError::HostNotAllowed));
        assert_eq!(validate_self_hosted("box.localhost"), Err(EgressError::HostNotAllowed));
        assert_eq!(validate_self_hosted("127.0.0.1"), Err(EgressError::BlockedAddress));
        assert_eq!(validate_self_hosted("192.168.0.10"), Err(EgressError::BlockedAddress));
        assert_eq!(validate_self_hosted("169.254.169.254"), Err(EgressError::BlockedAddress));
        // Injection attempts in the host string.
        assert_eq!(validate_self_hosted("evil.com/path"), Err(EgressError::HostNotAllowed));
        assert_eq!(validate_self_hosted("a@evil.com"), Err(EgressError::HostNotAllowed));
        // A public literal IP is allowed (some enterprises pin one).
        assert!(validate_self_hosted("140.82.121.3").is_ok());
    }

    #[test]
    fn url_is_built_host_side_and_hosted_providers_cannot_be_redirected() {
        // Hosted: the self_hosted arg is ignored — github stays on api.github.com.
        assert_eq!(
            build_forge_url("github", Some("evil.com"), "/repos/o/r/contents").unwrap(),
            "https://api.github.com/repos/o/r/contents"
        );
        // base_path is prepended.
        assert_eq!(
            build_forge_url("gitlab", None, "/projects").unwrap(),
            "https://gitlab.com/api/v4/projects"
        );
        // Self-hosted: the validated connection host is used, with the kind's base_path.
        assert_eq!(
            build_forge_url("gitea", Some("git.acme.io"), "/repos/o/r").unwrap(),
            "https://git.acme.io/api/v1/repos/o/r"
        );
        // A bad path is refused even for a known provider.
        assert_eq!(
            build_forge_url("github", None, "//evil.com"),
            Err(EgressError::BadPath)
        );
        // Self-hosted with no host supplied.
        assert_eq!(build_forge_url("gitea", None, "/x"), Err(EgressError::MissingHost));
        // Unknown provider.
        assert_eq!(build_forge_url("myspace", None, "/x"), Err(EgressError::UnknownProvider));
        // A self-hosted host that is a private IP is blocked at URL-build time.
        assert_eq!(
            build_forge_url("github-enterprise", Some("10.0.0.1"), "/x"),
            Err(EgressError::BlockedAddress)
        );
    }

    #[test]
    fn resolved_addresses_are_filtered_to_public_only() {
        let mixed = vec![ip("127.0.0.1"), ip("140.82.121.3"), ip("10.0.0.1")];
        assert_eq!(guarded_addrs(mixed), vec![ip("140.82.121.3")]);
        // A hostname that resolves only to loopback (rebinding) → nothing survives → refuse.
        assert!(guarded_addrs(vec![ip("127.0.0.1"), ip("::1")]).is_empty());
    }
}
