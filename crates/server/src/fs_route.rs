//! Scheme-based routing seam (CPE-685, epic CPE-616): the single dispatch point that decides which
//! [`FileSystemProvider`](crate::provider::FileSystemProvider) backs a given location string, by
//! classifying its scheme ([`location`](crate::location), CPE-680). Local paths run the local backend;
//! recognised remote schemes (`sftp`/`smb`/`webdav`/`s3`) are **not yet wired into the command layer**, so
//! they are rejected here with one clear, consistent message rather than being handed to the OS as a bogus
//! local path (which would surface as a cryptic "No such file" error).
//!
//! This is the seam the epic establishes: command entry points call [`require_local`] as a guard so local
//! behaviour is byte-for-byte unchanged and every command rejects a remote URI identically, and
//! [`provider_for`] is where the already-headless SFTP/WebDAV providers slot in when remote operations are
//! turned on. Deliberately thin and pure — no I/O, unit-tested.

use crate::location::{self, Scheme};
use crate::provider::{FileSystemProvider, LocalProvider};

/// Where a location string routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A plain local filesystem path — served by [`LocalProvider`].
    Local,
    /// A recognised remote scheme, not yet connected into the command layer.
    Remote(Scheme),
}

/// Classify a location string to its [`Route`] by scheme. A plain local path (incl. Windows `C:\…` / UNC
/// `\\host\share`) is [`Route::Local`]; a recognised remote scheme is [`Route::Remote`].
pub fn route(uri: &str) -> Route {
    let loc = location::parse(uri);
    if loc.is_local() {
        Route::Local
    } else {
        Route::Remote(loc.scheme)
    }
}

/// A human label for a scheme, used in user-facing messages.
pub fn scheme_label(scheme: Scheme) -> &'static str {
    match scheme {
        Scheme::Local => "local",
        Scheme::Sftp => "SFTP",
        Scheme::Smb => "SMB",
        Scheme::Webdav => "WebDAV",
        Scheme::S3 => "S3",
    }
}

/// The one message shown when a command is asked to act on a remote location that isn't wired up yet — so
/// every FS command rejects remote paths with identical wording.
pub fn not_connected_message(scheme: Scheme) -> String {
    format!(
        "{} locations aren't connected yet — remote file operations are not available.",
        scheme_label(scheme)
    )
}

/// Guard for a **local-only** command: `Ok(())` for a local path, a clean, consistent error for any remote
/// scheme. Command entry points call this at the top so local behaviour is unchanged and remote schemes
/// fail the same way everywhere instead of hitting the OS as a bogus path.
pub fn require_local(uri: &str) -> Result<(), String> {
    match route(uri) {
        Route::Local => Ok(()),
        Route::Remote(scheme) => Err(not_connected_message(scheme)),
    }
}

/// Select the [`FileSystemProvider`] backing a location, chosen by scheme: [`LocalProvider`] for a local
/// path today; a recognised remote scheme returns the not-connected error until its provider is wired in
/// (the SFTP/WebDAV provider crates already exist headlessly). This is the extension point remote backends
/// plug into.
pub fn provider_for(uri: &str) -> Result<Box<dyn FileSystemProvider>, String> {
    match route(uri) {
        Route::Local => Ok(Box::new(LocalProvider)),
        Route::Remote(scheme) => Err(not_connected_message(scheme)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_local_paths_route_local() {
        for p in [
            "/home/u/file.txt",
            "relative/path",
            "file.txt",
            r"C:\Users\me\Documents",
            r"\\nas\share\dir",
            "",
        ] {
            assert_eq!(route(p), Route::Local, "{p:?} should route Local");
            assert!(require_local(p).is_ok(), "{p:?} should pass the local guard");
            assert!(provider_for(p).is_ok(), "{p:?} should get the local provider");
        }
    }

    #[test]
    fn remote_schemes_route_remote() {
        let cases = [
            ("sftp://user@host/dir", Scheme::Sftp),
            ("ssh://host/dir", Scheme::Sftp),
            ("smb://nas/share", Scheme::Smb),
            ("webdav://host/dav", Scheme::Webdav),
            ("davs://host/dav", Scheme::Webdav),
            ("s3://bucket/key", Scheme::S3),
        ];
        for (uri, scheme) in cases {
            assert_eq!(route(uri), Route::Remote(scheme), "{uri} should route Remote({scheme:?})");
            let err = require_local(uri).expect_err("remote should be rejected by the local guard");
            assert!(err.contains(scheme_label(scheme)), "message names the scheme: {err}");
            assert!(provider_for(uri).is_err(), "no provider for a not-connected remote scheme yet");
        }
    }

    #[test]
    fn the_local_provider_actually_works_through_the_seam() {
        // provider_for hands back a usable LocalProvider (round-trips over a temp dir).
        let dir = std::env::temp_dir().join(format!("cpe_fsroute_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.to_string_lossy().to_string();
        let mut p = provider_for(&root).expect("local provider");
        p.mkdir(&root).unwrap();
        let f = dir.join("hello.txt");
        let fp = f.to_string_lossy().to_string();
        p.write(&fp, b"hi").unwrap();
        assert_eq!(p.read(&fp).unwrap(), b"hi");
        let listed = p.list(&root).unwrap();
        assert!(listed.iter().any(|e| e.name == "hello.txt"), "written file appears in listing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn not_connected_message_is_consistent_and_scheme_specific() {
        assert!(not_connected_message(Scheme::Sftp).contains("SFTP"));
        assert!(not_connected_message(Scheme::S3).contains("S3"));
        assert_ne!(not_connected_message(Scheme::Sftp), not_connected_message(Scheme::Smb));
    }
}
