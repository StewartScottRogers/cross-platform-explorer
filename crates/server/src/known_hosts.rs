//! SSH `known_hosts` parsing + host-key verification (CPE-682, epic CPE-616): the security core of the
//! future SFTP provider, deliberately decoupled from any network/ssh crate so it is pure and
//! unit-testable. Given the lines of a `known_hosts` file and the host key a server presents, it decides
//! whether to trust it — the **trust-on-first-use** (TOFU) + "a *changed* key is refused loudly" model
//! OpenSSH uses, which is what actually defends an SFTP session against a man-in-the-middle.
//!
//! Scope of this slice: plain host patterns and the `[host]:port` token OpenSSH writes for non-default
//! ports, with comma-separated pattern lists. **Not yet:** hashed hostnames (`|1|salt|hash`), wildcard
//! (`*.example.com`) / negated (`!host`) patterns, and `@revoked`/`@cert-authority` markers — a
//! non-matching or marker line simply doesn't establish trust, so the safe default is `Unknown` (prompt),
//! never a false `Trusted`. Hashed-hostname support can layer on hmac-sha1 as a follow-up.

#![allow(dead_code)] // consumed once the SFTP provider is wired (CPE-682 network half); compiled + tested now.

/// One parsed non-comment `known_hosts` entry: `patterns keytype base64key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHost {
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
}

/// Parse the lines of a `known_hosts` file into entries, skipping blanks, `#` comments, `@`-marker lines
/// (revoked / cert-authority — not honoured in this slice), and malformed lines (fewer than 3 fields).
pub fn parse_known_hosts(contents: &str) -> Vec<KnownHost> {
    let mut out = Vec::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(patterns), Some(key_type), Some(key_b64)) = (fields.next(), fields.next(), fields.next())
        else {
            continue; // a well-formed entry has at least: patterns keytype key
        };
        out.push(KnownHost {
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
/// *different* type never triggers `Changed` (OpenSSH would just add the new type).
pub fn verify_host_key(
    known: &[KnownHost],
    host: &str,
    port: u16,
    key_type: &str,
    key_b64: &str,
) -> HostKeyVerdict {
    let token = host_token(host, port);
    let mut saw_host_and_type = false;
    for entry in known {
        if entry.key_type != key_type || !entry.patterns.iter().any(|p| p == &token) {
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
        // 3 real entries; the comment, blank, @revoked marker, and 2-field line are all skipped.
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].patterns, vec!["host.example.com"]);
        assert_eq!(hosts[0].key_type, "ssh-ed25519");
        assert_eq!(hosts[1].patterns, vec!["[host.example.com]:2222"]);
        // A comma list splits into multiple patterns.
        assert_eq!(hosts[2].patterns, vec!["nas", "10.0.0.5"]);
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
}
