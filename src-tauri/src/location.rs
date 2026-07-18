//! Location model + URI parser (CPE-680, epic CPE-616): classify a location string as **local** (a plain
//! filesystem path — including Windows drive `C:\...` and UNC `\\host\share`) or a **remote scheme**
//! (`sftp`/`ssh`/`smb`/`webdav`/`davs`/`s3`), broken into `{scheme, user, host, port, path}`. This is the
//! enabling model so later code can route a location to a filesystem provider by scheme instead of
//! assuming local paths. Pure + dependency-free; unit-tested. No network, no auth.

#![allow(dead_code)] // consumed once providers (CPE-681+) are wired; kept always-compiled + tested now.

/// The kind of backend a location targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Local,
    Sftp,
    Smb,
    Webdav,
    S3,
}

/// A parsed location. For `Local`, only `path` is meaningful (the whole input); remote schemes fill in
/// the authority parts where present.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Location {
    pub scheme: Scheme,
    pub user: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub path: String,
}

impl Location {
    /// A plain local path (no recognised remote scheme).
    pub fn local(path: &str) -> Self {
        Location { scheme: Scheme::Local, user: None, host: None, port: None, path: path.to_string() }
    }

    pub fn is_local(&self) -> bool {
        self.scheme == Scheme::Local
    }
}

/// Map a URI scheme word to a `Scheme`, or `None` if it isn't a recognised remote scheme. `ssh` is an
/// alias for `sftp`; `davs` (secure WebDAV) maps to `Webdav`.
fn remote_scheme(word: &str) -> Option<Scheme> {
    match word.to_ascii_lowercase().as_str() {
        "sftp" | "ssh" => Some(Scheme::Sftp),
        "smb" => Some(Scheme::Smb),
        "webdav" | "davs" | "dav" => Some(Scheme::Webdav),
        "s3" => Some(Scheme::S3),
        _ => None,
    }
}

/// Parse a location string. Anything without a recognised `scheme://` prefix — a POSIX path, a Windows
/// drive path (`C:\…`), or a UNC path (`\\host\share`) — is `Local` with the whole string as `path`.
pub fn parse(input: &str) -> Location {
    // A remote location must contain "://". `C:\` and `\\host` never do, so drive/UNC paths stay Local.
    let Some(sep) = input.find("://") else {
        return Location::local(input);
    };
    let word = &input[..sep];
    let Some(scheme) = remote_scheme(word) else {
        // A "://" with an unknown scheme is treated as a local path rather than guessed at.
        return Location::local(input);
    };

    let rest = &input[sep + 3..]; // after "://"
    // The authority runs up to the first '/', the rest (with the leading '/') is the path.
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };

    // authority = [user@]host[:port]
    let (user, hostport) = match authority.rsplit_once('@') {
        Some((u, hp)) if !u.is_empty() => (Some(u.to_string()), hp),
        _ => (None, authority),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()),
        None => (hostport, None),
    };

    Location {
        scheme,
        user,
        host: if host.is_empty() { None } else { Some(host.to_string()) },
        port,
        path: if path.is_empty() { "/".to_string() } else { path.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paths_are_local() {
        for p in [r"C:\Users\a\file.txt", "/home/x/y", r"\\server\share\dir", "relative/path", "file.txt"] {
            let loc = parse(p);
            assert_eq!(loc.scheme, Scheme::Local, "{p} should be Local");
            assert!(loc.is_local());
            assert_eq!(loc.path, p, "local path is preserved verbatim");
            assert_eq!(loc.host, None);
        }
    }

    #[test]
    fn unknown_scheme_is_treated_as_local() {
        let loc = parse("mailto://x"); // not a filesystem scheme
        assert_eq!(loc.scheme, Scheme::Local);
        assert_eq!(loc.path, "mailto://x");
    }

    #[test]
    fn sftp_with_full_authority_parses() {
        let loc = parse("sftp://alice@host.example.com:2222/var/www");
        assert_eq!(loc.scheme, Scheme::Sftp);
        assert_eq!(loc.user.as_deref(), Some("alice"));
        assert_eq!(loc.host.as_deref(), Some("host.example.com"));
        assert_eq!(loc.port, Some(2222));
        assert_eq!(loc.path, "/var/www");
    }

    #[test]
    fn ssh_is_an_alias_for_sftp_and_defaults() {
        let loc = parse("ssh://host"); // no user, no port, no path
        assert_eq!(loc.scheme, Scheme::Sftp);
        assert_eq!(loc.user, None);
        assert_eq!(loc.host.as_deref(), Some("host"));
        assert_eq!(loc.port, None);
        assert_eq!(loc.path, "/", "missing path defaults to root");
    }

    #[test]
    fn other_remote_schemes_map_correctly() {
        assert_eq!(parse("smb://nas/share").scheme, Scheme::Smb);
        assert_eq!(parse("webdav://host/dav").scheme, Scheme::Webdav);
        assert_eq!(parse("davs://host/dav").scheme, Scheme::Webdav);
        assert_eq!(parse("s3://bucket/key").scheme, Scheme::S3);
        // s3://bucket/key → host=bucket, path=/key
        let s3 = parse("s3://bucket/key");
        assert_eq!(s3.host.as_deref(), Some("bucket"));
        assert_eq!(s3.path, "/key");
    }

    #[test]
    fn a_bad_port_is_dropped_not_fatal() {
        let loc = parse("sftp://host:notaport/x");
        assert_eq!(loc.host.as_deref(), Some("host"));
        assert_eq!(loc.port, None);
        assert_eq!(loc.path, "/x");
    }
}
