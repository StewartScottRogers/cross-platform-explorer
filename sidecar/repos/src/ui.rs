//! The Repositories sidecar's own served UI (CPE-432, AC2).
//!
//! Per ADR 0001 each sidecar serves its **own** UI; the host embeds it in a frame. This mirrors the
//! AI Console's `ui.rs` (CPE-271): a minimal, dependency-free HTTP server that serves one HTML page
//! on an ephemeral loopback port. The sidecar starts it after the handshake and announces the URL to
//! the host via a `Status` event (`ui:<url>`), which the host then points an iframe pane at.
//! Loopback-only, so it isn't reachable off the machine.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// A running UI server: the loopback port it listens on, and the serving thread.
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

/// Serve `html` on an ephemeral loopback port for the process's lifetime.
pub fn serve(html: String) -> Result<UiServer, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = std::thread::spawn(move || {
        let body = html.into_bytes();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            // Drain the request line/headers (best effort) and reply with the page.
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    Ok(UiServer { port, _handle: handle })
}

/// The Repositories placeholder UI page. Replaced by the real browse/mirror UI later (CPE-435); for
/// now it proves the sidecar serves a page the host can embed.
pub fn placeholder_ui() -> String {
    r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Repositories</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 14px system-ui, sans-serif; margin: 0; display: grid; place-items: center;
         height: 100vh; background: Canvas; color: CanvasText; }
  .card { text-align: center; opacity: 0.85; }
  h1 { font-size: 18px; margin: 0 0 6px; }
</style></head>
<body><div class="card"><h1>Repositories</h1><p>Sidecar UI mounted.</p></div></body>
</html>
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn serves_the_page_over_a_loopback_port() {
        let server = serve("<h1>hello-repos-ui</h1>".into()).unwrap();
        assert!(server.url().starts_with("http://127.0.0.1:"));

        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();

        assert!(resp.contains("200 OK"), "resp: {resp}");
        assert!(resp.contains("text/html"));
        assert!(resp.contains("hello-repos-ui"));
    }

    #[test]
    fn placeholder_is_valid_html() {
        let html = placeholder_ui();
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("Repositories"));
    }
}
