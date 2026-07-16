//! Generic-Git remote parsing (CPE-498).
//!
//! The known-forge path (`forge_clone`) builds the clone URL host-side from a fixed provider host, so
//! egress is implicitly allow-listed. The **Generic Git** provider instead clones/syncs an *arbitrary*
//! HTTPS/SSH URL the user supplies — which also covers self-hosted forges. That means the host is no
//! longer fixed, so before we let git reach it the host must be **admitted** to the egress allow-list
//! with explicit consent (Q5). This module is the pure, testable core: pull the **host** out of a git
//! URL (for the admission prompt + allow-list key) and hand back a **credential-stripped canonical
//! URL** for the hardened clone builder. It never performs I/O and never holds a token.

/// The transport of a parsed remote. `git://`, `file://`, `ext::`, etc. are not representable — they
/// are rejected by [`parse_remote`], matching the repos clone allow-list (`is_allowed_clone_url`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteScheme {
    Https,
    Ssh,
}

/// An arbitrary git remote, split into the pieces the generic-git path needs: the bare `host` (the
/// allow-list identity + what the user consents to), and a `url` that is the same remote with **any
/// embedded credentials stripped** — safe to log, display, and hand to the hardened clone builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemote {
    pub scheme: RemoteScheme,
    /// Lowercased hostname, **without** userinfo or port — the egress allow-list key.
    pub host: String,
    /// The remote with userinfo removed, otherwise byte-preserving (scheme, host, port, path).
    pub url: String,
}

/// Normalize a hostname for allow-list comparison: lowercase and drop a single trailing dot (the
/// root-label form `host.` and `host` are the same origin). Port/userinfo are never part of a host.
pub fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// A host string is well-formed enough to admit: non-empty, no whitespace, no `@`/`/`, has at least
/// one `.` or is `localhost`, and only DNS-ish characters. Deliberately strict — the host is used in
/// a security decision and reconstructed into a URL.
fn is_plausible_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let bare = host;
    let dnsish = bare.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    dnsish && !bare.starts_with('.') && !bare.starts_with('-') && (bare.contains('.') || bare == "localhost")
}

/// Split `authority` (`[user[:pass]@]host[:port]`) into `(host, port_suffix)` with any userinfo
/// dropped. `host` is bare; `port_suffix` is `""` or `":1234"` so the URL can be rebuilt verbatim.
fn split_authority(authority: &str) -> Option<(String, String)> {
    // Userinfo ends at the LAST '@' before the host (a password may itself contain '@'-free bytes;
    // we only strip, never keep it).
    let hostport = match authority.rsplit_once('@') {
        Some((_userinfo, hp)) => hp,
        None => authority,
    };
    if hostport.is_empty() {
        return None;
    }
    let (host, port_suffix) = match hostport.rsplit_once(':') {
        // Only treat a trailing `:digits` as a port; `host:` or `host:git` (scp path) isn't a port.
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, format!(":{p}")),
        _ => (hostport, String::new()),
    };
    let normalized = normalize_host(host);
    if !is_plausible_host(&normalized) {
        return None;
    }
    Some((normalized, port_suffix))
}

/// Parse an arbitrary git remote URL into its host + a credential-stripped canonical URL, or `None`
/// if it isn't an allowed transport (only `https://`, `ssh://`, and scp-like `[user@]host:path`).
/// Mirrors the repos clone allow-list so a URL that parses here is one the hardened builder accepts.
pub fn parse_remote(url: &str) -> Option<GitRemote> {
    let u = url.trim();
    if u.is_empty() || u.contains(char::is_whitespace) {
        return None;
    }

    // https:// and ssh:// — `scheme://[userinfo@]host[:port]/path`.
    for (prefix, scheme) in [("https://", RemoteScheme::Https), ("ssh://", RemoteScheme::Ssh)] {
        if let Some(rest) = u.strip_prefix(prefix) {
            let (authority, path) = match rest.split_once('/') {
                Some((a, p)) => (a, format!("/{p}")),
                None => (rest, String::new()),
            };
            let (host, port) = split_authority(authority)?;
            let url = format!("{prefix}{host}{port}{path}");
            return Some(GitRemote { scheme, host, url });
        }
    }

    // Reject every other explicit scheme (git://, file://, ext::, http://, …) before the scp guess.
    if u.contains("://") || u.starts_with("ext::") || u.starts_with("file:") {
        return None;
    }

    // scp-like `[user@]host:path` (git's implicit-ssh syntax). The ':' must come before any '/'.
    if let Some((authority, path)) = u.split_once(':') {
        if authority.contains('/') || path.is_empty() {
            return None;
        }
        let (host, port) = split_authority(authority)?;
        if !port.is_empty() {
            return None; // `host:22:path` is ambiguous — not scp-like
        }
        // Canonicalize to the same scp-like form, credentials stripped.
        let url = format!("{host}:{path}");
        return Some(GitRemote { scheme: RemoteScheme::Ssh, host, url });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_and_extracts_bare_host() {
        let r = parse_remote("https://gitlab.example.com/group/proj.git").unwrap();
        assert_eq!(r.scheme, RemoteScheme::Https);
        assert_eq!(r.host, "gitlab.example.com");
        assert_eq!(r.url, "https://gitlab.example.com/group/proj.git");
    }

    #[test]
    fn strips_userinfo_from_the_canonical_url_but_keeps_port() {
        let r = parse_remote("https://user:secret@git.acme.io:8443/a/b.git").unwrap();
        assert_eq!(r.host, "git.acme.io"); // no userinfo, no port in the host
        assert_eq!(r.url, "https://git.acme.io:8443/a/b.git"); // userinfo gone, port kept
    }

    #[test]
    fn parses_ssh_scheme() {
        let r = parse_remote("ssh://git@codeberg.org/owner/repo.git").unwrap();
        assert_eq!(r.scheme, RemoteScheme::Ssh);
        assert_eq!(r.host, "codeberg.org");
        assert_eq!(r.url, "ssh://codeberg.org/owner/repo.git");
    }

    #[test]
    fn parses_scp_like() {
        let r = parse_remote("git@github.com:owner/repo.git").unwrap();
        assert_eq!(r.scheme, RemoteScheme::Ssh);
        assert_eq!(r.host, "github.com");
        assert_eq!(r.url, "github.com:owner/repo.git");
    }

    #[test]
    fn normalizes_host_case_and_trailing_dot() {
        assert_eq!(parse_remote("https://GitHub.COM/o/r.git").unwrap().host, "github.com");
        assert_eq!(parse_remote("https://example.com./o/r.git").unwrap().host, "example.com");
    }

    #[test]
    fn allows_localhost_and_self_hosted() {
        assert_eq!(parse_remote("https://localhost:3000/o/r.git").unwrap().host, "localhost");
        assert_eq!(parse_remote("https://forge.internal.corp/o/r.git").unwrap().host, "forge.internal.corp");
    }

    #[test]
    fn rejects_disallowed_transports_and_junk() {
        for bad in [
            "git://github.com/o/r.git",
            "file:///etc/passwd",
            "ext::sh -c whoami",
            "http://insecure.example.com/o/r.git",
            "https://",
            "https:///no-host",
            "not a url",
            "https://bad host.com/o/r.git", // whitespace
            "https://-lead.com/o/r.git",
        ] {
            assert!(parse_remote(bad).is_none(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn normalize_host_is_idempotent_and_lowercasing() {
        assert_eq!(normalize_host("EXAMPLE.com."), "example.com");
        assert_eq!(normalize_host(&normalize_host("Foo.Bar")), "foo.bar");
    }
}
