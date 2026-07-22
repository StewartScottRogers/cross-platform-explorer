//! WebDAV filesystem provider (epic CPE-616): a remote backend over HTTP/WebDAV, implementing
//! [`cpe_server::provider::FileSystemProvider`] so the explorer can browse a WebDAV share (Nextcloud,
//! ownCloud, many NAS) by the same interface it uses for the local disk and SFTP.
//!
//! Unlike the SFTP provider this is **synchronous** — `ureq` is a blocking HTTP client, so no internal
//! async runtime is needed. TLS is pure-Rust (`rustls` + `ring`), so it builds with no C tooling on every
//! CI OS. The 6 provider ops map to WebDAV methods: PROPFIND (list/stat), GET (read), PUT (write), MKCOL
//! (mkdir), DELETE (delete). Testing runs against an in-process WebDAV server (see the tests) — no Docker.

use base64::Engine as _;
use cpe_server::provider::{FileSystemProvider, ProviderEntry};
use std::io::Read as _;

/// How to reach a WebDAV share.
#[derive(Debug, Clone)]
pub struct WebdavConfig {
    /// Base URL of the share, e.g. `https://host/remote.php/dav/files/me` (no trailing slash needed).
    pub base_url: String,
    /// Optional HTTP Basic credentials.
    pub user: Option<String>,
    pub password: Option<String>,
}

impl WebdavConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), user: None, password: None }
    }
    pub fn with_basic_auth(mut self, user: impl Into<String>, password: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self.password = Some(password.into());
        self
    }
}

/// A WebDAV share presented as a synchronous [`FileSystemProvider`].
pub struct WebdavProvider {
    agent: ureq::Agent,
    base_url: String,
    auth_header: Option<String>,
}

/// The PROPFIND body requesting the properties we need (resource type + content length + name).
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop>
  <d:resourcetype/><d:getcontentlength/><d:displayname/>
</d:prop></d:propfind>"#;

impl WebdavProvider {
    /// Build a provider for `config`. Does not perform a request (WebDAV is stateless HTTP); the first
    /// `list`/`read`/… issues a request and surfaces auth/connection errors then.
    pub fn connect(config: &WebdavConfig) -> Self {
        let auth_header = config.user.as_deref().map(|u| {
            let pass = config.password.as_deref().unwrap_or("");
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{pass}"));
            format!("Basic {token}")
        });
        WebdavProvider { agent: ureq::agent(), base_url: config.base_url.clone(), auth_header }
    }

    /// The absolute URL for a provider path (`/`-rooted).
    fn url_for(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path.trim_start_matches('/'))
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let mut req = self.agent.request(method, &self.url_for(path));
        if let Some(auth) = &self.auth_header {
            req = req.set("Authorization", auth);
        }
        req
    }
}

/// Map a `ureq` failure (transport error or non-2xx status) into a legible message.
fn http_err(path: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("{path}: HTTP {code}"),
        ureq::Error::Transport(t) => format!("{path}: {t}"),
    }
}

impl FileSystemProvider for WebdavProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        let body = self
            .request("PROPFIND", path)
            .set("Depth", "1")
            .set("Content-Type", "application/xml")
            .send_string(PROPFIND_BODY)
            .map_err(|e| http_err(path, e))?
            .into_string()
            .map_err(|e| format!("{path}: {e}"))?;
        // Depth:1 includes the collection itself first; skip the entry whose href is the requested dir.
        parse_multistatus(&body, Some(path))
    }

    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        let body = self
            .request("PROPFIND", path)
            .set("Depth", "0")
            .set("Content-Type", "application/xml")
            .send_string(PROPFIND_BODY)
            .map_err(|e| http_err(path, e))?
            .into_string()
            .map_err(|e| format!("{path}: {e}"))?;
        let mut entries = parse_multistatus(&body, None)?;
        let mut entry = entries.pop().ok_or_else(|| format!("{path}: not found"))?;
        // The name of a stat is the requested path's last segment (the href of `/` is empty).
        entry.name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_string();
        Ok(entry)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let resp = self.request("GET", path).call().map_err(|e| http_err(path, e))?;
        let mut buf = Vec::new();
        resp.into_reader().read_to_end(&mut buf).map_err(|e| format!("{path}: {e}"))?;
        Ok(buf)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        self.request("PUT", path).send_bytes(data).map_err(|e| http_err(path, e))?;
        Ok(())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.request("MKCOL", path).call().map_err(|e| http_err(path, e))?;
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        self.request("DELETE", path).call().map_err(|e| http_err(path, e))?;
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        // WebDAV MOVE: the target is the absolute URL of `to` in the Destination header.
        let dest = self.url_for(to);
        self.request("MOVE", from)
            .set("Destination", &dest)
            .set("Overwrite", "T")
            .call()
            .map_err(|e| http_err(from, e))?;
        Ok(())
    }
}

/// Parse a PROPFIND `multistatus` XML body into provider entries. If `skip_path` is set, the entry whose
/// href equals that path (the collection itself, in a Depth:1 listing) is omitted. Matches element
/// **local** names, so DAV namespace prefixes (`d:` / `D:` / none) don't matter.
fn parse_multistatus(xml: &str, skip_path: Option<&str>) -> Result<Vec<ProviderEntry>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("webdav: bad PROPFIND XML: {e}"))?;
    let skip = skip_path.map(normalize_href);
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|n| n.tag_name().name() == "response") {
        let href = resp
            .descendants()
            .find(|n| n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .unwrap_or("");
        let norm = normalize_href(&percent_decode(href));
        if skip.as_deref() == Some(norm.as_str()) {
            continue;
        }
        let name = norm.rsplit('/').next().unwrap_or("").to_string();
        if name.is_empty() {
            continue; // the collection root with no skip target — nothing to name
        }
        let is_dir = resp.descendants().any(|n| n.tag_name().name() == "collection");
        let size = resp
            .descendants()
            .find(|n| n.tag_name().name() == "getcontentlength")
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<u64>().ok())
            .unwrap_or(0);
        out.push(ProviderEntry { name, is_dir, size: if is_dir { 0 } else { size } });
    }
    Ok(out)
}

/// Normalise an href/path for comparison: strip a trailing slash (a collection's href ends in `/`).
fn normalize_href(s: &str) -> String {
    let t = s.trim_end_matches('/');
    if t.is_empty() { "/".to_string() } else { t.to_string() }
}

/// Minimal percent-decoding for href path segments (`%20` → space, etc.). Invalid escapes pass through.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    const FILE_BODY: &[u8] = b"hello webdav"; // 12 bytes

    /// A `<d:response>` for one resource.
    fn dav_response(href: &str, is_dir: bool, size: u64) -> String {
        let rt = if is_dir { "<d:collection/>" } else { "" };
        let len = if is_dir {
            String::new()
        } else {
            format!("<d:getcontentlength>{size}</d:getcontentlength>")
        };
        format!(
            r#"<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:resourcetype>{rt}</d:resourcetype>{len}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#
        )
    }

    /// A collection's href ends with a trailing slash.
    fn href_for(url: &str, is_dir: bool) -> String {
        if is_dir { format!("{}/", url.trim_end_matches('/')) } else { url.to_string() }
    }

    /// Serve one request against the temp-dir `root`, mapping WebDAV methods to `std::fs`.
    fn handle(mut req: tiny_http::Request, root: &Path) {
        let method = req.method().to_string().to_uppercase();
        let url = req.url().to_string();
        let depth = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Depth"))
            .map(|h| h.value.as_str().to_string())
            .unwrap_or_else(|| "1".to_string());
        let mut body = Vec::new();
        let _ = req.as_reader().read_to_end(&mut body);
        let real = root.join(url.trim_start_matches('/'));

        match method.as_str() {
            "PROPFIND" => match std::fs::metadata(&real) {
                Ok(meta) => {
                    let mut responses = dav_response(&href_for(&url, meta.is_dir()), meta.is_dir(), meta.len());
                    if meta.is_dir() && depth != "0" {
                        if let Ok(rd) = std::fs::read_dir(&real) {
                            for e in rd.flatten() {
                                if let Ok(cm) = e.metadata() {
                                    let child = format!(
                                        "{}/{}",
                                        url.trim_end_matches('/'),
                                        e.file_name().to_string_lossy()
                                    );
                                    responses += &dav_response(&href_for(&child, cm.is_dir()), cm.is_dir(), cm.len());
                                }
                            }
                        }
                    }
                    let xml = format!(
                        r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">{responses}</d:multistatus>"#
                    );
                    let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                    let _ = req.respond(tiny_http::Response::from_string(xml).with_status_code(207).with_header(ct));
                }
                Err(_) => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            },
            "GET" => match std::fs::read(&real) {
                Ok(data) => {
                    let _ = req.respond(tiny_http::Response::from_data(data));
                }
                Err(_) => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            },
            "PUT" => {
                if let Some(p) = real.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                let code = if std::fs::write(&real, &body).is_ok() { 201 } else { 500 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            "MKCOL" => {
                let code = if std::fs::create_dir_all(&real).is_ok() { 201 } else { 500 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            "DELETE" => {
                let r = if real.is_dir() {
                    std::fs::remove_dir_all(&real)
                } else {
                    std::fs::remove_file(&real)
                };
                let _ = req.respond(tiny_http::Response::empty(if r.is_ok() { 204 } else { 404 }));
            }
            "MOVE" => {
                // The Destination header is an absolute URL; map its path under `root`.
                let dest_path = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Destination"))
                    .map(|h| h.value.as_str().to_string())
                    .and_then(|u| u.rsplit_once("://").map(|(_, rest)| rest.split_once('/').map(|(_, p)| format!("/{p}")).unwrap_or_default()))
                    .unwrap_or_default();
                let dest_real = root.join(dest_path.trim_start_matches('/'));
                let code = if std::fs::rename(&real, &dest_real).is_ok() { 201 } else { 404 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            _ => {
                let _ = req.respond(tiny_http::Response::empty(405));
            }
        }
    }

    /// Spawn the in-process WebDAV server on an ephemeral port; returns its base URL. Seeds a temp root:
    /// `readme.txt` ("hello webdav") + `sub/nested.txt`.
    fn spawn_webdav_server() -> String {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root: PathBuf = std::env::temp_dir().join(format!("cpe-webdav-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("readme.txt"), FILE_BODY).unwrap();
        std::fs::write(root.join("sub").join("nested.txt"), b"deep").unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &root);
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn lists_stats_and_reads_over_webdav() {
        let base = spawn_webdav_server();
        let provider = WebdavProvider::connect(&WebdavConfig::new(&base));

        let mut names: Vec<_> =
            provider.list("/").expect("list").into_iter().map(|e| (e.name, e.is_dir)).collect();
        names.sort();
        assert_eq!(names, vec![("readme.txt".to_string(), false), ("sub".to_string(), true)]);

        assert_eq!(provider.read("/readme.txt").unwrap(), FILE_BODY);
        assert_eq!(provider.stat("/readme.txt").unwrap().size, FILE_BODY.len() as u64);
        assert!(!provider.stat("/readme.txt").unwrap().is_dir);
        assert!(provider.stat("/sub").unwrap().is_dir);
    }

    #[test]
    fn writes_mkdirs_and_deletes_round_trip() {
        let base = spawn_webdav_server();
        let mut provider = WebdavProvider::connect(&WebdavConfig::new(&base));

        provider.write("/notes.txt", b"remote write").expect("write");
        assert_eq!(provider.read("/notes.txt").unwrap(), b"remote write");

        provider.mkdir("/newdir").expect("mkdir");
        assert!(provider.stat("/newdir").unwrap().is_dir);

        provider.delete("/notes.txt").expect("delete");
        assert!(provider.read("/notes.txt").is_err(), "deleted file should 404");
    }

    #[test]
    fn rename_moves_a_file_over_webdav() {
        let base = spawn_webdav_server();
        let mut provider = WebdavProvider::connect(&WebdavConfig::new(&base));
        provider.rename("/readme.txt", "/renamed.txt").expect("MOVE");
        assert_eq!(provider.read("/renamed.txt").unwrap(), FILE_BODY);
        assert!(provider.read("/readme.txt").is_err(), "old path should be gone");
    }

    #[test]
    fn generic_transfer_walks_downloads_and_uploads_over_webdav() {
        // The provider-agnostic cpe_server::transfer works over the WebDAV transport too.
        let base = spawn_webdav_server();
        let mut provider = WebdavProvider::connect(&WebdavConfig::new(&base));
        let cancel = AtomicBool::new(false);

        // walk the seeded tree.
        let mut paths = Vec::new();
        let n = cpe_server::transfer::walk(&provider, "/", &cancel, |e| paths.push((e.path, e.is_dir))).unwrap();
        paths.sort();
        assert_eq!(n, 3, "readme.txt + sub + sub/nested.txt; got {paths:?}");
        assert!(paths.contains(&("/readme.txt".to_string(), false)));
        assert!(paths.contains(&("/sub/nested.txt".to_string(), false)));

        // download the tree locally.
        let out = std::env::temp_dir().join(format!("cpe-webdav-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let files = cpe_server::transfer::download_tree(&provider, "/", &out, &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("readme.txt")).unwrap(), FILE_BODY);

        // upload it back under a new remote root, then read one file over WebDAV to confirm it landed.
        cpe_server::transfer::upload_tree(&mut provider, &out, "/copied", &cancel).unwrap();
        assert_eq!(provider.read("/copied/readme.txt").unwrap(), FILE_BODY);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_missing_path_is_an_error() {
        let base = spawn_webdav_server();
        let provider = WebdavProvider::connect(&WebdavConfig::new(&base));
        assert!(provider.read("/nope.txt").unwrap_err().contains("404"));
        assert!(provider.stat("/nope").is_err());
    }
}
