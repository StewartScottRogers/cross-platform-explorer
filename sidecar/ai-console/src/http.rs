//! A tiny dependency-free HTTP/1.1 server for the sidecar's served UI + API (CPE-289).
//!
//! Just enough to route the launcher page and a small JSON API over an ephemeral loopback
//! port. Not a general web server: it reads one request per connection, supports
//! `Content-Length` bodies, and always sends permissive CORS headers because the host
//! embeds this page in a *sandboxed* iframe (opaque origin ⇒ requests carry `Origin:
//! null`). Loopback-only, so it isn't reachable off the machine.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// A parsed HTTP request: method, path (no query), decoded query pairs, and body.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// The value of the first query parameter named `key`, if present.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
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
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).ok()?;
        body = String::from_utf8_lossy(&buf).into_owned();
    }

    let (path, query) = parse_target(&target);
    Some(Request { method, path, query, body })
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

/// Serve `handler` over an ephemeral loopback port for the process's lifetime, sharing
/// `state` across connections (one thread per connection).
pub fn serve<S>(state: Arc<S>, handler: fn(&S, &Request) -> Response) -> Result<UiServer, String>
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
                let resp = match read_request(&mut reader) {
                    Some(req) if req.method == "OPTIONS" => Response::text(204, ""),
                    Some(req) => handler(&state, &req),
                    None => return,
                };
                let _ = write_response(&mut out, &resp);
            });
        }
    });
    Ok(UiServer { port, _handle: handle })
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
        let server = serve(Arc::new(()), handler).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        // Bound the read so a pathological hang fails fast instead of stalling the suite.
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        stream.write_all(b"GET /hello?x=7 HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("200 OK"), "resp: {resp}");
        assert!(resp.contains("Access-Control-Allow-Origin: *"));
        assert!(resp.contains("path=/hello q=Some(\"7\")"), "resp: {resp}");
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
