//! The Agent Board sidecar's own served UI (CPE-851, epic CPE-850).
//!
//! Per ADR 0001 each sidecar serves its **own** UI; the host frames it. A minimal, dependency-free HTTP
//! server (mirroring `sidecar/repos` and the AI Console) serves one HTML page on an ephemeral loopback
//! port; the sidecar announces the URL to the host via a `ui:<url>` Status event after the handshake.
//! Loopback-only, so it isn't reachable off the machine. The page is a board-branded placeholder here;
//! the real Kanban over `Tickets/` lands in CPE-852.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// A running UI server: the loopback port it listens on, and the serving thread.
pub struct UiServer {
    pub port: u16,
    _handle: JoinHandle<()>,
}

impl UiServer {
    /// The loopback URL the host should frame.
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
            // Drain the request (best effort) and reply with the page.
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

/// The Agent Board placeholder page. Proves the sidecar serves a framable UI; the real Kanban board over
/// `Tickets/` replaces it in CPE-852.
pub fn board_ui() -> String {
    r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Agent Board</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 14px system-ui, sans-serif; margin: 0; display: grid; place-items: center;
         height: 100vh; background: Canvas; color: CanvasText; }
  .card { text-align: center; opacity: 0.9; }
  h1 { font-size: 18px; margin: 0 0 6px; }
  p { margin: 2px 0; opacity: 0.75; }
</style></head>
<body>
  <div class="card">
    <h1>Agent Board</h1>
    <p>Sidecar UI mounted — running out of process.</p>
    <p>Kanban over your <code>Tickets/</code> coming next.</p>
  </div>
</body>
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
        let server = serve("<h1>hello-agent-board</h1>".into()).unwrap();
        assert!(server.url().starts_with("http://127.0.0.1:"));

        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();

        assert!(resp.contains("200 OK"), "resp: {resp}");
        assert!(resp.contains("text/html"));
        assert!(resp.contains("hello-agent-board"));
    }

    #[test]
    fn board_ui_is_valid_html() {
        let html = board_ui();
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("Agent Board"));
    }
}
