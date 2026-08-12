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
        // Pin an explicit no-redirect policy (CPE-1461 watch-item): the default agent follows up to 5
        // redirects, so a future change (or a hostile server today) could bounce a request via a `3xx`
        // toward `file://` or an attacker-controlled host — an SSRF/exfiltration foothold. Refusing to
        // auto-follow surfaces the `3xx` as an error instead; a WebDAV share never needs redirects.
        let agent = ureq::AgentBuilder::new().redirects(0).build();
        WebdavProvider { agent, base_url: config.base_url.clone(), auth_header }
    }

    /// The absolute URL for a provider path (`/`-rooted). Each segment is percent-encoded (CPE-1659
    /// found this the hard way against a real Apache `mod_dav` server): `ureq`'s request URL is parsed
    /// like any other URL, so a raw `#` in a path silently starts a fragment — everything after it is
    /// dropped from what's actually sent on the wire — and other reserved bytes aren't safe to send raw
    /// either. The in-process fake server this crate's own tests use reads the raw request-line text
    /// with no URL parsing at all, so it never exercised this; a real server does.
    fn url_for(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), percent_encode_path(path.trim_start_matches('/')))
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
        let resp = self.request("DELETE", path).call().map_err(|e| http_err(path, e))?;
        // CPE-1659 found this against a real Apache server: `.call()` only turns a >=400 status into
        // an `Err` (see `ureq::Request::call`), so a 3xx response comes back as `Ok`. Apache's
        // `mod_dir` `DirectorySlash` behaviour redirects (301) a request for a collection that's
        // missing its trailing slash INSTEAD of performing it — deleting `/some-dir` (no slash) would
        // otherwise silently "succeed" here while nothing was actually deleted. This agent disables
        // auto-follow-redirect on purpose (CPE-1461: blindly following a server-supplied `Location`
        // could bounce toward `file://` or an attacker-controlled host), so rather than trust the
        // Location header, retry ONCE against the well-known WebDAV collection-URL convention (a
        // trailing slash, RFC 4918 §8.3) — never a second, server-chosen destination.
        //
        // CPE-1659 negative-control proof (required acceptance criterion): this exact fix was reverted
        // on this branch and pushed as a deliberate break — the real-server rig's
        // `webdav_conformance_against_real_apache_moddav` test went RED ("the now-empty directory must
        // be gone from disk after delete") while the in-process `cargo test -p cpe-webdav` suite (which
        // never deletes a directory, only a file) stayed GREEN, proving the rig can fail and that a
        // same-author fake server cannot catch this class of bug. See the CPE-1659 Work Log for both
        // run URLs.
        if (300..400).contains(&resp.status()) && !path.ends_with('/') {
            let with_slash = format!("{}/", path.trim_end_matches('/'));
            let retry = self.request("DELETE", &with_slash).call().map_err(|e| http_err(path, e))?;
            // CPE-1673 follow-up: the retry is subject to the exact same `.call()`-only-errors-on->=400
            // behaviour as the first attempt above, so a server that ALSO redirects the trailing-slash
            // form would otherwise fall through to `Ok(())` here having deleted nothing — the identical
            // failure mode the retry exists to close, one hop further along. Never trust a second 3xx as
            // success; surface it as an error instead of silently no-op'ing.
            if (300..400).contains(&retry.status()) {
                return Err(format!(
                    "{path}: DELETE retried against {with_slash} also returned HTTP {} (not deleted)",
                    retry.status()
                ));
            }
        }
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

/// The deepest element nesting `parse_multistatus` will hand to `roxmltree` before refusing the response.
/// `roxmltree::Document::parse` — like most XML parsers — recurses per nesting level, so a PROPFIND
/// response with deep enough nesting is enough to blow a thread stack and crash the whole process with an
/// **uncatchable** stack overflow. The exact crash depth is **both stack-size- and build-profile-dependent**,
/// not a universal constant: confirmed locally at ~300-500 levels on a 2MB stack in a RELEASE build, and as
/// low as ~150 levels on a 256KB stack in a release build — but a DEBUG build overflows far shallower than
/// its release counterpart at the same stack size (debug recursion frames carry far less inlining/optimization,
/// so each nesting level costs substantially more stack), so don't assume the release-build numbers above
/// transfer to a `cargo build` without `--release`. 64 is sized with a wide margin under the smallest
/// *release-build* threshold observed, and the app only ever runs this guard on the shipped release build's
/// multi-MB Tokio `spawn_blocking` threads (not the default ~1MB/256KB-class thread some other contexts use),
/// which gives extra headroom on top of that margin. A real WebDAV `multistatus` body is only ever a handful
/// of levels deep (`multistatus > response > propstat > prop > ...`, ~5 levels), so even a small cap costs
/// nothing for legitimate responses while leaving a large margin below any observed crash depth (CPE-1398).
///
/// This guard's pre-scan uses the `xmlparser` crate's lexer to approximate what `roxmltree` (which vendors
/// its own, independently-forked lexer) will actually see; the two are separately-maintained forks of the
/// same lineage and `xmlparser` itself is dormant, so they can drift. Re-verify the pre-scan's tag/quote/
/// comment/CDATA/PI handling against `roxmltree`'s actual grammar on any `roxmltree` **major** version bump,
/// in case its parsing rules (or its vendored lexer) diverge from what `xmlparser` still assumes here.
const MAX_XML_NESTING_DEPTH: usize = 64;

/// Cheap, non-recursive guard against maliciously (or accidentally) deep XML nesting, run before the
/// document is handed to `roxmltree` (see [`MAX_XML_NESTING_DEPTH`]).
///
/// This used to be a hand-rolled byte scan for `<`/`>` tag boundaries, but that was quote-unaware: an
/// unescaped `>` inside a quoted attribute value (e.g. `<a b="/>">`, legal XML) let a real child-bearing
/// open element hide behind what the scan mistook for a self-closing `/>`, silently under-counting depth
/// and defeating the guard entirely (CPE-1398 follow-up). Rather than patch that one hole and risk another
/// like it, this walks the real tokens from [`xmlparser::Tokenizer`] — the same non-recursive streaming
/// lexer `roxmltree` itself used to be built on — which correctly handles quotes, comments, CDATA, and
/// processing instructions by construction, closing the whole class of scan-evasion bugs at once. Being a
/// streaming iterator (not a recursive-descent parser), walking it can't itself stack-overflow no matter
/// how deep or malformed the input is.
fn xml_nesting_too_deep(xml: &str, max_depth: usize) -> bool {
    let mut depth: usize = 0;
    for token in xmlparser::Tokenizer::from(xml) {
        let token = match token {
            Ok(t) => t,
            Err(_) => break, // malformed XML — let roxmltree::Document::parse report the real error
        };
        if let xmlparser::Token::ElementEnd { end, .. } = token {
            match end {
                // `>` closing a start tag: the element is now open, awaiting children/its close tag.
                xmlparser::ElementEnd::Open => {
                    depth += 1;
                    if depth > max_depth {
                        return true;
                    }
                }
                // `</name>`: the element that was opened is now closed.
                xmlparser::ElementEnd::Close(..) => depth = depth.saturating_sub(1),
                // `/>`: self-closing — never opens a level.
                xmlparser::ElementEnd::Empty => {}
            }
        }
    }
    false
}

/// Parse a PROPFIND `multistatus` XML body into provider entries. If `skip_path` is set, the entry whose
/// href equals that path (the collection itself, in a Depth:1 listing) is omitted. Matches element
/// **local** names, so DAV namespace prefixes (`d:` / `D:` / none) don't matter.
fn parse_multistatus(xml: &str, skip_path: Option<&str>) -> Result<Vec<ProviderEntry>, String> {
    if xml_nesting_too_deep(xml, MAX_XML_NESTING_DEPTH) {
        return Err("webdav: PROPFIND response XML nesting too deep".to_string());
    }
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
        // Source-side path-traversal defense (CPE-1461): the name is derived from the server's `<d:href>`
        // (after percent-decoding), so a hostile href like `/%2e%2e` or `/C:\...\evil` decodes to a name
        // that is `..` / carries a separator or drive prefix. Treat the name as a single opaque segment
        // and skip the whole entry if it isn't one, before it can reach the local-write sink.
        if !cpe_server::transfer::is_safe_name(&name) {
            continue;
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

/// Percent-encode a `/`-rooted path for an OUTGOING request URL, preserving `/` as the segment
/// separator — the mirror image of [`percent_decode`] above (which reverses this on the way IN, for
/// `<d:href>` values a server sends back). Every byte outside the URL-safe unreserved set
/// (`ALPHA / DIGIT / "-" / "." / "_" / "~"`, plus `/` itself) is escaped, so a name containing `#`,
/// `%`, a space, or non-ASCII/emoji bytes reaches the server as the SAME literal name instead of being
/// misparsed as a URL fragment/reserved character by the HTTP client library.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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

    // -----------------------------------------------------------------------------------------
    // CPE-1673 item 1: `percent_encode_path` had zero in-process coverage — it was pinned only by the
    // Linux-Docker-only real-server rig (the fake server above reads the raw request-line text with no
    // URL parsing at all, so it can never exercise this). This is a pure function, so a five-line unit
    // test runs on all three OS legs for free and guards the exact regression already shipped once: a
    // raw `#` silently starting a URL fragment and truncating everything after it.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn percent_encode_path_escapes_reserved_and_non_ascii_bytes_but_preserves_slashes() {
        // The `#` truncation bug this fix closed (CPE-1659): a literal `#` must never reach the URL
        // parser unescaped.
        assert_eq!(percent_encode_path("weird#name.txt"), "weird%23name.txt");
        // `%` itself must be escaped, or a name containing a literal percent would be misread as the
        // start of an escape sequence.
        assert_eq!(percent_encode_path("100%done.txt"), "100%25done.txt");
        assert_eq!(percent_encode_path("has space.txt"), "has%20space.txt");
        // Non-ASCII (e.g. "café") — each UTF-8 byte outside the unreserved set is escaped individually.
        assert_eq!(percent_encode_path("caf\u{e9}.txt"), "caf%C3%A9.txt");
        // Emoji — a 4-byte UTF-8 sequence, escaped byte-by-byte.
        assert_eq!(percent_encode_path("\u{1F600}.txt"), "%F0%9F%98%80.txt");
        // `/` stays the segment separator, and the unreserved set passes through unescaped.
        assert_eq!(percent_encode_path("sub/dir-name_1.2~3"), "sub/dir-name_1.2~3");
    }

    // -----------------------------------------------------------------------------------------
    // CPE-1673 item 2: the collection-DELETE retry (RFC 4918 trailing-slash convention) must not trust
    // a *second* 3xx as success — `ureq::Request::call()` only turns a >=400 status into an `Err`, so a
    // server that redirects the trailing-slash form too would otherwise silently fall through to
    // `Ok(())` having deleted nothing, exactly the failure mode the retry exists to close.
    // -----------------------------------------------------------------------------------------

    /// A dedicated fake server that redirects EVERY DELETE (with or without a trailing slash) with a
    /// 301 — simulating a server where the RFC 4918 trailing-slash retry also gets redirected, so the
    /// directory is never actually deleted on either attempt.
    fn spawn_always_redirecting_delete_server() -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                if req.method().to_string().eq_ignore_ascii_case("DELETE") {
                    let loc =
                        tiny_http::Header::from_bytes(&b"Location"[..], &b"http://example.invalid/dir/"[..])
                            .unwrap();
                    let _ = req.respond(tiny_http::Response::empty(301).with_header(loc));
                } else {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn delete_retry_that_also_redirects_is_reported_as_an_error_not_ok() {
        let base = spawn_always_redirecting_delete_server();
        let mut provider = WebdavProvider::connect(&WebdavConfig::new(&base));
        let err = provider
            .delete("/dir")
            .expect_err("a retry that ALSO comes back 3xx must not silently report Ok(()) — nothing was deleted");
        assert!(err.contains("dir"), "error should reference the path being deleted; got: {err}");
    }

    // -----------------------------------------------------------------------------------------
    // Adversarial panic-safety battery for `parse_multistatus` (CPE-1398).
    //
    // `parse_multistatus` hand-parses PROPFIND-response XML that comes straight off the wire from a
    // NETWORK-CONTROLLED WebDAV server — potentially malicious or just buggy. This table-driven battery
    // (mirroring the pattern in `crates/server/tests/parser_panic_safety.rs`) feeds it a wide range of
    // hostile input and asserts it never panics — always returns `Ok` or `Err`, never unwinds. It runs as
    // a `#[cfg(test)] mod` (not a `tests/` integration file) because `parse_multistatus` is private to
    // this crate, and that's the entrypoint the ticket asks to exercise directly (no need to relax its
    // visibility just for a test).
    // -----------------------------------------------------------------------------------------

    use std::panic::{self, AssertUnwindSafe};

    /// A tiny seeded linear-congruential generator so "seeded pseudo-random" bytes are deterministic
    /// across runs/machines — mirrors `crates/server/tests/common/mod.rs`'s `lcg_bytes` (no new
    /// dependency; not cryptographic, just reproducible).
    fn lcg_bytes(seed: u64, len: usize) -> Vec<u8> {
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) as u8
            })
            .collect()
    }

    /// Wrap `inner` in a well-formed `<d:multistatus>` envelope with the XML declaration this crate's
    /// real PROPFIND body uses, so cases that aren't testing the envelope itself don't also trip on it.
    fn multistatus_wrap(inner: &str) -> String {
        format!(r#"<?xml version="1.0" encoding="utf-8"?><d:multistatus xmlns:d="DAV:">{inner}</d:multistatus>"#)
    }

    /// The adversarial battery: `(case name, xml text)`. Covers CPE-1398's hostile-input classes: empty
    /// and garbage bytes (including non-UTF8 byte sequences, lossily decoded to `String` since
    /// `parse_multistatus` takes `&str` and Rust strings are always valid UTF-8 — the lossy conversion is
    /// itself the adversarial input here), truncated mid-tag/mid-attribute, deeply-nested elements (stack
    /// safety, a few thousand deep), huge/negative/overflowing `getcontentlength`, missing/empty/
    /// duplicate `href`, duplicate/empty tags, mismatched/unclosed tags, entity references (including a
    /// DOCTYPE expansion attempt), malformed percent-escapes in the href text, and an XML-bomb-ish flood
    /// of thousands of `<d:response>` elements.
    fn parse_multistatus_battery() -> Vec<(String, String)> {
        let mut cases: Vec<(String, String)> = vec![
            ("empty".into(), String::new()),
            ("garbage_ascii".into(), "not xml at all !@#$%^&*()".into()),
            (
                "garbage_bytes_lossy".into(),
                String::from_utf8_lossy(&[0xFF, 0xFE, 0x00, 0x01, 0x80, 0x81, 0x9F, 0xC0, 0xC1, 0xF5, 0xFF])
                    .into_owned(),
            ),
            ("just_lt".into(), "<".into()),
            ("just_gt".into(), ">".into()),
            ("just_amp".into(), "&".into()),
            ("bare_open_tag".into(), "<d:multistatus xmlns:d=\"DAV:\">".into()),
            (
                "truncated_mid_tag".into(),
                "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/fo".into(),
            ),
            ("truncated_mid_attr".into(), "<d:multistatus xmlns:d=\"DAV:\"".into()),
            ("unclosed_response".into(), multistatus_wrap("<d:response><d:href>/foo</d:href>")),
            ("mismatched_tags".into(), multistatus_wrap("<d:response><d:href>/foo</d:wrong></d:response>")),
            (
                "missing_href".into(),
                multistatus_wrap(
                    "<d:response><d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat></d:response>",
                ),
            ),
            ("empty_href".into(), multistatus_wrap("<d:response><d:href></d:href></d:response>")),
            ("whitespace_href".into(), multistatus_wrap("<d:response><d:href>   </d:href></d:response>")),
            (
                "duplicate_href".into(),
                multistatus_wrap("<d:response><d:href>/a</d:href><d:href>/b</d:href></d:response>"),
            ),
            (
                "duplicate_getcontentlength".into(),
                multistatus_wrap(
                    r#"<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>5</d:getcontentlength><d:getcontentlength>999</d:getcontentlength></d:prop></d:propstat></d:response>"#,
                ),
            ),
            (
                "huge_getcontentlength".into(),
                multistatus_wrap(
                    r#"<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>999999999999999999999999999999999999999999</d:getcontentlength></d:prop></d:propstat></d:response>"#,
                ),
            ),
            (
                "negative_getcontentlength".into(),
                multistatus_wrap(
                    r#"<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>-1</d:getcontentlength></d:prop></d:propstat></d:response>"#,
                ),
            ),
            (
                "overflow_u64_plus_one_getcontentlength".into(),
                multistatus_wrap(
                    r#"<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>18446744073709551616</d:getcontentlength></d:prop></d:propstat></d:response>"#,
                ),
            ),
            (
                "nonnumeric_getcontentlength".into(),
                multistatus_wrap(
                    "<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>abc123</d:getcontentlength></d:prop></d:propstat></d:response>",
                ),
            ),
            (
                "whitespace_only_getcontentlength".into(),
                multistatus_wrap(
                    "<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength>   \t\n  </d:getcontentlength></d:prop></d:propstat></d:response>",
                ),
            ),
            (
                "empty_getcontentlength".into(),
                multistatus_wrap(
                    "<d:response><d:href>/a</d:href><d:propstat><d:prop><d:getcontentlength></d:getcontentlength></d:prop></d:propstat></d:response>",
                ),
            ),
            (
                "collection_trailing_slash_no_marker".into(),
                multistatus_wrap("<d:response><d:href>/a/</d:href></d:response>"),
            ),
            (
                "entity_refs".into(),
                multistatus_wrap("<d:response><d:href>/a&amp;b&lt;&gt;&quot;&apos;</d:href></d:response>"),
            ),
            ("undefined_entity".into(), multistatus_wrap("<d:response><d:href>/a&undefined;</d:href></d:response>")),
            (
                "cdata_and_comment".into(),
                multistatus_wrap("<!-- comment --><d:response><![CDATA[weird data]]><d:href>/a</d:href></d:response>"),
            ),
            (
                "doctype_entity_expansion_attempt".into(),
                "<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY lol \"lol\">]><d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>&lol;</d:href></d:response></d:multistatus>".into(),
            ),
            (
                "bad_xml_decl_encoding".into(),
                "<?xml version=\"1.0\" encoding=\"bogus-charset\"?><d:multistatus xmlns:d=\"DAV:\"></d:multistatus>".into(),
            ),
            (
                "null_byte_in_element_text".into(),
                "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/a\u{0}b</d:href></d:response></d:multistatus>".to_string(),
            ),
            ("malformed_percent_in_href".into(), multistatus_wrap("<d:response><d:href>/a%zz%b%</d:href></d:response>")),
            ("percent_at_very_end".into(), multistatus_wrap("<d:response><d:href>/a%4</d:href></d:response>")),
            (
                "many_attributes".into(),
                multistatus_wrap(&format!(
                    "<d:response {}><d:href>/a</d:href></d:response>",
                    (0..500).map(|i| format!("x{i}=\"v\"")).collect::<Vec<_>>().join(" ")
                )),
            ),
        ];

        // Deeply nested elements — stack safety, a few thousand deep, per the ticket. One case leaves the
        // root unclosed (also exercises "truncated mid-deep-nesting"), one closes everything properly.
        let depth = 4000;
        cases.push((
            "deeply_nested_elements_unclosed_root".into(),
            format!("<d:multistatus xmlns:d=\"DAV:\">{}{}", "<a>".repeat(depth), "</a>".repeat(depth)),
        ));
        cases.push((
            "deeply_nested_elements_closed".into(),
            format!("<d:multistatus xmlns:d=\"DAV:\">{}{}</d:multistatus>", "<a>".repeat(depth), "</a>".repeat(depth)),
        ));

        // An XML-bomb-ish flood of thousands of <d:response> elements (roxmltree has no DTD entity
        // expansion beyond the 5 predefined XML entities, so this is the realistic "huge document" shape
        // a hostile/buggy server could actually send, rather than a billion-laughs entity bomb).
        let mut flood = String::from("<d:multistatus xmlns:d=\"DAV:\">");
        for i in 0..8000 {
            flood.push_str(&format!(
                "<d:response><d:href>/f{i}</d:href><d:propstat><d:prop><d:getcontentlength>{i}</d:getcontentlength></d:prop></d:propstat></d:response>"
            ));
        }
        flood.push_str("</d:multistatus>");
        cases.push(("flood_of_responses".into(), flood));

        // Seeded pseudo-random garbage of a few sizes, lossily decoded to valid UTF-8 text (the lossy
        // decode is itself part of the adversarial input, same rationale as `garbage_bytes_lossy` above).
        for &n in &[16usize, 256, 4096] {
            let bytes = lcg_bytes(0x00C0_FFEE ^ n as u64, n);
            cases.push((format!("seeded_random_lossy_{n}"), String::from_utf8_lossy(&bytes).into_owned()));
        }

        cases
    }

    /// Run `parse_multistatus(xml, skip)` under `catch_unwind`; a panic (the parser's own, or this
    /// harness's own bug) is re-raised as one clearly-attributed failure naming the case and skip variant.
    fn assert_parse_multistatus_no_panic(case_name: &str, skip: Option<&str>, xml: &str) {
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_multistatus(xml, skip)));
        if let Err(payload) = result {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            panic!(
                "parse_multistatus panic-safety violation: case `{case_name}` (skip_path={skip:?}) did \
                 not return gracefully (Ok/Err) but unwound: {msg}"
            );
        }
    }

    #[test]
    fn parse_multistatus_skips_path_traversal_hrefs() {
        // CPE-1461 source-side defense: a hostile PROPFIND response whose href decodes to a traversal
        // name (`..` via `%2e%2e`, a Windows drive path, a backslash-separated escape) must be dropped —
        // never surfaced as a provider entry that could reach the local-write sink. A legit sibling in
        // the same response still parses, proving no over-rejection.
        // `%2e%2e` decodes to a leaf name of `..`; a backslash-bearing segment carries a Windows drive.
        // Both are the *last* href segment (the only part used as the name), so both must be dropped.
        let xml = multistatus_wrap(
            "<d:response><d:href>/dir/%2e%2e</d:href></d:response>\
             <d:response><d:href>/dir/C:\\Windows\\evil.bat</d:href></d:response>\
             <d:response><d:href>/dir/good.txt</d:href><d:propstat><d:prop>\
             <d:getcontentlength>7</d:getcontentlength></d:prop></d:propstat></d:response>",
        );
        let entries = parse_multistatus(&xml, None).expect("well-formed");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["good.txt"], "only the safe sibling should survive; got {names:?}");
    }

    #[test]
    fn parse_multistatus_never_panics_on_hostile_input() {
        let cases = parse_multistatus_battery();
        assert!(cases.len() >= 30, "battery should cover the full hostile-input class list; got {}", cases.len());
        // Run every case both with no skip and with a skip_path set, so both `normalize_href` call sites
        // (the skip-target comparison and the per-response href) are exercised together.
        let skip_variants: [Option<&str>; 2] = [None, Some("/")];
        for (name, xml) in &cases {
            for skip in skip_variants {
                assert_parse_multistatus_no_panic(name, skip, xml);
            }
        }
    }

    #[test]
    fn parse_multistatus_empty_input_is_a_graceful_err_not_a_panic() {
        // Explicit contract check for the simplest hostile case, kept separate from the battery above for
        // a clear, targeted failure message if it regresses.
        let r = panic::catch_unwind(|| parse_multistatus("", None));
        assert!(r.is_ok(), "parse_multistatus(\"\", None) must not panic");
        assert!(r.unwrap().is_err(), "parse_multistatus(\"\", None) should be Err (not valid XML)");
    }

    #[test]
    fn xml_nesting_guard_rejects_deep_nesting_before_it_reaches_roxmltree() {
        // Confirms the CPE-1398 stack-overflow fix: nesting past MAX_XML_NESTING_DEPTH is rejected by the
        // cheap pre-scan (fast, no parse attempted), while a shallow, realistic multistatus body is not.
        let deep = format!("<d:multistatus xmlns:d=\"DAV:\">{}{}</d:multistatus>", "<a>".repeat(4000), "</a>".repeat(4000));
        assert!(xml_nesting_too_deep(&deep, MAX_XML_NESTING_DEPTH));
        assert!(parse_multistatus(&deep, None).is_err());

        let shallow = multistatus_wrap("<d:response><d:href>/a</d:href></d:response>");
        assert!(!xml_nesting_too_deep(&shallow, MAX_XML_NESTING_DEPTH));
        assert!(parse_multistatus(&shallow, None).is_ok());
    }

    #[test]
    fn xml_nesting_guard_survives_the_quote_unaware_bypass() {
        // CPE-1398 follow-up: a Reviewer found and empirically reproduced a real bypass in a first version
        // of this guard, which found a tag's closing '>' with a quote-UNaware byte scan. `<a b="/>">` is
        // legal XML whose attribute value contains the literal bytes `/>` — the old scan landed on that
        // embedded '>', saw the preceding '/', and wrongly concluded the tag was self-closing, so it was
        // never counted toward depth even though it's a real child-bearing open element. The Reviewer's
        // exact reproduction: `<a b="/>">` x2000 + `</a>` x2000 (~28KB) passed the old guard (returned
        // `false`) and then reliably crashed the process with an uncatchable stack overflow when handed to
        // `parse_multistatus` on a small thread. The fix replaced the hand-rolled scan with the real
        // `xmlparser::Tokenizer` (quote/comment/CDATA/PI-aware by construction), so this must now be
        // caught — under `catch_unwind` too, as a defense-in-depth check that it's a graceful `Err`, not
        // merely "the guard function returns true in isolation".
        let n = 2000;
        let bypass = format!(
            "<d:multistatus xmlns:d=\"DAV:\">{}{}</d:multistatus>",
            "<a b=\"/>\">".repeat(n),
            "</a>".repeat(n)
        );
        assert!(
            xml_nesting_too_deep(&bypass, MAX_XML_NESTING_DEPTH),
            "the quote-unaware-scan bypass shape must be recognized as too deep"
        );
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_multistatus(&bypass, None)));
        assert!(result.is_ok(), "parse_multistatus must not panic/crash on the bypass payload");
        assert!(result.unwrap().is_err(), "parse_multistatus must return Err for the bypass payload");
    }

    #[test]
    fn xml_nesting_guard_is_not_confused_by_gt_inside_comments_cdata_or_pis() {
        // Decoys containing a literal '>' inside constructs that don't nest (comments, CDATA, processing
        // instructions) must neither cause a false positive on shallow real input nor mask genuinely deep
        // real nesting — i.e. the guard must count real element tags only, regardless of '>' noise
        // elsewhere. Covers the same evasion *class* the quote-unaware scan fell to (misreading a literal
        // '>' that isn't really a tag terminator), just via different XML constructs than quoted attrs.

        // Shallow real nesting (5 levels) laced with '>'-bearing decoys: must NOT be flagged, and must
        // still parse to the expected entries (no false positive from the fix).
        let shallow = "<?xml version=\"1.0\"?><!-- a > b --><d:multistatus xmlns:d=\"DAV:\">\
             <![CDATA[ > ]]><?pi content > more?>\
             <d:response><!-- > --><d:href>/a</d:href><d:propstat><d:prop>\
             <![CDATA[>]]><d:getcontentlength>5</d:getcontentlength></d:prop></d:propstat></d:response>\
             </d:multistatus>"
            .to_string();
        assert!(!xml_nesting_too_deep(&shallow, MAX_XML_NESTING_DEPTH), "decoys must not inflate depth");
        let entries = parse_multistatus(&shallow, None).expect("well-formed despite the decoys");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[0].size, 5);

        // Genuinely deep real nesting (2000 levels) with the same kinds of '>'-bearing decoys interleaved
        // between real tags: the decoys must NOT mask the real depth (still correctly rejected), and the
        // call must still be a graceful Err under catch_unwind, never a crash.
        let n = 2000;
        let open = "<a><!-- > --><![CDATA[>]]>".repeat(n);
        let close = "</a>".repeat(n);
        let deep_with_decoys =
            format!("<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\">{open}{close}</d:multistatus>");
        assert!(xml_nesting_too_deep(&deep_with_decoys, MAX_XML_NESTING_DEPTH), "decoys must not mask real depth");
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_multistatus(&deep_with_decoys, None)));
        assert!(result.is_ok(), "must not panic/crash even with decoys present");
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn xml_nesting_guard_accepts_a_realistic_five_level_multistatus() {
        // A real PROPFIND response is only ever a handful of levels deep
        // (multistatus > response > propstat > prop > getcontentlength, 5 levels) — confirm the guard
        // never flags realistic traffic and the entries it parses are exactly right, so the fix has zero
        // false-positive cost on legitimate servers.
        let xml = multistatus_wrap(&format!(
            "{}{}",
            dav_response(&href_for("/docs", true), true, 0),
            dav_response(&href_for("/docs/readme.txt", false), false, 12),
        ));
        assert!(!xml_nesting_too_deep(&xml, MAX_XML_NESTING_DEPTH));
        let mut entries = parse_multistatus(&xml, None).expect("realistic multistatus must parse");
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "docs");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "readme.txt");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].size, 12);
    }
}
