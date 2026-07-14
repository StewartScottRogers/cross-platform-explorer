//! LM Studio auto-detection (CPE-286).
//!
//! Ports the reference's `_resolve-lmstudio-url.ps1`: probe candidate LM Studio
//! endpoints (loopback on the two common ports) for a reachable `/v1/models` and report
//! the loaded model, so the LM Studio provider recipes "just work" without the user
//! typing a URL. The reachability check is abstracted behind [`Probe`] so the selection
//! logic is unit-testable; [`RealProbe`] does the actual TCP/HTTP call.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream, UdpSocket};
use std::time::Duration;

/// The provider id that triggers LM Studio auto-detection at launch (CPE-330). An agent
/// manifest lists this in `providers` and carries a matching recipe that injects the
/// detected endpoint via `{base_url}`.
pub const PROVIDER_ID: &str = "lmstudio-local";

/// LM Studio's usual OpenAI-compatible server ports: 1234 default, 1235 the common
/// fallback when 1234 is taken.
const PORTS: &[u16] = &[1234, 1235];

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

/// The `http://host:port` endpoints to probe for one host, on LM Studio's usual ports.
fn ports_for(host: &str) -> Vec<String> {
    PORTS.iter().map(|p| format!("http://{host}:{p}")).collect()
}

/// The default candidate endpoints: loopback on LM Studio's usual ports (1234 default,
/// 1235 the common fallback).
pub fn default_candidates() -> Vec<String> {
    ports_for("127.0.0.1")
}

/// Candidate endpoints to probe, in priority order: loopback first (the overwhelmingly
/// common case), then this host's LAN IPv4 if known — so LM Studio bound only to the LAN
/// interface (`Serve on Local Network`) is still found without any manual URL entry. Pure
/// so the ordering is unit-testable; the live LAN address is passed in.
pub fn candidates(lan: Option<Ipv4Addr>) -> Vec<String> {
    let mut cands = default_candidates();
    if let Some(ip) = lan {
        cands.extend(ports_for(&ip.to_string()));
    }
    cands
}

/// Best-effort primary LAN IPv4 of this host. Uses a *connected* UDP socket to learn the
/// outbound interface address — no packets are actually sent (UDP connect only sets the
/// default route), so it's fast and side-effect free. `None` if it can't be determined.
pub fn lan_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => Some(v4),
        _ => None,
    }
}

/// Merge an auto-detected endpoint into the caller-supplied `base_url`/`model` for a
/// launch (CPE-330). A value the caller pinned always wins; detection fills the gaps —
/// the reachable URL and, crucially, the *actually-loaded* model — so selecting
/// `lmstudio-local` "just works" with no manual URL entry. When nothing is detected the
/// supplied values pass through unchanged (the recipe's `base_url` default then applies).
pub fn resolve_launch(
    base_url: Option<String>,
    model: Option<String>,
    detected: Option<LmStudio>,
) -> (Option<String>, Option<String>) {
    match detected {
        Some(d) => (base_url.or(Some(d.base_url)), model.or(d.model)),
        None => (base_url, model),
    }
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

/// Convenience: detect against loopback + this host's LAN IPv4 with the real probe.
pub fn detect_default() -> Option<LmStudio> {
    detect(&candidates(lan_ipv4()), &RealProbe { timeout: Duration::from_millis(600) })
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
    fn ports_for_covers_both_default_ports() {
        assert_eq!(ports_for("127.0.0.1"), vec!["http://127.0.0.1:1234", "http://127.0.0.1:1235"]);
    }

    #[test]
    fn candidates_put_loopback_first_then_lan() {
        // No LAN address: loopback only.
        assert_eq!(candidates(None), default_candidates());
        // With a LAN address: loopback is still probed first, LAN appended.
        let lan = "192.168.1.50".parse().unwrap();
        assert_eq!(
            candidates(Some(lan)),
            vec![
                "http://127.0.0.1:1234",
                "http://127.0.0.1:1235",
                "http://192.168.1.50:1234",
                "http://192.168.1.50:1235",
            ]
        );
    }

    #[test]
    fn resolve_launch_fills_url_and_loaded_model_from_detection() {
        let detected = LmStudio {
            base_url: "http://127.0.0.1:1234".into(),
            model: Some("qwen3-coder-30b".into()),
        };
        let (url, model) = resolve_launch(None, None, Some(detected));
        assert_eq!(url.as_deref(), Some("http://127.0.0.1:1234"));
        assert_eq!(model.as_deref(), Some("qwen3-coder-30b"));
    }

    #[test]
    fn resolve_launch_lets_a_pinned_value_win_over_detection() {
        let detected = LmStudio {
            base_url: "http://127.0.0.1:1234".into(),
            model: Some("loaded-model".into()),
        };
        let (url, model) = resolve_launch(
            Some("http://10.0.0.9:1234".into()),
            Some("my-model".into()),
            Some(detected),
        );
        assert_eq!(url.as_deref(), Some("http://10.0.0.9:1234"));
        assert_eq!(model.as_deref(), Some("my-model"));
    }

    #[test]
    fn resolve_launch_passes_supplied_through_when_nothing_detected() {
        let (url, model) = resolve_launch(Some("http://x:1234".into()), None, None);
        assert_eq!(url.as_deref(), Some("http://x:1234"));
        assert_eq!(model, None);
    }

    #[test]
    fn first_json_id_extracts_the_model() {
        let body = r#"HTTP/1.0 200 OK

{"data":[{"id":"qwen3-coder-30b","object":"model"}]}"#;
        assert_eq!(first_json_id(body).as_deref(), Some("qwen3-coder-30b"));
        assert_eq!(first_json_id("{}"), None);
    }
}
