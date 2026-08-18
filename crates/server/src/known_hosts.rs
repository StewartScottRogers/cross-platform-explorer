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

/// Render one [`KnownHost`] back to its `known_hosts` line form (the inverse of the per-entry half of
/// [`parse_known_hosts`]), with no trailing newline.
fn format_entry(entry: &KnownHost) -> String {
    let marker = match entry.marker {
        Some(Marker::Revoked) => "@revoked ",
        Some(Marker::CertAuthority) => "@cert-authority ",
        None => "",
    };
    format!("{marker}{} {} {}", entry.patterns.join(","), entry.key_type, entry.key_b64)
}

/// Persist `entries` to `path` as a `known_hosts`-format file (one line per entry, the same shape
/// [`parse_known_hosts`] reads back), creating parent directories as needed. Writes to a sibling temp file
/// and renames it into place, so a crash mid-write can never leave a half-written store behind (CPE-1512:
/// this is the **app-managed** store — callers must never point it at the user's real `~/.ssh/known_hosts`,
/// which this module only ever reads).
pub fn save_known_hosts(path: &std::path::Path, entries: &[KnownHost]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut out = String::new();
    for entry in entries {
        out.push_str(&format_entry(entry));
        out.push('\n');
    }
    // Same-directory temp file so the rename is same-filesystem (atomic on all three target OSes).
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, out).map_err(|e| format!("{}: {e}", tmp.display()))?;
    // CPE-1710: the app's OWN known_hosts, `default_app_known_hosts_path()` in the app config dir —
    // never `~/.ssh/known_hosts`, which this crate only ever reads. Atomic replace of our own file.
    #[allow(clippy::disallowed_methods)]
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Record a first-contact host key into the **app-managed** `known_hosts` store at `path` (CPE-1512): the
/// TOFU completion. Reads the existing store, and — unless an entry for the exact same
/// `(host, port, key_type, key_b64)` is already present (no duplicate entries on a re-record of the same
/// key) — appends a new plain entry and rewrites the store. Never touches an existing entry for a
/// *different* key on the same host: this function is only ever called on an `Unknown` verdict, and must
/// never be used to silently accept a `Changed` key. A malformed existing store line is skipped (never a
/// panic), matching [`parse_known_hosts`].
pub fn append_host_key(
    path: &std::path::Path,
    host: &str,
    port: u16,
    key_type: &str,
    key_b64: &str,
) -> Result<(), String> {
    let mut entries = load_known_hosts(path);
    let token = host_token(host, port);
    let already_recorded = entries
        .iter()
        .any(|e| e.marker.is_none() && e.key_type == key_type && e.key_b64 == key_b64 && e.patterns.iter().any(|p| p == &token));
    if already_recorded {
        return Ok(());
    }
    entries.push(KnownHost {
        marker: None,
        patterns: vec![token],
        key_type: key_type.to_string(),
        key_b64: key_b64.to_string(),
    });
    save_known_hosts(path, &entries)
}

/// The app-managed `known_hosts` store path (CPE-1512): sits next to `connections.json` in the per-OS user
/// config dir (`%APPDATA%\cross-platform-explorer\known_hosts` on Windows; `$XDG_CONFIG_HOME` or
/// `~/.config/cross-platform-explorer/known_hosts` elsewhere). Deliberately **not** the user's real
/// `~/.ssh/known_hosts` ([`default_known_hosts_path`]) — this module never writes to that file; recording a
/// first-contact key writes only here.
pub fn default_app_known_hosts_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    Some(base.join("cross-platform-explorer").join("known_hosts"))
}

/// Load + merge the app-managed store (`app_path`) with the user's real `~/.ssh/known_hosts` (`ssh_path`)
/// for a verify call (CPE-1512). **The user's `~/.ssh` entries always win**: an app-store entry whose host
/// token + key-type is already covered by an `ssh_path` entry is dropped from the merge — regardless of
/// whether the key material agrees — so a stale or (in a compromise scenario) tampered app-store entry can
/// never override a genuine OpenSSH pin, and a real key rotation the user already accepted via `ssh`/`scp`
/// is not masked by an app-recorded key for the old one. Either path may be missing/unreadable; that side
/// then contributes no entries (never an error).
pub fn load_merged_known_hosts(app_path: &std::path::Path, ssh_path: &std::path::Path) -> Vec<KnownHost> {
    let ssh_entries = load_known_hosts(ssh_path);
    let app_entries = load_known_hosts(app_path);
    let covered_by_ssh = |pattern: &str, key_type: &str| {
        ssh_entries.iter().any(|e| e.key_type == key_type && e.patterns.iter().any(|p| p == pattern))
    };
    let mut merged = ssh_entries.clone();
    for entry in app_entries {
        if entry.patterns.iter().any(|p| covered_by_ssh(p, &entry.key_type)) {
            continue; // the user's ~/.ssh pin for this host+type wins
        }
        merged.push(entry);
    }
    merged
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

    /// A fresh scratch dir for the persistence tests below, unique per test run (PID + a counter so
    /// several tests in the same process don't collide).
    fn scratch_dir(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-kh-{tag}"))
    }

    #[test]
    fn save_known_hosts_round_trips_through_parse() {
        let dir = scratch_dir("save-roundtrip");
        let path = dir.join("known_hosts");
        let entries = vec![
            KnownHost {
                marker: None,
                patterns: vec!["h".into()],
                key_type: "ssh-ed25519".into(),
                key_b64: "AAAAONE".into(),
            },
            KnownHost {
                marker: Some(Marker::Revoked),
                patterns: vec!["evil.example.com".into()],
                key_type: "ssh-ed25519".into(),
                key_b64: "AAAABAD".into(),
            },
        ];
        save_known_hosts(&path, &entries).expect("save");
        let loaded = load_known_hosts(&path);
        assert_eq!(loaded, entries, "round-trips byte-for-byte through save + load");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_host_key_records_a_first_contact_key() {
        let dir = scratch_dir("append-first");
        let path = dir.join("known_hosts");
        assert!(!path.exists(), "store starts absent, like a fresh app install");

        append_host_key(&path, "nas.example.com", 22, "ssh-ed25519", "AAAANEW").expect("record");
        let loaded = load_known_hosts(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].patterns, vec!["nas.example.com"]);
        assert_eq!(loaded[0].key_type, "ssh-ed25519");
        assert_eq!(loaded[0].key_b64, "AAAANEW");
        assert_eq!(loaded[0].marker, None);

        // A second connect presenting the SAME key resolves Trusted against the now-recorded store.
        assert_eq!(
            verify_host_key(&loaded, "nas.example.com", 22, "ssh-ed25519", "AAAANEW"),
            HostKeyVerdict::Trusted,
        );
        // A DIFFERENT key for the same host resolves Changed, never a silent Unknown/Trusted.
        assert_eq!(
            verify_host_key(&loaded, "nas.example.com", 22, "ssh-ed25519", "AAAAIMPOSTOR"),
            HostKeyVerdict::Changed,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_host_key_does_not_duplicate_the_same_key() {
        let dir = scratch_dir("append-dup");
        let path = dir.join("known_hosts");
        append_host_key(&path, "h", 22, "ssh-ed25519", "AAAAKEY").unwrap();
        append_host_key(&path, "h", 22, "ssh-ed25519", "AAAAKEY").unwrap();
        append_host_key(&path, "h", 22, "ssh-ed25519", "AAAAKEY").unwrap();
        assert_eq!(load_known_hosts(&path).len(), 1, "re-recording the same key must not duplicate");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_host_key_distinguishes_port_and_host() {
        let dir = scratch_dir("append-distinct");
        let path = dir.join("known_hosts");
        append_host_key(&path, "h", 22, "ssh-ed25519", "AAAAKEY").unwrap();
        append_host_key(&path, "h", 2222, "ssh-ed25519", "AAAAKEY").unwrap(); // different port, same key
        append_host_key(&path, "other", 22, "ssh-ed25519", "AAAAKEY").unwrap(); // different host
        assert_eq!(load_known_hosts(&path).len(), 3, "distinct host/port pairs are separate entries");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_host_key_skips_a_malformed_existing_line_without_panicking() {
        let dir = scratch_dir("append-malformed");
        let path = dir.join("known_hosts");
        std::fs::write(&path, "this-line-is-garbage\nh ssh-rsa AAAAOLD\n").unwrap();
        append_host_key(&path, "new-host", 22, "ssh-ed25519", "AAAANEW").expect("must not panic");
        let loaded = load_known_hosts(&path);
        // The malformed line never round-trips (parse_known_hosts already drops it); the well-formed
        // pre-existing entry survives alongside the newly appended one.
        assert_eq!(loaded.len(), 2, "got {loaded:?}");
        assert!(loaded.iter().any(|e| e.patterns == vec!["h".to_string()] && e.key_b64 == "AAAAOLD"));
        assert!(loaded.iter().any(|e| e.patterns == vec!["new-host".to_string()] && e.key_b64 == "AAAANEW"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_app_known_hosts_path_is_distinct_from_the_real_ssh_one() {
        if let (Some(app), Some(ssh)) = (default_app_known_hosts_path(), default_known_hosts_path()) {
            assert_ne!(app, ssh, "the app store must never alias the user's real ~/.ssh/known_hosts");
            assert!(app.ends_with("known_hosts"), "got {app:?}");
            assert!(
                app.to_string_lossy().contains("cross-platform-explorer"),
                "lives under the app's own config dir, got {app:?}"
            );
        }
    }

    #[test]
    fn merged_known_hosts_prefers_ssh_pin_on_conflict() {
        let dir = scratch_dir("merge-conflict");
        let ssh_path = dir.join("ssh_known_hosts");
        let app_path = dir.join("app_known_hosts");
        // The user genuinely pinned this host via OpenSSH with KEY-REAL.
        std::fs::write(&ssh_path, "pinned.example.com ssh-ed25519 KEYREAL\n").unwrap();
        // The app store (e.g. stale, or in a compromise scenario tampered with) has a DIFFERENT key for
        // the same host.
        append_host_key(&app_path, "pinned.example.com", 22, "ssh-ed25519", "KEYOTHER").unwrap();

        let merged = load_merged_known_hosts(&app_path, &ssh_path);
        // The user's pin wins: the real key is Trusted...
        assert_eq!(
            verify_host_key(&merged, "pinned.example.com", 22, "ssh-ed25519", "KEYREAL"),
            HostKeyVerdict::Trusted,
        );
        // ...and the app-store's conflicting key is refused as Changed, NOT silently Trusted.
        assert_eq!(
            verify_host_key(&merged, "pinned.example.com", 22, "ssh-ed25519", "KEYOTHER"),
            HostKeyVerdict::Changed,
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merged_known_hosts_includes_app_only_entries() {
        let dir = scratch_dir("merge-app-only");
        let ssh_path = dir.join("ssh_known_hosts"); // never created — no ~/.ssh entry for this host
        let app_path = dir.join("app_known_hosts");
        append_host_key(&app_path, "app-only.example.com", 22, "ssh-ed25519", "AAAAAPP").unwrap();

        let merged = load_merged_known_hosts(&app_path, &ssh_path);
        assert_eq!(
            verify_host_key(&merged, "app-only.example.com", 22, "ssh-ed25519", "AAAAAPP"),
            HostKeyVerdict::Trusted,
            "an app-recorded, first-contact key is honoured when ~/.ssh has no opinion",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merged_known_hosts_tolerates_missing_files_on_both_sides() {
        let dir = scratch_dir("merge-missing");
        // Neither file exists.
        let merged = load_merged_known_hosts(&dir.join("app"), &dir.join("ssh"));
        assert!(merged.is_empty());
    }
}
