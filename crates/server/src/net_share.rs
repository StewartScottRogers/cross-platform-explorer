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
}
