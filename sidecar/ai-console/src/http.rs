//! A tiny dependency-free HTTP/1.1 server for the sidecar's served UI + API (CPE-289).
//!
//! Just enough to route the launcher page and a small JSON API over an ephemeral loopback
//! port. Not a general web server: it reads one request per connection, supports
//! `Content-Length` bodies, and always sends permissive CORS headers because the host
//! embeds this page in a *sandboxed* iframe (opaque origin ⇒ requests carry `Origin:
//! null`). Loopback-only, so it isn't reachable off the machine.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use base64::Engine as _;

/// A parsed HTTP request: method, path (no query), decoded query pairs, headers, and body.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// The value of the first query parameter named `key`, if present.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// A header value by case-insensitive name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Whether this is a WebSocket upgrade request.
    pub fn is_websocket_upgrade(&self) -> bool {
        self.method == "GET"
            && self.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"))
            && self.header("sec-websocket-key").is_some()
    }
}

/// A response: HTTP status, content type, and body bytes.
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn html(body: impl Into<Vec<u8>>) -> Self {
        Self { status: 200, content_type: "text/html; charset=utf-8".into(), body: body.into() }
    }
    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self { status: 200, content_type: "application/json".into(), body: body.into() }
    }
    pub fn text(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self { status, content_type: "text/plain; charset=utf-8".into(), body: body.into() }
    }
    pub fn not_found() -> Self {
        Self::text(404, "not found")
    }
}

/// Split a request target into its path and decoded `key=value` query pairs.
pub fn parse_target(target: &str) -> (String, Vec<(String, String)>) {
    match target.split_once('?') {
        None => (target.to_string(), Vec::new()),
        Some((path, qs)) => {
            let pairs = qs
                .split('&')
                .filter(|s| !s.is_empty())
                .map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                    (percent_decode(k), percent_decode(v))
                })
                .collect();
            (path.to_string(), pairs)
        }
    }
}

/// Minimal percent-decoding (enough for query values); leaves malformed escapes as-is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Largest request body we buffer. The sidecar's API only takes small JSON payloads; this bound exists
/// so an attacker-supplied `Content-Length` can't drive an unbounded `vec![0u8; n]` whose allocation
/// failure aborts the whole process.
pub const MAX_REQUEST_BODY: usize = 16 * 1024 * 1024;

/// Read and parse one HTTP request from a buffered reader. Returns `None` on a malformed or
/// empty request.
pub fn read_request<R: BufRead>(reader: &mut R) -> Option<Request> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let (name, value) = (name.trim(), value.trim());
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name.to_string(), value.to_string()));
        }
    }

    // Refuse an over-large declared body BEFORE allocating, so a hostile Content-Length can't abort the
    // process on allocation failure (same class as the WebSocket frame cap).
    if content_length > MAX_REQUEST_BODY {
        return None;
    }
    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).ok()?;
        body = String::from_utf8_lossy(&buf).into_owned();
    }

    let (path, query) = parse_target(&target);
    Some(Request { method, path, query, headers, body })
}

/// A running UI server: the loopback port it listens on, and the accept thread.
pub struct UiServer {
    pub port: u16,
    _handle: JoinHandle<()>,
}

impl UiServer {
    /// The loopback URL the host should embed.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// Serve over an ephemeral loopback port for the process's lifetime, sharing `state`
/// across connections (one thread per connection). Normal requests go to `handler`; a
/// WebSocket upgrade is completed here and the raw stream is handed to `ws_handler`
/// (which owns it for the session).
pub fn serve<S>(
    state: Arc<S>,
    handler: fn(&S, &Request) -> Response,
    ws_handler: fn(&S, &Request, TcpStream),
) -> Result<UiServer, String>
where
    S: Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let state = Arc::clone(&state);
            thread::spawn(move || {
                let mut reader = BufReader::new(match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => return,
                });
                let mut out = stream;
                match read_request(&mut reader) {
                    Some(req) if req.method == "OPTIONS" => {
                        let _ = write_response(&mut out, &Response::text(204, ""));
                    }
                    Some(req) if req.is_websocket_upgrade() => {
                        let accept = ws_accept_key(req.header("sec-websocket-key").unwrap_or(""));
                        let hs = format!(
                            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
                             Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
                        );
                        if out.write_all(hs.as_bytes()).is_ok() && out.flush().is_ok() {
                            ws_handler(&state, &req, out); // hands off the upgraded stream
                        }
                    }
                    Some(req) => {
                        let _ = write_response(&mut out, &handler(&state, &req));
                    }
                    None => {}
                }
            });
        }
    });
    Ok(UiServer { port, _handle: handle })
}

// ---- WebSocket (RFC 6455) — just what a terminal needs (CPE-334) --------------------

/// Opcodes we handle.
pub mod ws_op {
    pub const TEXT: u8 = 0x1;
    pub const BINARY: u8 = 0x2;
    pub const CLOSE: u8 = 0x8;
    pub const PING: u8 = 0x9;
    pub const PONG: u8 = 0xA;
}

/// `Sec-WebSocket-Accept` = base64(sha1(client-key + magic GUID)).
pub fn ws_accept_key(client_key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(client_key.as_bytes());
    h.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// Write one server frame (FIN=1, unmasked).
pub fn ws_write_frame(w: &mut impl Write, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut head = Vec::with_capacity(10);
    head.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        head.push(len as u8);
    } else if len <= 0xFFFF {
        head.push(126);
        head.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        head.push(127);
        head.extend_from_slice(&(len as u64).to_be_bytes());
    }
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// Largest inbound WebSocket payload we accept in one frame. A terminal only carries keystrokes,
/// resizes, and pastes, so this is generous; the point is that an attacker-declared 64-bit length can't
/// drive an unbounded `vec![0u8; len]` that aborts the process on allocation failure.
pub const MAX_WS_PAYLOAD: usize = 64 * 1024 * 1024;

/// One decoded inbound frame.
pub struct WsFrame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// Read one client frame (unmasking it). `Ok(None)` on end-of-stream.
pub fn ws_read_frame(r: &mut impl Read) -> io::Result<Option<WsFrame>> {
    let mut h = [0u8; 2];
    if fill(r, &mut h)? {
        return Ok(None);
    }
    let fin = h[0] & 0x80 != 0;
    let opcode = h[0] & 0x0F;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7F) as usize;
    if len == 126 {
        let mut e = [0u8; 2];
        if fill(r, &mut e)? {
            return Ok(None);
        }
        len = u16::from_be_bytes(e) as usize;
    } else if len == 127 {
        let mut e = [0u8; 8];
        if fill(r, &mut e)? {
            return Ok(None);
        }
        len = u64::from_be_bytes(e) as usize;
    }
    // Reject an over-large declared length BEFORE allocating: an attacker-controlled 64-bit length
    // would otherwise drive a giant `vec![0u8; len]` whose allocation failure aborts the whole sidecar.
    if len > MAX_WS_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "websocket frame exceeds maximum payload size",
        ));
    }
    let mut mask = [0u8; 4];
    if masked && fill(r, &mut mask)? {
        return Ok(None);
    }
    let mut payload = vec![0u8; len];
    if len > 0 && fill(r, &mut payload)? {
        return Ok(None);
    }
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i & 3];
        }
    }
    Ok(Some(WsFrame { fin, opcode, payload }))
}

/// Read exactly `buf.len()` bytes; `Ok(true)` if EOF was hit first.
fn fill(r: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => return Ok(true),
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

/// Write a response with permissive CORS headers (the page is a sandboxed, opaque-origin
/// iframe, so its requests carry `Origin: null`).
pub fn write_response<W: Write>(out: &mut W, resp: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} OK\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        resp.status,
        resp.content_type,
        resp.body.len()
    );
    out.write_all(head.as_bytes())?;
    out.write_all(&resp.body)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_target_path_and_query() {
        let (path, q) = parse_target("/api/session/abc/output?since=42&x=a%20b");
        assert_eq!(path, "/api/session/abc/output");
        assert_eq!(q, vec![("since".to_string(), "42".to_string()), ("x".into(), "a b".into())]);
    }

    #[test]
    fn reads_a_get_request() {
        let raw = "GET /api/catalog?x=1 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let mut r = Cursor::new(raw.as_bytes());
        let req = read_request(&mut r).unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/api/catalog");
        assert_eq!(req.query("x"), Some("1"));
        assert!(req.body.is_empty());
    }

    #[test]
    fn reads_a_post_with_body() {
        let raw = "POST /api/launch HTTP/1.1\r\nContent-Length: 13\r\n\r\n{\"agent\":\"x\"}";
        let mut r = Cursor::new(raw.as_bytes());
        let req = read_request(&mut r).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/launch");
        assert_eq!(req.body, "{\"agent\":\"x\"}");
    }

    #[test]
    fn serve_routes_a_real_request() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpStream;

        fn handler(_state: &(), req: &Request) -> Response {
            Response::text(200, format!("path={} q={:?}", req.path, req.query("x")))
        }
        fn ws_noop(_state: &(), _req: &Request, _stream: TcpStream) {}
        let server = serve(Arc::new(()), handler, ws_noop).unwrap();
        // Do the exchange with a few retries: the whole test suite runs in parallel and
        // spawns many subprocesses, so a single loopback round-trip can occasionally be
        // starved. Retrying keeps this deterministic without a serial-test dependency.
        let mut resp = String::new();
        for attempt in 0..5 {
            resp.clear();
            let r = (|| -> std::io::Result<()> {
                let mut stream = TcpStream::connect(("127.0.0.1", server.port))?;
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                stream.write_all(b"GET /hello?x=7 HTTP/1.1\r\nHost: localhost\r\n\r\n")?;
                stream.read_to_string(&mut resp)?;
                Ok(())
            })();
            if r.is_ok() && resp.contains("200 OK") {
                break;
            }
            assert!(attempt < 4, "server never responded: {r:?} resp={resp}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(resp.contains("200 OK"), "resp: {resp}");
        assert!(resp.contains("Access-Control-Allow-Origin: *"));
        assert!(resp.contains("path=/hello q=Some(\"7\")"), "resp: {resp}");
    }

    #[test]
    fn ws_accept_matches_rfc6455_example() {
        // The canonical example from RFC 6455 §1.3.
        assert_eq!(ws_accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn ws_write_then_read_masked_roundtrip() {
        // Server frames are unmasked, FIN + opcode + length + payload.
        let mut buf = Vec::new();
        ws_write_frame(&mut buf, ws_op::BINARY, b"hello").unwrap();
        assert_eq!(buf[0], 0x80 | ws_op::BINARY);
        assert_eq!(buf[1], 5);
        assert_eq!(&buf[2..], b"hello");

        // Client frames are masked — build one and confirm we unmask it.
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let payload = b"type me";
        let mut frame = vec![0x80 | ws_op::TEXT, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i & 3]);
        }
        let mut cur = std::io::Cursor::new(frame);
        let f = ws_read_frame(&mut cur).unwrap().unwrap();
        assert!(f.fin);
        assert_eq!(f.opcode, ws_op::TEXT);
        assert_eq!(f.payload, payload);
    }

    #[test]
    fn read_request_refuses_an_over_large_content_length() {
        // A hostile Content-Length must be rejected before allocating its body (no process-aborting
        // giant allocation). Only headers are supplied — no gigantic body ever arrives.
        let raw = "POST /api HTTP/1.1\r\nContent-Length: 999999999999\r\n\r\n";
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw.as_bytes().to_vec()));
        assert!(read_request(&mut reader).is_none(), "over-large Content-Length must be refused");
        // A normal small body still parses.
        let ok = "POST /api HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(ok.as_bytes().to_vec()));
        assert_eq!(read_request(&mut reader).unwrap().body, "hello");
    }

    #[test]
    fn ws_read_frame_rejects_an_over_large_declared_length() {
        // A 64-bit length header declaring a huge payload must be refused BEFORE allocating, so a
        // single crafted frame can't drive an unbounded allocation that aborts the sidecar. We only
        // supply the 10-byte header (opcode + 127 + 8-byte length) — a real giant body never arrives.
        let mut frame = vec![0x80 | ws_op::BINARY, 127];
        frame.extend_from_slice(&(u64::MAX).to_be_bytes()); // absurd declared length
        let mut cur = std::io::Cursor::new(frame);
        match ws_read_frame(&mut cur) {
            Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
            Ok(_) => panic!("an over-large frame length must be rejected, not allocated"),
        }
    }

    #[test]
    fn writes_cors_headers() {
        let resp = Response::json(b"{}".to_vec());
        let mut buf = Vec::new();
        write_response(&mut buf, &resp).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("Access-Control-Allow-Origin: *"));
        assert!(s.contains("application/json"));
    }
}
