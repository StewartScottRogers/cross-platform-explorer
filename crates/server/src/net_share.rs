//! Network-share address parser (pure model) — CPE-1024, epic CPE-716. Turns a user-typed share
//! address (`smb://…`, `nfs://…`, `ftp://…`, `sftp://…`, or a Windows UNC path) into a normalized
//! [`NetworkShare`], so the future "connect to server" dialog and the mount glue share one tested
//! understanding of an address. No network or mount I/O here — hand-rolled parsing only, no new
//! dependencies (lean-core: do not pull a URL crate for this).

/// The protocol a parsed share address uses. A Windows UNC path (`\\host\share`) is normalized to
/// [`ShareProtocol::Smb`] — UNC *is* SMB.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ShareProtocol {
    Smb,
    Nfs,
    Ftp,
    Sftp,
}

impl ShareProtocol {
    fn as_str(&self) -> &'static str {
        match self {
            ShareProtocol::Smb => "smb",
            ShareProtocol::Nfs => "nfs",
            ShareProtocol::Ftp => "ftp",
            ShareProtocol::Sftp => "sftp",
        }
    }
}

/// A parsed, normalized network-share address.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NetworkShare {
    pub protocol: ShareProtocol,
    pub host: String,
    /// The first path segment after the host (empty when the address has no path at all, e.g.
    /// `ftp://host`).
    pub share: String,
    /// Everything after `share`, with a leading `/` — empty string when there is none.
    pub path: String,
}

impl NetworkShare {
    /// Render back to the canonical `proto://host/share/path` form. Round-trips any address
    /// [`parse_share`] produced (smb, nfs, ftp, sftp all share the same URL shape).
    pub fn to_url(&self) -> String {
        let mut url = format!("{}://{}", self.protocol.as_str(), self.host);
        if !self.share.is_empty() {
            url.push('/');
            url.push_str(&self.share);
        }
        url.push_str(&self.path);
        url
    }
}

/// Split `rest` (everything after `scheme://` or a UNC prefix, forward-slash-normalized) into
/// `(host, share, path)`. `host` must be non-empty.
fn split_host_share_path(rest: &str) -> Result<(String, String, String), String> {
    let mut top = rest.splitn(2, '/');
    let host = top.next().unwrap_or("").to_string();
    if host.is_empty() {
        return Err("network share address is missing a host".to_string());
    }
    let remainder = top.next().unwrap_or("");
    if remainder.is_empty() {
        return Ok((host, String::new(), String::new()));
    }
    let mut sub = remainder.splitn(2, '/');
    let share = sub.next().unwrap_or("").to_string();
    let path = match sub.next() {
        Some(rest) if !rest.is_empty() => format!("/{rest}"),
        _ => String::new(),
    };
    Ok((host, share, path))
}

/// Parse a user-typed network-share address into a [`NetworkShare`].
///
/// Accepts `smb://`, `nfs://`, `ftp://`, `sftp://` URLs and Windows UNC paths
/// (`\\host\share\sub`, back- or forward-slashed) — UNC is normalized to `Smb`. A host is always
/// required; empty input, a scheme with no host, and an unrecognized scheme all return `Err`.
pub fn parse_share(input: &str) -> Result<NetworkShare, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("network share address is empty".to_string());
    }

    if let Some(idx) = input.find("://") {
        let scheme = input[..idx].to_ascii_lowercase();
        let rest = &input[idx + 3..];
        let protocol = match scheme.as_str() {
            "smb" => ShareProtocol::Smb,
            "nfs" => ShareProtocol::Nfs,
            "ftp" => ShareProtocol::Ftp,
            "sftp" => ShareProtocol::Sftp,
            other => return Err(format!("unknown network share scheme: {other}")),
        };
        let (host, share, path) = split_host_share_path(rest)?;
        return Ok(NetworkShare { protocol, host, share, path });
    }

    if let Some(rest) = input.strip_prefix("\\\\").or_else(|| input.strip_prefix("//")) {
        let normalized = rest.replace('\\', "/");
        let (host, share, path) = split_host_share_path(&normalized)?;
        return Ok(NetworkShare { protocol: ShareProtocol::Smb, host, share, path });
    }

    Err(format!("unrecognized network share address: {input}"))
}

// ── Home "Shared" tab enumeration (CPE-1163) ────────────────────────────────────────────────────
//
// The pure, platform-agnostic half of the Shared tab: turn the OS's own view of mounted network
// storage (parsed from `net use` on Windows, `/proc/mounts` on Linux, `mount` on macOS) plus the
// user's manually-added locations into one deduplicated list of [`NetShare`] rows. All the parsing +
// merging lives here so it is unit-testable with no I/O; the app adapter (`lib.rs`) only runs the
// (time-bounded) OS commands and feeds their stdout in. A dead/offline server never reaches this
// code — the adapter bounds the subprocess and hands us an empty string on timeout.

/// One row on the Home "Shared" tab (CPE-1163): a mapped/mounted network drive the OS already knows
/// about, or a user-added network location. `path` is the navigable handle (a drive root like `Z:\`
/// for a mapped drive, a mountpoint for a Unix mount, or the raw `\\server\share` / `smb://…` address
/// for a user-added location); `kind` tells the UI which right-click actions apply.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NetShare {
    /// Display label — the UNC/source for an enumerated share, or the share/host for a user location.
    pub name: String,
    /// Navigable path: a drive root (`Z:\`), a mountpoint (`/mnt/share`), or the user's raw address.
    pub path: String,
    /// `"mapped"` (OS-enumerated Windows mapped drive), `"mount"` (Unix SMB/NFS mount), or `"user"`
    /// (a location the user typed + we persist). The UI adapts the menu: mapped → "Disconnect",
    /// user → "Remove".
    pub kind: String,
}

/// A drive-letter token like `Z:` (exactly one ASCII letter + a colon).
fn is_drive_letter(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Parse the output of Windows `net use` into mapped-drive rows.
///
/// `net use` (no args) prints the local redirector's connection table — it does **not** contact the
/// servers, so it returns promptly even when a mapped server is offline. The output is columnar and
/// **localized**, but a mapped-drive row always carries a drive-letter token (`Z:`) and a UNC token
/// (`\\host\share`) whose shapes are language-independent, so we scan tokens for those two rather
/// than depend on fixed column widths or the English status words. A connection with no drive letter
/// (an IPC/`$` share) is skipped — it isn't something the Shared tab can open.
pub fn parse_net_use(output: &str) -> Vec<NetShare> {
    let mut out = Vec::new();
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let letter = tokens.iter().copied().find(|t| is_drive_letter(t));
        let unc = tokens.iter().copied().find(|t| t.starts_with("\\\\"));
        if let (Some(letter), Some(unc)) = (letter, unc) {
            out.push(NetShare {
                name: format!("{unc} ({letter})"),
                path: format!("{letter}\\"),
                kind: "mapped".to_string(),
            });
        }
    }
    out
}

/// Filesystem types on Linux/macOS that represent a network mount worth surfacing.
fn is_network_fstype(fstype: &str) -> bool {
    matches!(
        fstype,
        "cifs" | "smbfs" | "smb3" | "nfs" | "nfs4" | "fuse.smbfs" | "fuse.sshfs" | "webdav" | "afpfs"
    )
}

/// The trailing component of a `//host/share`, `host:/export`, or `/mnt/point` source/path — used as
/// a friendly display name. Falls back to the whole string when it has no separator.
fn share_leaf(s: &str) -> String {
    let trimmed = s.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .find(|seg| !seg.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// Decode the octal `\NNN` escapes the kernel writes into `/proc/mounts` (e.g. `\040` for a space).
fn decode_proc_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let bytes = field.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let oct = &field[i + 1..i + 4];
            if let Ok(code) = u8::from_str_radix(oct, 8) {
                out.push(code as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Parse `/proc/mounts` (Linux) into network-mount rows. Each line is
/// `source mountpoint fstype options dump pass`; we keep only network filesystem types.
pub fn parse_proc_mounts(mounts: &str) -> Vec<NetShare> {
    let mut out = Vec::new();
    for line in mounts.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 || !is_network_fstype(fields[2]) {
            continue;
        }
        let source = decode_proc_field(fields[0]);
        let mountpoint = decode_proc_field(fields[1]);
        out.push(NetShare {
            name: format!("{} ({})", share_leaf(&source), mountpoint),
            path: mountpoint,
            kind: "mount".to_string(),
        });
    }
    out
}

/// Parse the output of macOS/BSD `mount` into network-mount rows. Lines read
/// `//user@host/share on /Volumes/share (smbfs, ...)`; we split on ` on ` and read the fs type from
/// the parenthesised options, keeping only network filesystems.
pub fn parse_macos_mount(output: &str) -> Vec<NetShare> {
    let mut out = Vec::new();
    for line in output.lines() {
        let Some((source, rest)) = line.split_once(" on ") else { continue };
        let Some(paren) = rest.find(" (") else { continue };
        let mountpoint = rest[..paren].trim();
        let opts = rest[paren + 2..].trim_end_matches(')');
        let fstype = opts.split(',').next().unwrap_or("").trim();
        if !is_network_fstype(fstype) {
            continue;
        }
        out.push(NetShare {
            name: format!("{} ({mountpoint})", share_leaf(source.trim())),
            path: mountpoint.to_string(),
            kind: "mount".to_string(),
        });
    }
    out
}

/// Validate a user-typed network location and build its [`NetShare`]. Returns `None` for anything
/// [`parse_share`] rejects (empty, hostless, unknown scheme, junk). The row's `path` keeps the user's
/// original address verbatim (a UNC is directly navigable on Windows); the display name is the share
/// name, or the host when the address names no share.
pub fn user_share(input: &str) -> Option<NetShare> {
    let parsed = parse_share(input).ok()?;
    let name = if parsed.share.is_empty() { parsed.host } else { parsed.share };
    Some(NetShare { name, path: input.trim().to_string(), kind: "user".to_string() })
}

/// Normalize a path to a dedup key: trimmed, trailing slashes dropped, lowercased (share paths are
/// case-insensitive on Windows and treating them so cross-platform is harmless here).
fn dedup_key(path: &str) -> String {
    path.trim().trim_end_matches(['/', '\\']).to_ascii_lowercase()
}

/// Combine the OS-enumerated shares with the user-added locations into one list, enumerated first,
/// dropping any user location whose path duplicates an already-listed share (so a user-added address
/// that the OS also has mapped doesn't appear twice). Invalid user entries are silently skipped.
pub fn combine_shares(enumerated: Vec<NetShare>, user_added: &[String]) -> Vec<NetShare> {
    let mut seen: std::collections::HashSet<String> =
        enumerated.iter().map(|s| dedup_key(&s.path)).collect();
    let mut out = enumerated;
    for input in user_added {
        if let Some(share) = user_share(input) {
            if seen.insert(dedup_key(&share.path)) {
                out.push(share);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smb_url() {
        let s = parse_share("smb://myserver/share/sub/dir").unwrap();
        assert_eq!(s.protocol, ShareProtocol::Smb);
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "share");
        assert_eq!(s.path, "/sub/dir");
    }

    #[test]
    fn parses_nfs_url() {
        let s = parse_share("nfs://myserver/export/path").unwrap();
        assert_eq!(s.protocol, ShareProtocol::Nfs);
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "export");
        assert_eq!(s.path, "/path");
    }

    #[test]
    fn parses_ftp_and_sftp_urls() {
        let ftp = parse_share("ftp://myserver/path").unwrap();
        assert_eq!(ftp.protocol, ShareProtocol::Ftp);
        assert_eq!(ftp.host, "myserver");
        assert_eq!(ftp.share, "path");
        assert_eq!(ftp.path, "");

        let sftp = parse_share("sftp://myserver/path").unwrap();
        assert_eq!(sftp.protocol, ShareProtocol::Sftp);
        assert_eq!(sftp.host, "myserver");
    }

    #[test]
    fn parses_host_only_with_no_share() {
        let s = parse_share("ftp://myserver").unwrap();
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "");
        assert_eq!(s.path, "");
    }

    #[test]
    fn parses_unc_backslash_path() {
        let s = parse_share(r"\\myserver\share\sub").unwrap();
        assert_eq!(s.protocol, ShareProtocol::Smb);
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "share");
        assert_eq!(s.path, "/sub");
    }

    #[test]
    fn parses_unc_forward_slash_path() {
        let s = parse_share("//myserver/share/sub").unwrap();
        assert_eq!(s.protocol, ShareProtocol::Smb);
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "share");
        assert_eq!(s.path, "/sub");
    }

    #[test]
    fn parses_unc_mixed_slashes() {
        let s = parse_share(r"\\myserver/share\sub/dir").unwrap();
        assert_eq!(s.protocol, ShareProtocol::Smb);
        assert_eq!(s.host, "myserver");
        assert_eq!(s.share, "share");
        assert_eq!(s.path, "/sub/dir");
    }

    #[test]
    fn round_trips_smb_via_to_url() {
        let s = parse_share("smb://myserver/share/sub/dir").unwrap();
        assert_eq!(s.to_url(), "smb://myserver/share/sub/dir");
    }

    #[test]
    fn round_trips_nfs_via_to_url() {
        let s = parse_share("nfs://myserver/export/path").unwrap();
        assert_eq!(s.to_url(), "nfs://myserver/export/path");
    }

    #[test]
    fn round_trips_unc_as_smb_url() {
        let s = parse_share(r"\\myserver\share\sub").unwrap();
        assert_eq!(s.to_url(), "smb://myserver/share/sub");
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(parse_share("").is_err());
        assert!(parse_share("   ").is_err());
    }

    #[test]
    fn scheme_only_with_no_host_is_an_error() {
        assert!(parse_share("smb://").is_err());
        assert!(parse_share(r"\\").is_err());
    }

    #[test]
    fn unknown_scheme_is_an_error() {
        let err = parse_share("s3://bucket/key").unwrap_err();
        assert!(err.contains("s3"), "error should mention the bad scheme: {err}");
    }

    #[test]
    fn junk_input_with_no_scheme_or_unc_is_an_error() {
        assert!(parse_share("just some text").is_err());
        assert!(parse_share("myserver/share").is_err());
    }

    // ── Shared-tab enumeration (CPE-1163) ────────────────────────────────────────────────────────

    #[test]
    fn parse_net_use_extracts_mapped_drives() {
        // A realistic English `net use` dump: a header, a separator, two mapped drives (OK +
        // Disconnected), an IPC connection with no drive letter, and the trailer.
        let out = "New connections will be remembered.\n\
            \n\
            Status       Local     Remote                    Network\n\
            -------------------------------------------------------------------------------\n\
            OK           Z:        \\\\server\\share           Microsoft Windows Network\n\
            Disconnected Y:        \\\\other\\pub              Microsoft Windows Network\n\
                         \\\\srv\\IPC$                         Microsoft Windows Network\n\
            The command completed successfully.\n";
        let shares = parse_net_use(out);
        assert_eq!(shares.len(), 2, "two lettered drives; the IPC row is skipped");
        assert_eq!(shares[0].path, "Z:\\");
        assert_eq!(shares[0].kind, "mapped");
        assert!(shares[0].name.contains("\\\\server\\share"));
        assert!(shares[0].name.contains("Z:"));
        assert_eq!(shares[1].path, "Y:\\");
    }

    #[test]
    fn parse_net_use_is_robust_to_a_localized_or_empty_dump() {
        // Localized status words don't matter — the drive-letter + UNC tokens are language-neutral.
        let localized = "Statut       Local     Distant\n\
            Déconnecté   W:        \\\\host\\docs\n";
        let shares = parse_net_use(localized);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].path, "W:\\");
        // A timed-out / empty subprocess yields no rows, never a panic.
        assert!(parse_net_use("").is_empty());
        assert!(parse_net_use("There are no entries in the list.\n").is_empty());
    }

    #[test]
    fn parse_proc_mounts_keeps_only_network_filesystems() {
        let mounts = "sysfs /sys sysfs rw 0 0\n\
            //fileserver/media /mnt/media cifs rw,vers=3.0 0 0\n\
            /dev/sda1 / ext4 rw 0 0\n\
            nas:/export/backups /mnt/backups nfs4 rw 0 0\n\
            tmpfs /run tmpfs rw 0 0\n";
        let shares = parse_proc_mounts(mounts);
        assert_eq!(shares.len(), 2, "cifs + nfs4 only; local fs skipped");
        assert_eq!(shares[0].path, "/mnt/media");
        assert_eq!(shares[0].kind, "mount");
        assert!(shares[0].name.starts_with("media"));
        assert_eq!(shares[1].path, "/mnt/backups");
    }

    #[test]
    fn parse_proc_mounts_decodes_octal_escapes_in_paths() {
        let mounts = "//host/My\\040Share /mnt/My\\040Share cifs rw 0 0\n";
        let shares = parse_proc_mounts(mounts);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].path, "/mnt/My Share");
    }

    #[test]
    fn parse_macos_mount_keeps_only_network_filesystems() {
        let out = "/dev/disk1s1 on / (apfs, local, journaled)\n\
            //guest@nas/media on /Volumes/media (smbfs, nodev, nosuid, mounted by me)\n\
            nas:/export on /Volumes/export (nfs)\n\
            map auto_home on /System/Volumes/Data/home (autofs, nosuid)\n";
        let shares = parse_macos_mount(out);
        assert_eq!(shares.len(), 2, "smbfs + nfs only");
        assert_eq!(shares[0].path, "/Volumes/media");
        assert_eq!(shares[0].kind, "mount");
        assert_eq!(shares[1].path, "/Volumes/export");
    }

    #[test]
    fn user_share_accepts_valid_addresses_and_rejects_junk() {
        let unc = user_share(r"\\server\projects").unwrap();
        assert_eq!(unc.kind, "user");
        assert_eq!(unc.name, "projects");
        assert_eq!(unc.path, r"\\server\projects", "keeps the raw navigable address");

        let smb = user_share("smb://nas/media").unwrap();
        assert_eq!(smb.name, "media");

        // Host-only address → the host is the label.
        assert_eq!(user_share("ftp://myhost").unwrap().name, "myhost");

        assert!(user_share("").is_none());
        assert!(user_share("not a share").is_none());
        assert!(user_share("s3://bucket/key").is_none());
    }

    #[test]
    fn combine_shares_appends_user_entries_and_dedupes() {
        let enumerated = vec![NetShare {
            name: "\\\\server\\share (Z:)".to_string(),
            path: "Z:\\".to_string(),
            kind: "mapped".to_string(),
        }];
        let user = vec![
            r"\\nas\media".to_string(),   // new → kept
            "z:\\".to_string(),           // dup of the mapped drive (case/slash-insensitive) → dropped
            "garbage".to_string(),        // invalid → skipped
        ];
        let combined = combine_shares(enumerated, &user);
        assert_eq!(combined.len(), 2);
        assert_eq!(combined[0].path, "Z:\\", "enumerated first");
        assert_eq!(combined[1].kind, "user");
        assert_eq!(combined[1].path, r"\\nas\media");
    }

    #[test]
    fn combine_shares_with_no_input_is_empty_not_a_hang() {
        assert!(combine_shares(Vec::new(), &[]).is_empty());
    }
}
