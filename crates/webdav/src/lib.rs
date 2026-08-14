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
use std::time::Duration;

/// How long a single socket read may make **no progress at all** before the request is abandoned
/// (CPE-1706 item 1). Until that ticket this crate built `AgentBuilder::new().redirects(0)` and nothing
/// else, and **`ureq` 2.x defaults `timeout_read`, `timeout_write` and the overall `timeout` all to
/// `None`** — `AgentBuilder::timeout_read`'s own doc says it plainly: *"requests may block forever on reads
/// by default"*. Since `WebdavProvider` runs on `spawn_blocking` threads, one unresponsive share could hold
/// a pool thread with nothing able to reclaim it.
///
/// This is a **stall** bound, not a transfer budget: the clock restarts on every byte that arrives, so a
/// large `read` over a genuinely poor link is never cut off for being slow, only for having stopped. 30 s
/// is a wide margin over the time-to-first-byte of any real share — a NAS that has sent nothing for half a
/// minute is not slow, it is gone — and matches the value `cpe-s3` chose for the same knob, so the two
/// HTTP backends behave alike.
const TIMEOUT_READ: Duration = Duration::from_secs(30);

/// The write-side twin of [`TIMEOUT_READ`]: how long one socket write may block with the peer's receive
/// window shut. Same value, same reasoning. It bounds a stalled `PUT`, not a slow one.
const TIMEOUT_WRITE: Duration = Duration::from_secs(30);

/// Pinned explicitly rather than inherited: `ureq` 2.12.1 already defaults `timeout_connect` to 30 s
/// (`agent.rs:256`), so connect was the one phase that was never unbounded. Setting it to the same value
/// changes nothing today and stops a future `ureq` default change from silently unbounding it.
const TIMEOUT_CONNECT: Duration = Duration::from_secs(30);

/// End-to-end deadline for one **metadata** exchange — `PROPFIND`, `MKCOL`, `DELETE`, `MOVE` — applied
/// per call site by [`WebdavProvider::request_bounded`] (CPE-1706 round 2). See that method for why a
/// per-request bound is the only one of the three knobs that closes a dribbling server, and why it is
/// deliberately *not* applied to `read`/`write`.
///
/// # This value has to do two jobs, because `ureq` makes it an either/or
/// A per-request deadline **replaces** [`TIMEOUT_READ`] for that request (`ureq` `stream.rs:433-436`
/// takes the deadline branch *instead of* `config.timeout_read`), so for a `PROPFIND` this is now the
/// *only* bound. It must both exceed the slowest legitimate metadata exchange **and** stay short enough
/// that a dead share still fails promptly — the accept-then-silence case that [`TIMEOUT_READ`] used to
/// catch in 30 s is now this constant's job too.
///
/// **60 s.** A `PROPFIND` body is already memory-bounded at 10 MiB by `ureq`'s own `into_string` limit
/// (`response.rs:33` — a loud error, not a truncation), and a realistic `Depth: 1` listing is well under
/// a megabyte, which is seconds even on a punishing link. 60 s doubles the old dead-share wait rather
/// than quadrupling it; 120 s was tried first and rejected for buying no real listing any safety while
/// making a dead share take two minutes to report. Matches `cpe-s3`'s `TIMEOUT_LIST_REQUEST` so the two
/// HTTP backends behave alike.
const TIMEOUT_METADATA_REQUEST: Duration = Duration::from_secs(60);

/// Upper bound on how many bytes [`WebdavProvider::read`] will buffer for one file (CPE-1706 round 2).
///
/// `read` materialises a whole remote file into a `Vec<u8>` — that is the [`FileSystemProvider::read`]
/// contract, shared with `cpe-sftp` and `cpe-ftp` — and until now it did so with a bare `read_to_end`,
/// **uncapped**, driven directly by server-controlled data. A hostile or broken share could stream until
/// the process died of allocation failure.
///
/// **An over-cap read is a loud `Err`, never a truncated `Vec` returned as if it were the file.** That
/// distinction is the whole design: silently handing back a partial file would be far worse than
/// refusing, because the caller writes it to disk as the complete download (`cpe_server::transfer`'s
/// `download_tree` is the real consumer). Refusing is recoverable; a silently truncated file is data loss
/// that looks like success.
///
/// **2 GiB**, chosen as a memory backstop rather than a file-size policy. No legitimate use of this app
/// reads a 2 GiB file *into RAM* successfully anyway — at that size the whole-file-in-a-`Vec` contract is
/// itself the problem — so this refuses the pathological case without narrowing what actually works
/// today. Note `cpe-ftp`'s module doc already names the real fix ("a whole-file cap is a broader,
/// pre-existing trait-level question"): streaming `read` into a sink belongs at the trait, across all
/// four remote backends, and is **not** in this ticket's scope. This is the backstop until then.
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Build the `ureq::Agent` every request in this crate goes through — the single place the transport's
/// bounds are set, so `connect` and any test-injected variant cannot drift apart (only the two `Duration`s
/// differ between them).
///
/// **The AGENT-level `ureq` `.timeout()` is deliberately not set.** It would cap every request regardless
/// of progress, killing a legitimate multi-minute `GET` of a large file over a bad connection — a real
/// user, not a hypothetical — and it *replaces* the per-read bound rather than adding to it (`ureq`
/// `agent.rs:476-477`: "takes precedence over `.timeout_read()`"), so it would trade a good bound for a
/// worse one.
///
/// **That reasoning was right about the agent and wrong to stop there (CPE-1706 round 2).** Declining the
/// agent-level knob is not the same as declining an end-to-end bound everywhere: `ureq::Request::timeout`
/// is *per request*, so it can be applied to the small metadata exchanges and withheld from the large
/// transfers. Without it these per-read bounds left a real hole — a server that sends valid `200 OK`
/// headers and then dribbles one byte every 29 s restarts the per-read clock forever and was measured
/// holding a `list` thread indefinitely. See [`WebdavProvider::request_bounded`] and
/// [`TIMEOUT_METADATA_REQUEST`]. `read`/`write` still take the per-read bound only, on purpose.
///
/// **State the residual rather than only the mechanism (CPE-1706 UAT).** The consequence of that last
/// sentence is that `read` remains **time-unbounded against a dribbling server**: measured still running
/// past 150 s at shipped values. Only its *memory* is capped, by `read_cap` at 2 GiB, so the true bound is
/// 2 GiB × the per-read interval — one held `spawn_blocking` thread for a very long time. This is a
/// deliberate trade, not an oversight: an end-to-end deadline on `read` would kill a legitimate
/// multi-minute download of a large file for the crime of being slow, and unlike `list` — which the app
/// issues automatically on navigation — `read` requires a user to ask for a download. The exposure is
/// therefore one thread per user action rather than one per navigation. Recorded here because this whole
/// ticket exists to fix a comment that named a mechanism and let a reader infer a bound it did not have.
///
/// Unlike `cpe-s3`, this crate needs no *listing*-level wall-clock budget: `list` is a single `PROPFIND`
/// with `Depth: 1` and no pagination loop, so there is no page count to multiply — the per-request bound
/// bounds the whole operation. (`cpe-s3`'s `list` follows up to 1000 `ListObjectsV2` pages, which is why
/// it carries `MAX_LIST_WALL_CLOCK` on top of these.)
///
/// `redirects(0)` is the pre-existing CPE-1461 policy, kept — see [`WebdavProvider::connect`].
/// `timeout_connect` is a parameter rather than a direct read of [`TIMEOUT_CONNECT`] for a testing
/// reason (CPE-1706 round 2): `ureq`'s own default for that knob is *also* 30 s, so asserting the built
/// agent has `timeout_connect: Some(30s)` passes identically whether the line is wired or deleted. Taking
/// it as a parameter lets the guard test pass a value nothing else would produce.
fn build_agent(timeout_read: Duration, timeout_write: Duration, timeout_connect: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(timeout_connect)
        .timeout_read(timeout_read)
        .timeout_write(timeout_write)
        .build()
}

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
    /// End-to-end deadline for one metadata exchange — [`TIMEOUT_METADATA_REQUEST`] in production. A
    /// field so the dribble guard can be observed firing in a second instead of a minute, and it must be
    /// overridable *separately* from the agent's `timeout_read`, because setting a per-request deadline
    /// replaces that read timeout rather than layering on it.
    metadata_deadline: Duration,
    /// Per-file byte cap for `read` — [`MAX_READ_BYTES`] in production, a field so the refusal can be
    /// exercised on kilobytes instead of gigabytes.
    read_cap: u64,
}

/// The PROPFIND body requesting the properties we need (resource type + content length + name).
const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop>
  <d:resourcetype/><d:getcontentlength/><d:displayname/>
</d:prop></d:propfind>"#;

impl WebdavProvider {
    /// Build a provider for `config`. Does not perform a request (WebDAV is stateless HTTP); the first
    /// `list`/`read`/… issues a request and surfaces auth/connection errors then.
    /// This is the constructor production uses, and the only place the shipped timeout values are chosen
    /// — see [`TIMEOUT_READ`] and [`build_agent`].
    pub fn connect(config: &WebdavConfig) -> Self {
        Self::connect_with_timeouts(config, TIMEOUT_READ, TIMEOUT_WRITE)
    }

    /// [`WebdavProvider::connect`] with the transport's stall bounds supplied by the caller instead of
    /// taken from [`TIMEOUT_READ`]/[`TIMEOUT_WRITE`].
    ///
    /// Public because a caller on a pathologically slow share has a legitimate reason to widen them, but
    /// its first use is this crate's own tests: a stalling-server test that had to wait out the shipped
    /// 30 s would cost 30 s of CI wall clock on three OSes, so the test injects a short bound and drives
    /// the *same* [`build_agent`] path production drives — only the `Duration`s differ. The shipped values
    /// themselves are pinned separately by `tests::the_shipped_timeout_values_are_finite_and_sane`.
    pub fn connect_with_timeouts(
        config: &WebdavConfig,
        timeout_read: Duration,
        timeout_write: Duration,
    ) -> Self {
        let auth_header = config.user.as_deref().map(|u| {
            let pass = config.password.as_deref().unwrap_or("");
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{pass}"));
            format!("Basic {token}")
        });
        // Pin an explicit no-redirect policy (CPE-1461 watch-item): the default agent follows up to 5
        // redirects, so a future change (or a hostile server today) could bounce a request via a `3xx`
        // toward `file://` or an attacker-controlled host — an SSRF/exfiltration foothold. Refusing to
        // auto-follow surfaces the `3xx` as an error instead; a WebDAV share never needs redirects.
        let agent = build_agent(timeout_read, timeout_write, TIMEOUT_CONNECT);
        WebdavProvider {
            agent,
            base_url: config.base_url.clone(),
            auth_header,
            metadata_deadline: TIMEOUT_METADATA_REQUEST,
            read_cap: MAX_READ_BYTES,
        }
    }

    /// Override the per-request metadata deadline ([`TIMEOUT_METADATA_REQUEST`] by default). Production
    /// never calls this; it exists so the dribble guard can be observed firing in a second instead of a
    /// minute.
    pub fn with_metadata_deadline(mut self, deadline: Duration) -> Self {
        self.metadata_deadline = deadline;
        self
    }

    /// Override the per-file read cap ([`MAX_READ_BYTES`] by default). Production never calls this; it
    /// exists so the refusal can be observed on a few kilobytes instead of two gigabytes.
    pub fn with_read_cap(mut self, cap: u64) -> Self {
        self.read_cap = cap;
        self
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

    /// [`WebdavProvider::request`] plus an end-to-end [`TIMEOUT_METADATA_REQUEST`] deadline, for the
    /// **small, fixed-size** exchanges: `PROPFIND` (list/stat), `MKCOL`, `DELETE`, `MOVE`.
    ///
    /// This is the CPE-1706 round-2 fix and the distinction is the entire point: [`TIMEOUT_READ`] is
    /// per-read, so its clock restarts on every byte and a server that sends valid headers then dribbles
    /// **one byte every 29 s** is never cut off. `ureq::Request::timeout` (`request.rs:60`) is per
    /// *request* — `DeadlineStream::fill_buf` recomputes the remaining budget on every read
    /// (`stream.rs:85-89`), and the deadline propagates into the response body reader — so it bounds the
    /// whole exchange. Because it is set per call site rather than on the agent, `read`'s `GET` and
    /// `write`'s `PUT` keep the per-read semantics they actually want: those carry user file data of
    /// unbounded size, where "slow but progressing" is a legitimate transfer, not an attack.
    fn request_bounded(&self, method: &str, path: &str) -> ureq::Request {
        self.request(method, path).timeout(self.metadata_deadline)
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
            .request_bounded("PROPFIND", path)
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
            .request_bounded("PROPFIND", path)
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

    /// Deliberately uses the *unbounded-per-request* [`WebdavProvider::request`]: a large file over a poor
    /// link is a legitimate multi-minute transfer, and an end-to-end deadline would kill it for being slow
    /// rather than for being stalled. [`TIMEOUT_READ`] is the right bound here — it fires when the server
    /// stops sending, not when the file is big. Memory is bounded separately by [`MAX_READ_BYTES`].
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let resp = self.request("GET", path).call().map_err(|e| http_err(path, e))?;
        let mut buf = Vec::new();
        // Read one byte MORE than the cap, so that hitting the cap is distinguishable from a file that is
        // exactly cap-sized — then refuse loudly. Never return the truncated prefix: `download_tree`
        // writes whatever comes back to disk as the finished file, so a silent truncation here is data
        // loss wearing a success. See MAX_READ_BYTES.
        resp.into_reader()
            .take(self.read_cap + 1)
            .read_to_end(&mut buf)
            .map_err(|e| format!("{path}: {e}"))?;
        if buf.len() as u64 > self.read_cap {
            return Err(format!(
                "{path}: the server sent more than the {}-byte read cap without finishing — refusing \
                 rather than returning a truncated file as if it were complete",
                self.read_cap
            ));
        }
        Ok(buf)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        self.request("PUT", path).send_bytes(data).map_err(|e| http_err(path, e))?;
        Ok(())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.request_bounded("MKCOL", path).call().map_err(|e| http_err(path, e))?;
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        let resp = self.request_bounded("DELETE", path).call().map_err(|e| http_err(path, e))?;
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
            let retry = self.request_bounded("DELETE", &with_slash).call().map_err(|e| http_err(path, e))?;
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
        self.request_bounded("MOVE", from)
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
                // CPE-1726, the sibling primitive (CPE-1719's failure shape, checked here rather than
                // only `rename`): `fs::write` **follows** a link at the final component and writes
                // *through* it, so a PUT onto a symlink clobbers the link's target rather than the link.
                // Left as-is for the same measured reason as MOVE below — this is `#[cfg(test)]`-only,
                // and a real WebDAV share follows the link too — but recorded so the next sweep does not
                // have to rediscover which of the two shapes this is.
                let code = if std::fs::write(&real, &body).is_ok() { 201 } else { 500 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            "MKCOL" => {
                let code = if std::fs::create_dir_all(&real).is_ok() { 201 } else { 500 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            "DELETE" => {
                // CPE-1726 (found while sweeping the sibling primitives, and the one place this crate's
                // rig differed from `cpe-ftp`'s and `cpe-sftp`'s): those two get `DELE`/`RMD` and
                // `remove`/`rmdir` as *separate wire verbs*, so they never have to classify. WebDAV's
                // `DELETE` is one verb for both, and the classifier was `real.is_dir()`, which
                // **follows** the final component — a symlink to a directory answered "directory" and
                // went to `remove_dir_all`, recursing through the link into whatever it points at.
                // `symlink_metadata` never follows, so one stat answers link / dir / file on its own
                // (CPE-1719's measurement); a link is unlinked, never traversed.
                let r = match std::fs::symlink_metadata(&real).map(|m| m.file_type()) {
                    Ok(t) if t.is_dir() => std::fs::remove_dir_all(&real),
                    Ok(_) => std::fs::remove_file(&real),
                    Err(e) => Err(e),
                };
                let _ = req.respond(tiny_http::Response::empty(if r.is_ok() { 204 } else { 404 }));
            }
            "MOVE" => {
                // The Destination header is an absolute URL; map its path under `root`.
                //
                // CPE-1726: this used to end `.unwrap_or_default()` and then `root.join(dest_path)`, so
                // an **absent or malformed** `Destination` collapsed to the empty string and the
                // destination silently became the server root itself — a request that named no target at
                // all still got handed a real, live path to rename onto. A `None` here is now a 400,
                // which is both what RFC 4918 §9.9.4 says and the honest answer: a rig that invents a
                // destination when the client supplied none cannot be trusted to be modelling the wire.
                let dest_path = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Destination"))
                    .map(|h| h.value.as_str().to_string())
                    .and_then(|u| u.rsplit_once("://").map(|(_, rest)| rest.to_string()))
                    .and_then(|rest| rest.split_once('/').map(|(_, p)| p.to_string()))
                    .filter(|p| !p.is_empty());
                let Some(dest_path) = dest_path else {
                    let _ = req.respond(tiny_http::Response::empty(400));
                    return;
                };
                let dest_real = root.join(dest_path.trim_start_matches('/'));
                // CPE-1726 re-took CPE-1710's classification against a **measurement** instead of a
                // category ("it is a protocol server" is a category). DELIBERATELY UNGUARDED — do not
                // wrap this in `cpe_server::fsutil::rename_into_slot`; the measurement is:
                //
                // 1. This entire WebDAV server is `#[cfg(test)]`. `cpe-webdav` ships a *client*
                //    ([`WebdavProvider`]) and no server, so this line is not compiled into the app. The
                //    "remote client" supplying the Destination header is a test in this same file, over
                //    loopback, against a per-test temp root this rig created and seeded itself. There is
                //    no third party whose files sit at the destination — the premise the ticket weighed
                //    ("the client is not the person whose files are there") is simply absent here, and
                //    that absence is what decides it.
                // 2. That premise is pinned rather than trusted:
                //    `cpe_1726_every_destructive_filesystem_call_is_confined_to_the_test_rig` goes red
                //    the moment this line (or any sibling destructive primitive) moves above the
                //    `#[cfg(test)]` marker, so promoting the rig to production forces the decision to be
                //    re-taken rather than silently inherited.
                // 3. A test double must model the wire, not defend against it. MOVE with the default
                //    `Overwrite: T` is *defined* to replace the destination; hardening the rig would make
                //    the client tests pass against a server unlike any Nextcloud the app will ever meet.
                //
                // What `fs::rename` does to a link at the destination is pinned, not assumed, by
                // `cpe_1726_rename_onto_a_link_never_writes_through_it`.
                #[allow(clippy::disallowed_methods)]
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
        spawn_webdav_server_returning_root().0
    }

    /// Like [`spawn_webdav_server`] but also hands back the server's on-disk root, so a test can stage a
    /// condition *inside* the served tree (CPE-1726 needs a symlink sitting at a MOVE destination) rather
    /// than only driving it through the wire. Same seeding, same uniqueness scheme — the root is simply
    /// not thrown away.
    fn spawn_webdav_server_returning_root() -> (String, PathBuf) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // `(pid, n)` alone is NOT unique across runs: these directories are never cleaned up, and Windows
        // reuses process ids freely, so a later run can land on a previous run's root and inherit its
        // files. That bit during CPE-1706 — `lists_stats_and_reads_over_webdav` saw a `renamed.txt` left
        // by `rename_moves_a_file_over_webdav` from an earlier process with the same pid (223 stale roots
        // were sitting in the temp dir at the time). A nanosecond stamp makes reuse collide only if two
        // roots are created in the same nanosecond by the same pid with the same counter.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root: PathBuf =
            std::env::temp_dir().join(format!("cpe-webdav-{}-{}-{}", std::process::id(), n, stamp));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("readme.txt"), FILE_BODY).unwrap();
        std::fs::write(root.join("sub").join("nested.txt"), b"deep").unwrap();
        let root_ret = root.clone();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &root);
            }
        });
        (format!("http://{addr}"), root_ret)
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 1: wall-clock is bounded — and the test proving it cannot itself hang CI.
    // ---------------------------------------------------------------------------------------------

    /// Run `f` on a spawned thread and fail the test if it has not returned within `deadline`.
    ///
    /// **libtest has no per-test timeout**, so a test whose subject is "this call cannot block forever"
    /// regresses into a *hang*, not a red: with the bound gone there is nothing left to end the call and
    /// `cargo test` would sit there until the CI job's own limit killed it. This converts that into a
    /// deterministic red naming what happened. (Ported from `crates/s3`'s `provider.rs`, CPE-1706 item 5.)
    fn call_with_deadline<T: Send + 'static>(
        what: &str,
        deadline: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(deadline) {
            Ok(value) => value,
            Err(_) => panic!(
                "{what} did not return within {deadline:?}. The bound that was supposed to stop it is \
                 gone, so this call would have run forever — libtest has no per-test timeout, so without \
                 this deadline the CI job would have hung until its own limit rather than reporting a \
                 failure."
            ),
        }
    }

    /// A server that completes the TCP accept and then **never sends a byte**, holding every connection
    /// open forever. Holding the accepted streams in a `Vec` is what makes it a stall rather than a reset:
    /// dropping them would close the socket and the client would get a prompt EOF, proving nothing.
    fn spawn_a_server_that_accepts_and_never_answers() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                match stream {
                    Ok(s) => held.push(s),
                    Err(_) => break,
                }
            }
        });
        format!("http://{addr}")
    }

    /// A server that completes the accept, sends a **valid `200 OK` header block**, then emits its body
    /// **one byte at a time with `gap` between bytes, forever** (CPE-1706 round 2). A per-read timeout's
    /// clock restarts on every byte, so this peer never trips one — it is not stalled at any instant,
    /// only useless in aggregate. Only an end-to-end per-request deadline sees it.
    fn spawn_a_server_that_dribbles_one_byte_at_a_time(gap: Duration) -> String {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                std::thread::spawn(move || {
                    let head = "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml\r\n\
                                Content-Length: 8388608\r\n\r\n";
                    if s.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    let _ = s.flush();
                    let body = b"<d:multistatus xmlns:d=\"DAV:\"></d:multistatus>";
                    let mut i = 0usize;
                    loop {
                        std::thread::sleep(gap);
                        if s.write_all(&[body[i % body.len()]]).is_err() || s.flush().is_err() {
                            return;
                        }
                        i += 1;
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    /// **The round-2 blocking finding, pinned at the SHIPPED values, on the live path.** WebDAV is routed
    /// through `crates/vfs` today (`cpe_vfs::open` → `remote_dir_entries` → the `list_dir` Tauri command,
    /// inside `spawn_blocking`), and tokio's default 512-thread blocking pool is not configured smaller —
    /// so before this fix each attempt against a dribbling share drained one pool thread permanently.
    /// This runs the real [`WebdavProvider::connect`], no injected `Duration`, and requires `list` to
    /// come back. It costs [`TIMEOUT_METADATA_REQUEST`] of wall clock per CI job on purpose: the previous
    /// round proved the mechanism through a seam and shipped values that did not bound this.
    #[test]
    fn a_server_that_dribbles_one_byte_at_a_time_is_cut_off_at_the_shipped_values() {
        let base = spawn_a_server_that_dribbles_one_byte_at_a_time(Duration::from_secs(5));
        let started = std::time::Instant::now();
        let err = call_with_deadline(
            "WebdavProvider::list (SHIPPED values) against a server dribbling one byte every 5 s",
            Duration::from_secs(150),
            move || WebdavProvider::connect(&WebdavConfig::new(&base)).list("/"),
        )
        .unwrap_err();
        let elapsed = started.elapsed();
        assert!(err.starts_with("/:"), "the error must name the path that dribbled: {err}");
        assert!(
            elapsed >= TIMEOUT_METADATA_REQUEST,
            "returning BEFORE the deadline means something else ended this, so it is not evidence about \
             the deadline: {elapsed:?}"
        );
        assert!(
            elapsed < TIMEOUT_METADATA_REQUEST + Duration::from_secs(30),
            "the request deadline must be what ended it, but it took {elapsed:?}"
        );
    }

    /// The fast twin of the shipped-values test above, through the same `request_bounded` →
    /// `.timeout(..)` path with a short deadline injected, so the guard can be broken and observed in a
    /// second during iteration.
    #[test]
    fn a_dribbling_server_is_cut_off_by_the_per_request_deadline() {
        let base = spawn_a_server_that_dribbles_one_byte_at_a_time(Duration::from_millis(50));
        let started = std::time::Instant::now();
        let err = call_with_deadline(
            "WebdavProvider::list against a dribbling server under a 500 ms metadata deadline",
            Duration::from_secs(30),
            move || {
                WebdavProvider::connect(&WebdavConfig::new(&base))
                    .with_metadata_deadline(Duration::from_millis(500))
                    .list("/")
            },
        )
        .unwrap_err();
        let elapsed = started.elapsed();
        assert!(err.starts_with("/:"), "{err}");
        assert!(
            elapsed < Duration::from_secs(10),
            "the 500 ms metadata deadline must be what ended it, but it took {elapsed:?}"
        );
    }

    /// Every knob [`build_agent`] sets, asserted on the **real production agent** (CPE-1706 round 2).
    /// `timeout_write` and `timeout_connect` shipped in round 1 with no guard — deleting either left all
    /// 16 tests passing. `timeout_write` is close to untestable end-to-end (a `PROPFIND` body is a few
    /// hundred bytes and always fits the socket buffer), and `timeout_connect`'s stated rationale was to
    /// pin the value against a future `ureq` default change, which a deleted line defeats invisibly.
    /// `ureq::Agent` derives `Debug` over its `AgentConfig` — the same surface `ureq`'s own
    /// `agent_config_debug` test uses — so this inspects the built agent rather than the constants.
    #[test]
    fn build_agent_wires_every_timeout_knob_and_deliberately_leaves_the_agent_level_one_unset() {
        // Distinctive values nothing else would produce. ureq DEFAULTS timeout_connect to 30 s, so
        // asserting `Some(30s)` would pass with the line deleted.
        let agent = format!("{:?}", build_agent(Duration::from_secs(11), Duration::from_secs(12), Duration::from_secs(13)));
        assert!(agent.contains("timeout_read: Some(11s)"), "timeout_read is not wired: {agent}");
        assert!(agent.contains("timeout_write: Some(12s)"), "timeout_write is not wired: {agent}");
        assert!(
            agent.contains("timeout_connect: Some(13s)"),
            "timeout_connect is not wired — ureq's own default is 30s, so a `Some(30s)` here would mean              the line was DELETED and the default was showing through: {agent}"
        );
        assert!(
            agent.contains("timeout: None"),
            "the AGENT-level overall timeout must stay unset — it takes precedence over timeout_read \
             (ureq agent.rs:476-477) and would cap a legitimate multi-minute GET of a large file by wall \
             clock regardless of progress. Metadata requests get a per-REQUEST deadline instead: {agent}"
        );
        assert!(agent.contains("redirects: 0"), "the CPE-1461 no-redirect policy is not wired: {agent}");
    }

    /// `read` deliberately keeps per-read semantics (a big file over a poor link is a legitimate slow
    /// transfer), so its protection is a **byte** cap, not a time one — and the cap must refuse rather
    /// than hand back a truncated prefix, because `download_tree` writes whatever comes back to disk as
    /// the finished file. This drives the real `read` against a server that streams past the cap.
    #[test]
    fn a_read_that_runs_past_the_byte_cap_is_refused_rather_than_truncated() {
        // An ordinary file must round-trip untouched through the real read path — the cap must not be
        // something a normal download can feel.
        let base = spawn_webdav_server();
        let provider = WebdavProvider::connect(&WebdavConfig::new(&base));
        assert_eq!(
            provider.read("/readme.txt").unwrap(),
            FILE_BODY,
            "the cap must not disturb an ordinary read"
        );

        // And the refusal itself, driven for real: a server that declares a huge Content-Length and
        // streams forever. `read` must come back with an Err naming the cap, never an Ok holding the
        // prefix — `download_tree` writes whatever comes back to disk as the finished file, so a silent
        // truncation here is data loss wearing a success. Uses a 4 KiB cap injected through the same
        // field production fills from MAX_READ_BYTES.
        let endless = spawn_a_server_that_streams_forever();
        let capped = WebdavProvider::connect(&WebdavConfig::new(&endless)).with_read_cap(4096);
        let result = call_with_deadline(
            "WebdavProvider::read against a server that never stops sending",
            Duration::from_secs(30),
            move || capped.read("/big.bin"),
        );
        match result {
            Err(e) => assert!(
                e.contains("read cap"),
                "the error must name the cap so the cause is diagnosable: {e}"
            ),
            Ok(bytes) => panic!(
                "an endless stream came back as a {}-byte file — a truncated prefix returned as the \
                 complete download is data loss that looks like success",
                bytes.len()
            ),
        }
    }

    /// Sends a valid `200 OK` with a huge declared length and then streams bytes as fast as the client
    /// will take them, forever — the shape a byte cap exists for. Distinct from the dribbler above: this
    /// one is not slow, it is simply endless, so no time-based bound would ever stop it.
    fn spawn_a_server_that_streams_forever() -> String {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                std::thread::spawn(move || {
                    let head = "HTTP/1.1 200 OK\r\nContent-Length: 1099511627776\r\n\r\n";
                    if s.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    let chunk = [b'x'; 4096];
                    while s.write_all(&chunk).is_ok() {}
                });
            }
        });
        format!("http://{addr}")
    }

    /// The per-read socket bound, driven through the production request path: `connect_with_timeouts` and
    /// `connect` differ only in where the two `Duration`s come from — both build the agent via
    /// `build_agent`, and the call then goes through the same `request()` → `send_string()` production
    /// uses. A short bound is injected because waiting out the shipped 30 s would cost 30 s of wall clock
    /// in every CI job on three OSes; the shipped values are pinned by
    /// [`the_shipped_timeout_values_are_finite_and_sane`].
    ///
    /// Checked on `list` **and** `read`, because they take different `ureq` paths — `send_string` +
    /// `into_string` for the PROPFIND, `call` + `into_reader` for the GET — and it is the GET path where an
    /// unbounded read would hurt most (a large download from a share that dies mid-transfer).
    ///
    /// The error *text* is not asserted beyond the path prefix `http_err` adds: a socket read timeout
    /// surfaces through `std::io` differently per platform (`WouldBlock` — "Resource temporarily
    /// unavailable" — on Unix, `TimedOut` on Windows) and this repo runs a 3-OS CI matrix. What is asserted
    /// is the behaviour under test, identical everywhere: it **returned, with an error, quickly**.
    #[test]
    fn a_server_that_accepts_the_connection_and_then_never_answers_is_cut_off_by_the_read_timeout() {
        for op in ["list", "read"] {
            let base = spawn_a_server_that_accepts_and_never_answers();
            let short = Duration::from_millis(300);
            let started = std::time::Instant::now();
            let err = call_with_deadline(
                &format!("WebdavProvider::{op} against a server that accepts and never answers"),
                Duration::from_secs(30),
                move || {
                    // The metadata deadline is shortened alongside the read timeout because a
                    // per-request deadline REPLACES `timeout_read` for that request (`ureq`
                    // `stream.rs:433-436`); left at the shipped 60 s this test would be measuring the
                    // deadline, not the read timeout it claims to measure.
                    let p = WebdavProvider::connect_with_timeouts(&WebdavConfig::new(&base), short, short)
                        .with_metadata_deadline(short);
                    match op {
                        "list" => p.list("/readme.txt").map(|_| ()),
                        _ => p.read("/readme.txt").map(|_| ()),
                    }
                },
            )
            .unwrap_err();
            let elapsed = started.elapsed();
            assert!(
                err.starts_with("/readme.txt:"),
                "the error must name the path that stalled, so a user knows what to blame: {err}"
            );
            assert!(
                elapsed < Duration::from_secs(10),
                "the 300 ms read timeout — not some other accident — must be what ended {op}, but it took \
                 {elapsed:?}"
            );
        }
    }

    /// The companion to the test above: that one proves the *mechanism* through the production builder
    /// with an injected `Duration`, this pins the *values* `connect` actually installs, which it
    /// deliberately does not wait out. Together they cover "`WebdavProvider::connect` produces an agent
    /// whose reads and writes are bounded by a finite, sane timeout": remove `.timeout_read(..)` from
    /// `build_agent` and the first reds; make `TIMEOUT_READ` useless and this one does.
    #[test]
    fn the_shipped_timeout_values_are_finite_and_sane() {
        for (name, value) in [
            ("TIMEOUT_READ", TIMEOUT_READ),
            ("TIMEOUT_WRITE", TIMEOUT_WRITE),
            ("TIMEOUT_CONNECT", TIMEOUT_CONNECT),
            ("TIMEOUT_METADATA_REQUEST", TIMEOUT_METADATA_REQUEST),
        ] {
            assert!(
                value >= Duration::from_secs(5),
                "{name} = {value:?} is short enough to cut off a legitimately slow share's \
                 time-to-first-byte — this knob bounds a stall, not a transfer"
            );
            assert!(
                value <= Duration::from_secs(120),
                "{name} = {value:?} is long enough that a dead peer still holds a spawn_blocking thread \
                 for minutes, which is what CPE-1706 exists to stop"
            );
        }
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

    // ---------------------------------------------------------------------------------------------
    // CPE-1726 — the unguarded `fs::rename` on a remote-supplied path, decided by measurement
    // ---------------------------------------------------------------------------------------------

    /// Every `std::fs` primitive that can destroy something, as the literal source text a sweep would
    /// grep for. `fs::copy` and `File::create` are in the list even though this crate has neither: the
    /// point of a guard is to catch the one that gets added later, and CPE-1719 was missed precisely
    /// because the sweep looked for `rename` while the bug was a `write`.
    const CPE_1726_DESTRUCTIVE_CALLS: &[&str] = &[
        "std::fs::rename(",
        "std::fs::write(",
        "std::fs::copy(",
        "std::fs::remove_file(",
        "std::fs::remove_dir(",
        "std::fs::remove_dir_all(",
        "std::fs::File::create(",
        "std::fs::OpenOptions",
    ];

    /// **The guard that carries CPE-1726's decision.** The `#[allow(clippy::disallowed_methods)]` on
    /// `MOVE` argues that the unguarded rename is safe *because the whole server is a `#[cfg(test)]`
    /// test double* — no shipped code, no third-party files at the destination. That is a measurement,
    /// not a category, and this test is what keeps it a measurement: if the rig (or any single
    /// destructive call in it) is ever promoted above the `#[cfg(test)]` marker, this goes red and the
    /// decision has to be re-taken rather than inherited from a comment written when it was still true.
    ///
    /// A source scan rather than a type-level trick because the property *is* textual — "no destructive
    /// `std::fs` call exists in the compiled-into-the-app half of this file" is exactly what a reviewer
    /// or a future sweep would check by hand, and this makes CI check it on every commit instead.
    /// `\r` is stripped first: the working tree is CRLF on Windows and LF on the Linux/macOS runners,
    /// and a needle containing `\n` would silently match nothing on one of them — a guard that cannot
    /// fail on half the matrix is the failure this ticket family exists to stop.
    #[test]
    fn cpe_1726_every_destructive_filesystem_call_is_confined_to_the_test_rig() {
        let src = include_str!("lib.rs").replace('\r', "");
        let marker = "\n#[cfg(test)]\nmod tests {";
        let rig_starts = src.find(marker).expect(
            "[CPE-1726] this guard finds the test rig by its exact `#[cfg(test)] / mod tests {` header \
             at column 0. It is missing, so the scan below has no boundary to test against and would \
             pass vacuously. Fix the needle to match the new header — never delete the guard.",
        );
        let mut leaked = Vec::new();
        for needle in CPE_1726_DESTRUCTIVE_CALLS {
            let mut from = 0;
            while let Some(hit) = src[from..].find(needle) {
                let at = from + hit;
                if at < rig_starts {
                    leaked.push(format!("  line {}: {needle}", src[..at].matches('\n').count() + 1));
                }
                from = at + needle.len();
            }
        }
        assert!(
            leaked.is_empty(),
            "[CPE-1726] a destructive `std::fs` call now exists in the SHIPPED half of cpe-webdav:\n{}\n\n\
             CPE-1726 left the `MOVE` rename unguarded on one measured premise: this crate ships a \
             client and its WebDAV *server* is a `#[cfg(test)]` test double, so no user's files are \
             ever at a destination it is handed. The call(s) above are outside that rig, so the premise \
             no longer holds for them and the decision must be re-taken, not inherited:\n\
             - renaming onto a slot a user or a remote named → `cpe_server::fsutil::rename_into_slot`;\n\
             - editing a file that may be a link → `cpe_server::fsutil::replace_file_contents`;\n\
             - claiming a new name → `cpe_server::fsutil::stage_exclusive`.\n\
             Moving the line back inside the rig is also a fix. Deleting this assertion is not.",
            leaked.join("\n")
        );
    }

    /// CPE-1726 acceptance: what actually happens when a **symlink** sits at the destination of the
    /// rig's `MOVE`. Both legs assert on the slot and on the victim's bytes and **never on the returned
    /// `Result`** — every bug in this family (CPE-1710/1716/1719) returned `Ok` while destroying
    /// something, so the return value is the one witness that has never been reliable.
    ///
    /// The property being pinned is the one that separates `rename` from `write`: **`fs::rename` does
    /// not follow the final component**, so it replaces the link and leaves the link's target alone,
    /// whereas the `PUT` handler's `fs::write` at the same slot would write *through* it and clobber the
    /// target. That is the whole reason the two need different fixes, and it is asserted rather than
    /// trusted.
    ///
    /// # Platform split (measured on CPE-1716, re-measured here)
    /// A **live file symlink** cannot be staged on an unprivileged Windows runner at all — a junction is
    /// directory-only and a hard link reports `is_symlink() == false` — so the live leg declares
    /// `supported_here = cfg!(unix)`: a legitimate skip on Windows, and red under CI on Unix if the
    /// runner ever loses the capability. The **dangling** leg runs everywhere, because
    /// `fsutil::make_dangling_link` has a privilege-free junction fallback.
    #[test]
    fn cpe_1726_rename_onto_a_link_never_writes_through_it() {
        let (base, root) = spawn_webdav_server_returning_root();
        let mut provider = WebdavProvider::connect(&WebdavConfig::new(&base));

        // ── Leg 1: a LIVE link at the destination, pointing at a victim with known bytes.
        let victim = root.join("victim.txt");
        std::fs::write(&victim, b"victim bytes").unwrap();
        let slot = root.join("slot.txt");
        #[cfg(windows)]
        let staged = std::os::windows::fs::symlink_file(&victim, &slot).is_ok();
        #[cfg(unix)]
        let staged = std::os::unix::fs::symlink(&victim, &slot).is_ok();
        if cpe_server::fsutil::require_staged("live_file_symlink", cfg!(unix), staged) {
            provider.write("/live-src.txt", b"source bytes").expect("seed the MOVE source");
            let r = provider.rename("/live-src.txt", "/slot.txt");
            assert_eq!(
                std::fs::read(&victim).unwrap(),
                b"victim bytes",
                "the link's TARGET must be untouched — `fs::rename` does not follow the final \
                 component, so a write-through here would mean the rig had stopped using `rename` \
                 (MOVE reported {r:?})"
            );
            assert!(
                !std::fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
                "and the link itself must be GONE, replaced by the moved file: that is the silent \
                 destruction CPE-1726 weighed and deliberately accepted for a `#[cfg(test)]` rig \
                 (MOVE reported {r:?})"
            );
            assert_eq!(std::fs::read(&slot).unwrap(), b"source bytes", "MOVE reported {r:?}");
        } else {
            cpe_server::skip_notice!(
                "[CPE-1726] SKIPPED the LIVE-link leg of cpe-webdav's MOVE test: this machine cannot \
                 create a file symlink at {} (Windows without Developer Mode or elevation; a junction \
                 is directory-only and a hard link is not a symlink — measured on CPE-1716). The \
                 DANGLING leg below runs on this runner and covers the write-through property.",
                slot.display()
            );
            let _ = std::fs::remove_file(&slot);
        }

        // ── Leg 2: a DANGLING link. Runs on every platform (junction fallback), and it is the leg that
        // proves the write-through property without needing a live target: if the rig ever wrote
        // *through* the link instead of replacing it, the link's non-existent target would spring into
        // existence. It never may.
        let dangling = root.join("dangling.txt");
        if cpe_server::fsutil::make_dangling_link(&dangling) {
            let never = root.join("dangling.txt-target-that-does-not-exist");
            provider.write("/dangling-src.txt", b"dangling source").expect("seed the MOVE source");
            let r = provider.rename("/dangling-src.txt", "/dangling.txt");
            assert!(
                !matches!(never.try_exists(), Ok(true)),
                "the dangling link's target must NEVER be created: it existing means the rig wrote \
                 THROUGH the link (the CPE-1719 shape) instead of replacing it (MOVE reported {r:?})"
            );
            // Outcome consistency, so a rig that reports success without moving anything is red rather
            // than green. Not an assertion *on* the `Result` — it is an assertion on the slot, selected
            // by what the rig claimed.
            let link_now = std::fs::symlink_metadata(&dangling).map(|m| m.file_type().is_symlink());
            if r.is_ok() {
                assert_eq!(
                    std::fs::read(&dangling).ok().as_deref(),
                    Some(&b"dangling source"[..]),
                    "MOVE reported success, so the slot must now hold the moved file's bytes; it holds \
                     something else (is_symlink = {link_now:?})"
                );
            } else {
                assert_eq!(
                    link_now.ok(),
                    Some(true),
                    "MOVE reported failure ({r:?}), so it must have left the link alone — a failed \
                     rename that still destroyed the destination is the worst of both outcomes"
                );
            }
        }
    }

    /// CPE-1726, the defect this crate had and its two siblings did not (they get `DELE`/`RMD` and
    /// `remove`/`rmdir` as separate wire verbs, so they never have to classify): the rig's `MOVE`
    /// derived its destination with `.unwrap_or_default()`, so a request whose `Destination` header was
    /// **absent or malformed** collapsed to the empty string and `root.join("")` handed `fs::rename` the
    /// **server root itself** as a live destination.
    ///
    /// Driven over the real wire with a raw request rather than through `WebdavProvider`, because the
    /// provider always sets the header — the bug is only reachable by a client that does not, which is
    /// exactly the "obeying a remote instruction" case CPE-1726 is about. Asserts on the root still
    /// being there with its seeded contents, never on the status code alone.
    #[test]
    fn cpe_1726_a_move_with_no_destination_header_never_targets_the_server_root() {
        use std::io::{Read as _, Write as _};
        let (base, root) = spawn_webdav_server_returning_root();
        let addr = base.trim_start_matches("http://").to_string();

        let mut sock = std::net::TcpStream::connect(&addr).expect("connect to the rig");
        sock.write_all(b"MOVE /readme.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
            .expect("send a MOVE with no Destination header");
        let mut resp = String::new();
        let _ = sock.read_to_string(&mut resp);

        assert!(
            root.join("readme.txt").is_file(),
            "the source must still be there: with no Destination the rig had nothing to move it to, and \
             pre-fix it moved it onto `root.join(\"\")` — the served root — instead of refusing \
             (response was {resp:?})"
        );
        assert!(
            root.join("sub").join("nested.txt").is_file(),
            "and the served tree must be intact (response was {resp:?})"
        );
        assert!(
            resp.contains("400"),
            "a MOVE with no Destination is a 400 (RFC 4918 §9.9.4), not an invented destination; got \
             {resp:?}"
        );
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
