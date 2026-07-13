//! LM Studio auto-detection (CPE-286).
//!
//! Ports the reference's `_resolve-lmstudio-url.ps1`: probe candidate LM Studio
//! endpoints (loopback on the two common ports) for a reachable `/v1/models` and report
//! the loaded model, so the LM Studio provider recipes "just work" without the user
//! typing a URL. The reachability check is abstracted behind [`Probe`] so the selection
//! logic is unit-testable; [`RealProbe`] does the actual TCP/HTTP call.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// A reachable LM Studio endpoint and (best-effort) the model it has loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LmStudio {
    pub base_url: String,
    pub model: Option<String>,
}

/// Checks whether an LM Studio endpoint is reachable, returning the loaded model id if
/// so (or `Some("")` when reachable but the model can't be determined).
pub trait Probe {
    fn probe(&self, base_url: &str) -> Option<String>;
}

/// The default candidate endpoints: loopback on LM Studio's usual ports (1234 default,
/// 1235 the common fallback). LAN-address enumeration can be layered on later.
pub fn default_candidates() -> Vec<String> {
    vec![
        "http://127.0.0.1:1234".to_string(),
        "http://127.0.0.1:1235".to_string(),
    ]
}

/// Return the first reachable endpoint among `candidates`, in order.
pub fn detect(candidates: &[String], probe: &dyn Probe) -> Option<LmStudio> {
    for url in candidates {
        if let Some(model) = probe.probe(url) {
            let model = if model.is_empty() { None } else { Some(model) };
            return Some(LmStudio { base_url: url.clone(), model });
        }
    }
    None
}

/// Convenience: detect against [`default_candidates`] with the real probe.
pub fn detect_default() -> Option<LmStudio> {
    detect(&default_candidates(), &RealProbe { timeout: Duration::from_millis(600) })
}

/// The real probe: TCP-connect to the endpoint and issue a minimal HTTP GET of
/// `/v1/models`, best-effort extracting the first model id from the body.
pub struct RealProbe {
    pub timeout: Duration,
}

impl Probe for RealProbe {
    fn probe(&self, base_url: &str) -> Option<String> {
        let (host, port) = split_host_port(base_url)?;
        let addr = format!("{host}:{port}");
        let sock = addr.to_socket_addrs_first()?;
        let mut stream = TcpStream::connect_timeout(&sock, self.timeout).ok()?;
        stream.set_read_timeout(Some(self.timeout)).ok()?;
        stream.set_write_timeout(Some(self.timeout)).ok()?;
        let req = format!("GET /v1/models HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).ok()?;
        let mut body = String::new();
        stream.read_to_string(&mut body).ok()?;
        // Reachable; pull the first "id":"..." if present (best effort).
        Some(first_json_id(&body).unwrap_or_default())
    }
}

/// Parse `http://host:port` into (host, port).
fn split_host_port(base_url: &str) -> Option<(String, u16)> {
    let rest = base_url.strip_prefix("http://").or_else(|| base_url.strip_prefix("https://"))?;
    let rest = rest.split('/').next()?; // drop any path
    let (host, port) = rest.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

/// Best-effort: the value of the first `"id":"..."` in a JSON-ish body.
fn first_json_id(body: &str) -> Option<String> {
    let idx = body.find("\"id\"")?;
    let after = &body[idx + 4..];
    let colon = after.find(':')?;
    let after = &after[colon + 1..];
    let start = after.find('"')? + 1;
    let end = after[start..].find('"')? + start;
    Some(after[start..end].to_string())
}

/// Tiny helper so we don't pull in extra deps for name resolution.
trait ToSocketAddrFirst {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr>;
}
impl ToSocketAddrFirst for String {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr> {
        use std::net::ToSocketAddrs;
        self.to_socket_addrs().ok()?.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe {
        reachable: Vec<(String, String)>, // (url, model)
    }
    impl Probe for FakeProbe {
        fn probe(&self, base_url: &str) -> Option<String> {
            self.reachable
                .iter()
                .find(|(u, _)| u == base_url)
                .map(|(_, m)| m.clone())
        }
    }

    #[test]
    fn detect_returns_the_first_reachable_with_its_model() {
        let probe = FakeProbe {
            reachable: vec![("http://127.0.0.1:1235".into(), "qwen3-coder-30b".into())],
        };
        let found = detect(&default_candidates(), &probe).unwrap();
        assert_eq!(found.base_url, "http://127.0.0.1:1235");
        assert_eq!(found.model.as_deref(), Some("qwen3-coder-30b"));
    }

    #[test]
    fn reachable_but_unknown_model_is_none() {
        let probe = FakeProbe { reachable: vec![("http://127.0.0.1:1234".into(), String::new())] };
        let found = detect(&default_candidates(), &probe).unwrap();
        assert_eq!(found.base_url, "http://127.0.0.1:1234");
        assert_eq!(found.model, None);
    }

    #[test]
    fn none_reachable_yields_none() {
        let probe = FakeProbe { reachable: vec![] };
        assert!(detect(&default_candidates(), &probe).is_none());
    }

    #[test]
    fn split_host_port_parses_urls() {
        assert_eq!(split_host_port("http://127.0.0.1:1234"), Some(("127.0.0.1".into(), 1234)));
        assert_eq!(split_host_port("http://host:1235/v1"), Some(("host".into(), 1235)));
        assert_eq!(split_host_port("not a url"), None);
    }

    #[test]
    fn first_json_id_extracts_the_model() {
        let body = r#"HTTP/1.0 200 OK

{"data":[{"id":"qwen3-coder-30b","object":"model"}]}"#;
        assert_eq!(first_json_id(body).as_deref(), Some("qwen3-coder-30b"));
        assert_eq!(first_json_id("{}"), None);
    }
}
