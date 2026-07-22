//! SSH `known_hosts` parsing + host-key verification (CPE-682, epic CPE-616): the security core of the
//! future SFTP provider, deliberately decoupled from any network/ssh crate so it is pure and
//! unit-testable. Given the lines of a `known_hosts` file and the host key a server presents, it decides
//! whether to trust it — the **trust-on-first-use** (TOFU) + "a *changed* key is refused loudly" model
//! OpenSSH uses, which is what actually defends an SFTP session against a man-in-the-middle.
//!
//! Scope of this slice: plain host patterns and the `[host]:port` token OpenSSH writes for non-default
//! ports, with comma-separated pattern lists, plus the `@revoked` / `@cert-authority` line markers. **Not
//! yet:** hashed hostnames (`|1|salt|hash`) and wildcard (`*.example.com`) / negated (`!host`) patterns —
//! a non-matching line simply doesn't establish trust, so the safe default is `Unknown` (prompt), never a
//! false `Trusted`. Hashed-hostname support can layer on hmac-sha1 as a follow-up.

#![allow(dead_code)] // consumed once the SFTP provider is wired (CPE-682 network half); compiled + tested now.

/// An optional leading marker on a `known_hosts` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// `@revoked` — this key is explicitly revoked; it must be refused even if it would otherwise match.
    Revoked,
    /// `@cert-authority` — a CA key used to verify host *certificates*, not a host key itself. Excluded
    /// from normal host-key matching in this slice (certificate validation is out of scope).
    CertAuthority,
}

/// One parsed non-comment `known_hosts` entry: `[@marker] patterns keytype base64key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHost {
    /// A leading `@revoked` / `@cert-authority` marker, if present.
    pub marker: Option<Marker>,
    /// The host patterns this line applies to (a comma-separated list in the file), e.g. `["host", "1.2.3.4"]`
    /// or `["[host]:2222"]`.
    pub patterns: Vec<String>,
    /// The key algorithm, e.g. `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`.
    pub key_type: String,
    /// The base64-encoded public key blob (compared verbatim — byte-identical means the same key).
    pub key_b64: String,
}

/// The verdict for a host key a server presents, against what we already know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyVerdict {
    /// A stored entry for this host + key-type matches the presented key — proceed.
    Trusted,
    /// No stored entry for this host + key-type — first contact. The UI should prompt to trust and, on
    /// acceptance, append the key (TOFU). Safe default whenever we can't establish a match.
    Unknown,
    /// A stored entry for this host + key-type exists but the key **differs** — a possible MITM (or a
    /// legitimately rekeyed server). Refuse loudly; never silently proceed.
    Changed,
    /// The presented key matches a `@revoked` entry for this host — explicitly refused.
    Revoked,
}

/// Parse the lines of a `known_hosts` file into entries, skipping blanks, `#` comments, and malformed
/// lines. A leading `@revoked` / `@cert-authority` marker is honoured; an unknown `@marker` line is
/// skipped (rather than mis-parsed).
pub fn parse_known_hosts(contents: &str) -> Vec<KnownHost> {
    let mut out = Vec::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        // An optional leading @marker shifts the remaining fields by one.
        let mut first = fields.next();
        let marker = match first {
            Some("@revoked") => {
                first = fields.next();
                Some(Marker::Revoked)
            }
            Some("@cert-authority") => {
                first = fields.next();
                Some(Marker::CertAuthority)
            }
            Some(tok) if tok.starts_with('@') => continue, // unknown marker — skip rather than mis-parse
            _ => None,
        };
        let (Some(patterns), Some(key_type), Some(key_b64)) = (first, fields.next(), fields.next()) else {
            continue; // a well-formed entry has at least: [@marker] patterns keytype key
        };
        out.push(KnownHost {
            marker,
            patterns: patterns.split(',').filter(|p| !p.is_empty()).map(str::to_string).collect(),
            key_type: key_type.to_string(),
            key_b64: key_b64.to_string(),
        });
    }
    out
}

/// The host token OpenSSH records in `known_hosts`: the bare `host` on the default port 22, else the
/// bracketed `[host]:port` form.
pub fn host_token(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Decide whether to trust the `(key_type, key_b64)` a server at `host:port` presents, given the parsed
/// `known_hosts`. See [`HostKeyVerdict`]. Matching is per host-token **and** key-type: a stored key of a
/// *different* type never triggers `Changed` (OpenSSH would just add the new type). A `@revoked` match
/// wins over everything; `@cert-authority` entries are not host keys and are ignored here.
pub fn verify_host_key(
    known: &[KnownHost],
    host: &str,
    port: u16,
    key_type: &str,
    key_b64: &str,
) -> HostKeyVerdict {
    let token = host_token(host, port);
    let matches_host_type =
        |e: &KnownHost| e.key_type == key_type && e.patterns.iter().any(|p| p == &token);

    // Revocation wins: a presented key listed under `@revoked` for this host is refused outright,
    // even if a separate (non-revoked) entry would otherwise trust it.
    if known
        .iter()
        .any(|e| e.marker == Some(Marker::Revoked) && matches_host_type(e) && e.key_b64 == key_b64)
    {
        return HostKeyVerdict::Revoked;
    }

    let mut saw_host_and_type = false;
    for entry in known {
        // Only normal host-key entries establish trust; skip markers (revoked already handled, CA is not
        // a host key).
        if entry.marker.is_some() || !matches_host_type(entry) {
            continue;
        }
        saw_host_and_type = true;
        if entry.key_b64 == key_b64 {
            return HostKeyVerdict::Trusted;
        }
    }
    if saw_host_and_type {
        HostKeyVerdict::Changed
    } else {
        HostKeyVerdict::Unknown
    }
}

/// The default `~/.ssh/known_hosts` path for the current user, from `$HOME` (or `%USERPROFILE%` on
/// Windows). `None` if neither is set.
pub fn default_known_hosts_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::Path::new(&home).join(".ssh").join("known_hosts"))
}

/// Load + parse a `known_hosts` file. A missing or unreadable file yields an **empty** list (first-use
/// TOFU), never an error — the safe default for host-key verification.
pub fn load_known_hosts(path: &std::path::Path) -> Vec<KnownHost> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_known_hosts(&contents),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment line
host.example.com ssh-ed25519 AAAAKEYONE

[host.example.com]:2222 ssh-ed25519 AAAAKEYTWO
nas,10.0.0.5 ssh-rsa AAAARSAKEY
@revoked evil.example.com ssh-ed25519 AAAABADKEY
malformed-line-only-two-fields ssh-rsa
";

    #[test]
    fn parses_entries_and_skips_noise() {
        let hosts = parse_known_hosts(SAMPLE);
        // 4 entries (incl. the @revoked one); the comment, blank, and 2-field lines are skipped.
        assert_eq!(hosts.len(), 4);
        assert_eq!(hosts[0].patterns, vec!["host.example.com"]);
        assert_eq!(hosts[0].key_type, "ssh-ed25519");
        assert_eq!(hosts[0].marker, None);
        assert_eq!(hosts[1].patterns, vec!["[host.example.com]:2222"]);
        // A comma list splits into multiple patterns.
        assert_eq!(hosts[2].patterns, vec!["nas", "10.0.0.5"]);
        // The @revoked marker is parsed, with its fields shifted past the marker.
        assert_eq!(hosts[3].marker, Some(Marker::Revoked));
        assert_eq!(hosts[3].patterns, vec!["evil.example.com"]);
        assert_eq!(hosts[3].key_b64, "AAAABADKEY");
    }

    #[test]
    fn a_revoked_key_is_refused_over_everything() {
        // A key both trusted AND revoked for the same host must be refused (revocation wins).
        let k = parse_known_hosts(
            "h ssh-ed25519 AAAAGOOD\n@revoked h ssh-ed25519 AAAABAD\n",
        );
        assert_eq!(verify_host_key(&k, "h", 22, "ssh-ed25519", "AAAABAD"), HostKeyVerdict::Revoked);
        // A different, non-revoked key for the same host is still trusted.
        assert_eq!(verify_host_key(&k, "h", 22, "ssh-ed25519", "AAAAGOOD"), HostKeyVerdict::Trusted);
        // The revoked key from SAMPLE is refused.
        let s = parse_known_hosts(SAMPLE);
        assert_eq!(
            verify_host_key(&s, "evil.example.com", 22, "ssh-ed25519", "AAAABADKEY"),
            HostKeyVerdict::Revoked,
        );
    }

    #[test]
    fn a_cert_authority_line_is_not_a_host_key() {
        // A @cert-authority entry is parsed but never establishes host-key trust (it's a CA, not a host
        // key) — a host presenting that key material is still Unknown, not Trusted.
        let k = parse_known_hosts("@cert-authority ca.example.com ssh-ed25519 AAAACAKEY\n");
        assert_eq!(k[0].marker, Some(Marker::CertAuthority));
        assert_eq!(
            verify_host_key(&k, "ca.example.com", 22, "ssh-ed25519", "AAAACAKEY"),
            HostKeyVerdict::Unknown,
        );
    }

    #[test]
    fn an_unknown_marker_line_is_skipped() {
        let k = parse_known_hosts("@bogus h ssh-ed25519 AAAAKEY\nh2 ssh-rsa AAAARSA\n");
        assert_eq!(k.len(), 1, "the @bogus line is skipped, not mis-parsed");
        assert_eq!(k[0].patterns, vec!["h2"]);
    }

    #[test]
    fn host_token_brackets_only_nondefault_ports() {
        assert_eq!(host_token("h", 22), "h");
        assert_eq!(host_token("h", 2222), "[h]:2222");
    }

    #[test]
    fn a_matching_key_is_trusted() {
        let k = parse_known_hosts(SAMPLE);
        assert_eq!(
            verify_host_key(&k, "host.example.com", 22, "ssh-ed25519", "AAAAKEYONE"),
            HostKeyVerdict::Trusted,
        );
        // The port-2222 entry matches only via the bracketed token.
        assert_eq!(
            verify_host_key(&k, "host.example.com", 2222, "ssh-ed25519", "AAAAKEYTWO"),
            HostKeyVerdict::Trusted,
        );
        // An IP listed in a comma list is matched too.
        assert_eq!(
            verify_host_key(&k, "10.0.0.5", 22, "ssh-rsa", "AAAARSAKEY"),
            HostKeyVerdict::Trusted,
        );
    }

    #[test]
    fn an_unknown_host_or_type_is_unknown_not_changed() {
        let k = parse_known_hosts(SAMPLE);
        // Never seen this host.
        assert_eq!(
            verify_host_key(&k, "new.example.com", 22, "ssh-ed25519", "AAAAWHATEVER"),
            HostKeyVerdict::Unknown,
        );
        // Known host, but a key-type we have no entry for → Unknown (OpenSSH would just add it), NOT Changed.
        assert_eq!(
            verify_host_key(&k, "host.example.com", 22, "ssh-rsa", "AAAASOMERSA"),
            HostKeyVerdict::Unknown,
        );
        // Right host, right port bracket needed: default-port lookup must not match the [host]:2222 entry.
        assert_eq!(
            verify_host_key(&k, "host.example.com", 22, "ssh-ed25519", "AAAAKEYTWO"),
            HostKeyVerdict::Changed,
            "same host+type on port 22 with a different key is a changed key",
        );
    }

    #[test]
    fn a_changed_key_is_flagged_loudly() {
        let k = parse_known_hosts(SAMPLE);
        // Same host + same key-type, DIFFERENT key material → possible MITM.
        assert_eq!(
            verify_host_key(&k, "host.example.com", 22, "ssh-ed25519", "AAAAIMPOSTOR"),
            HostKeyVerdict::Changed,
        );
    }

    #[test]
    fn load_known_hosts_reads_and_parses_a_file() {
        let dir = std::env::temp_dir().join(format!("cpe-kh-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("known_hosts");
        std::fs::write(&path, SAMPLE).unwrap();
        let loaded = load_known_hosts(&path);
        assert_eq!(loaded.len(), 4, "same entries as parse_known_hosts(SAMPLE)");
        assert_eq!(loaded[3].marker, Some(Marker::Revoked));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_known_hosts_of_a_missing_file_is_empty_not_an_error() {
        // A missing file ⇒ empty (first-use TOFU), never a panic/error.
        assert!(load_known_hosts(std::path::Path::new("/no/such/cpe/known_hosts")).is_empty());
    }

    #[test]
    fn default_known_hosts_path_ends_in_ssh_known_hosts() {
        // CI always sets HOME (Unix) or USERPROFILE (Windows); if neither, the fn returns None (also fine).
        if let Some(p) = default_known_hosts_path() {
            assert!(p.ends_with(std::path::Path::new(".ssh").join("known_hosts")), "got {p:?}");
        }
    }
}
