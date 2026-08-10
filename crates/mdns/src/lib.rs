//! mDNS/DNS-SD LAN discovery (CPE-1523, epic CPE-1517) — the cross-platform complement to the
//! Windows-native WNet discovery (`discover_network_windows`, CPE-1519, in `src-tauri`). mDNS is the
//! ONLY discovery path on macOS/Linux, and on Windows it's a superset: it surfaces sftp/webdav/ftp/nfs
//! hosts WNet's SMB-only neighborhood never sees.
//!
//! Split like `cpe_server::net_share` (the WNet half's pure/impure split):
//! - [`map_mdns_service`] — pure, unit-tested: one resolved mDNS/DNS-SD service record → an optional
//!   [`NetShare`] row (`kind: "discovered"`, uniform with the WNet tier).
//! - [`discover`] — impure: drives an `mdns-sd` [`ServiceDaemon`], browsing the mapped service types and
//!   collecting resolved records until a timeout, mapping + deduping into the final `Vec<NetShare>`.

use cpe_server::net_share::{dedup_key, NetShare, NetworkShare, ShareProtocol};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// The DNS-SD service types this crate browses for, in the exact form `ServiceDaemon::browse` and
/// `ServiceInfo::get_type()` use (fully qualified, trailing dot) — mirrors the table in CPE-1523.
/// `_afpovertcp._tcp.local.` / `_http._tcp.local.` / `_qnap._tcp.local.` are deliberately NOT browsed —
/// AFP has no provider in this app (SMB/sftp/webdav/ftp/nfs only), and a bare HTTP or vendor-management
/// service isn't a navigable share; [`map_mdns_service`] also maps all three to `None` defensively, so a
/// stray resolve for one (e.g. a future caller widening the browse list) still can't produce a row.
pub const SERVICE_TYPES: [&str; 6] = [
    "_smb._tcp.local.",
    "_sftp-ssh._tcp.local.",
    "_webdav._tcp.local.",
    "_webdavs._tcp.local.",
    "_ftp._tcp.local.",
    "_nfs._tcp.local.",
];

/// TXT-record keys checked (in order) for a service's human-friendly display name. There's no single
/// RFC-mandated "display name" TXT key shared across smb/sftp/webdav/ftp/nfs advertisers, so this is a
/// pragmatic best-effort guess — a friendly name is a nice-to-have; [`map_mdns_service`] always falls
/// back to the hostname when none of these are present, so a row is never left unnamed.
const TXT_NAME_KEYS: [&str; 2] = ["name", "friendly_name"];

/// Map one DNS-SD service type to its `(scheme, default_port)`, or `None` for anything not in the
/// CPE-1523 table (including `_afpovertcp._tcp.local.` / `_http._tcp.local.` / `_qnap._tcp.local.`).
fn scheme_and_default_port(service_type: &str) -> Option<(ShareProtocol, u16)> {
    match service_type {
        "_smb._tcp.local." => Some((ShareProtocol::Smb, 445)),
        "_sftp-ssh._tcp.local." => Some((ShareProtocol::Sftp, 22)),
        "_webdav._tcp.local." => Some((ShareProtocol::Webdav, 80)),
        "_webdavs._tcp.local." => Some((ShareProtocol::Davs, 443)),
        "_ftp._tcp.local." => Some((ShareProtocol::Ftp, 21)),
        "_nfs._tcp.local." => Some((ShareProtocol::Nfs, 2049)),
        _ => None,
    }
}

/// Map one resolved mDNS/DNS-SD service record to a [`NetShare`] row (`kind: "discovered"`, uniform
/// with the WNet tier's `map_discovered_share`). Pure — no I/O; `discover` below is the only caller that
/// touches the network.
///
/// - `service_type` — the fully-qualified DNS-SD type (e.g. `"_smb._tcp.local."`); anything outside the
///   6-entry CPE-1523 table (including `_afpovertcp._tcp.local.`, `_http._tcp.local.`,
///   `_qnap._tcp.local.`) maps to `None` — AFP has no provider here, and a bare HTTP/vendor-management
///   service isn't a navigable share.
/// - `hostname` — the advertised target host (a trailing `.` from the DNS record, if present, is
///   trimmed); an empty/whitespace-only hostname maps to `None` (nothing navigable to build a row from).
/// - `port` — the resolved SRV port. `0` (a malformed/unresolved record) falls back to the scheme's
///   standard port rather than producing a broken `:0` URL.
/// - `txt_name` — a TXT-record friendly name, when the caller found one; falls back to `hostname` when
///   absent/blank, matching `map_discovered_share`'s comment → remote_name fallback.
///
/// `path` is built via [`NetworkShare::to_url`] — the SAME address-rendering the "Add a connection" /
/// "Add a network location" flows already use — with the port folded into the `host` field (omitted
/// when it's the scheme's default port, matching `Connection::location()`'s convention of hiding a
/// profile's default port).
pub fn map_mdns_service(service_type: &str, hostname: &str, port: u16, txt_name: Option<&str>) -> Option<NetShare> {
    let (protocol, default_port) = scheme_and_default_port(service_type)?;

    let hostname = hostname.trim().trim_end_matches('.');
    if hostname.is_empty() {
        return None;
    }

    let effective_port = if port == 0 { default_port } else { port };
    let host =
        if effective_port == default_port { hostname.to_string() } else { format!("{hostname}:{effective_port}") };

    let path = NetworkShare { protocol, host, share: String::new(), path: String::new() }.to_url();

    let name = txt_name.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(hostname).to_string();

    Some(NetShare { name, path, kind: "discovered".to_string() })
}

/// Best-effort TXT friendly-name lookup — tries [`TXT_NAME_KEYS`] in order, returning the first present,
/// non-blank value.
fn txt_friendly_name(info: &mdns_sd::ResolvedService) -> Option<String> {
    TXT_NAME_KEYS
        .iter()
        .find_map(|key| info.get_property_val_str(key))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Discover LAN shares over mDNS/DNS-SD (CPE-1523): browses the 6 [`SERVICE_TYPES`] and collects
/// resolved records for up to `timeout` (the app calls this with ~6s, matching the WNet tier's
/// `DISCOVERY_TIMEOUT`), mapping each via [`map_mdns_service`] and deduping by
/// [`cpe_server::net_share::dedup_key`] — the same key `combine_shares`/`flatten_discovered` use, so a
/// row here collapses with an identically-addressed row from either of those tiers once the frontend
/// merges all three (CPE-1523's TS `mergeDiscovered`).
///
/// Skip-on-error throughout, never panics: a daemon that fails to start, a service type that fails to
/// browse, or any per-event glitch just yields fewer rows — mirroring `list_dir`'s skip-on-error
/// guarantee and `discover_network_windows_impl`'s "give up quietly on the network" behavior. Runs on
/// whatever thread calls it; the `src-tauri` command wraps this in `spawn_blocking` since this loop
/// blocks the calling thread for the full `timeout` (short of an early daemon-start failure).
pub fn discover(timeout: Duration) -> Vec<NetShare> {
    let Ok(daemon) = ServiceDaemon::new() else {
        return Vec::new();
    };

    let receivers: Vec<(&str, mdns_sd::Receiver<ServiceEvent>)> = SERVICE_TYPES
        .iter()
        .filter_map(|&service_type| daemon.browse(service_type).ok().map(|rx| (service_type, rx)))
        .collect();

    let mut seen = HashSet::new();
    let mut out = Vec::new();

    if !receivers.is_empty() {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let mut received_any = false;
            for (service_type, receiver) in &receivers {
                while let Ok(event) = receiver.try_recv() {
                    received_any = true;
                    if let ServiceEvent::ServiceResolved(info) = event {
                        let txt_name = txt_friendly_name(&info);
                        if let Some(share) =
                            map_mdns_service(service_type, info.get_hostname(), info.get_port(), txt_name.as_deref())
                        {
                            if seen.insert(dedup_key(&share.path)) {
                                out.push(share);
                            }
                        }
                    }
                }
            }
            if !received_any {
                // Nothing new on any channel this pass — a short sleep avoids a hot busy-loop while
                // still round-robining across all 6 receivers instead of parking (`recv_timeout`) on
                // just one for the whole deadline.
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    // Best-effort: an error here just means the daemon's background thread outlives this call — no
    // rows are lost, and (mirroring `discover_network_windows_impl`'s abandoned-thread comment) it's
    // fine for the daemon to keep running in the background rather than being force-killed.
    let _ = daemon.shutdown();

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── map_mdns_service: the 6 mapped types ────────────────────────────────────────────────────────

    #[test]
    fn maps_smb_at_its_default_port() {
        let share = map_mdns_service("_smb._tcp.local.", "qnap.local.", 445, None).unwrap();
        assert_eq!(share.path, "smb://qnap.local", "default port omitted from the URL");
        assert_eq!(share.name, "qnap.local", "no TXT name → hostname fallback");
        assert_eq!(share.kind, "discovered");
    }

    #[test]
    fn maps_sftp_at_its_default_port() {
        let share = map_mdns_service("_sftp-ssh._tcp.local.", "nas.local.", 22, None).unwrap();
        assert_eq!(share.path, "sftp://nas.local");
    }

    #[test]
    fn maps_webdav_at_its_default_port() {
        let share = map_mdns_service("_webdav._tcp.local.", "nas.local.", 80, None).unwrap();
        assert_eq!(share.path, "webdav://nas.local");
    }

    #[test]
    fn maps_webdavs_to_the_davs_scheme_at_its_default_port() {
        let share = map_mdns_service("_webdavs._tcp.local.", "nas.local.", 443, None).unwrap();
        assert_eq!(share.path, "davs://nas.local");
    }

    #[test]
    fn maps_ftp_at_its_default_port() {
        let share = map_mdns_service("_ftp._tcp.local.", "nas.local.", 21, None).unwrap();
        assert_eq!(share.path, "ftp://nas.local");
    }

    #[test]
    fn maps_nfs_at_its_default_port() {
        let share = map_mdns_service("_nfs._tcp.local.", "nas.local.", 2049, None).unwrap();
        assert_eq!(share.path, "nfs://nas.local");
    }

    // ── the 3 excluded types → None ─────────────────────────────────────────────────────────────────

    #[test]
    fn afp_is_not_mapped() {
        assert!(map_mdns_service("_afpovertcp._tcp.local.", "nas.local.", 548, None).is_none());
    }

    #[test]
    fn bare_http_is_not_mapped() {
        assert!(map_mdns_service("_http._tcp.local.", "nas.local.", 80, None).is_none());
    }

    #[test]
    fn qnap_vendor_service_is_not_mapped() {
        assert!(map_mdns_service("_qnap._tcp.local.", "nas.local.", 8080, None).is_none());
    }

    #[test]
    fn an_unrecognized_service_type_is_not_mapped() {
        assert!(map_mdns_service("_ipp._tcp.local.", "printer.local.", 631, None).is_none());
    }

    // ── name fallback ────────────────────────────────────────────────────────────────────────────────

    #[test]
    fn prefers_the_txt_friendly_name_over_the_hostname() {
        let share = map_mdns_service("_smb._tcp.local.", "qnap.local.", 445, Some("Office QNAP")).unwrap();
        assert_eq!(share.name, "Office QNAP");
        assert_eq!(share.path, "smb://qnap.local", "the URL still uses the hostname, not the friendly name");
    }

    #[test]
    fn a_blank_txt_name_falls_back_to_the_hostname_like_no_name_at_all() {
        let share = map_mdns_service("_smb._tcp.local.", "qnap.local.", 445, Some("   ")).unwrap();
        assert_eq!(share.name, "qnap.local");
    }

    // ── port defaulting ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_non_default_port_is_folded_into_the_host() {
        let share = map_mdns_service("_sftp-ssh._tcp.local.", "nas.local.", 2222, None).unwrap();
        assert_eq!(share.path, "sftp://nas.local:2222");
    }

    #[test]
    fn a_zero_port_falls_back_to_the_schemes_default_port() {
        // A malformed/unresolved SRV record reporting port 0 must never produce a broken `:0` URL.
        let share = map_mdns_service("_webdav._tcp.local.", "nas.local.", 0, None).unwrap();
        assert_eq!(share.path, "webdav://nas.local");
    }

    // ── hostname edge cases ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn trims_the_trailing_dns_dot_from_the_hostname() {
        let share = map_mdns_service("_smb._tcp.local.", "qnap.local.", 445, None).unwrap();
        assert!(!share.path.contains("local.."), "no doubled/dangling dot from the trailing-dot trim");
        assert_eq!(share.path, "smb://qnap.local");
    }

    #[test]
    fn an_empty_or_whitespace_hostname_maps_to_none() {
        assert!(map_mdns_service("_smb._tcp.local.", "", 445, None).is_none());
        assert!(map_mdns_service("_smb._tcp.local.", "   ", 445, None).is_none());
        assert!(map_mdns_service("_smb._tcp.local.", ".", 445, None).is_none());
    }

    // ── discover(): never panics with no daemon reachable in a locked-down/offline test sandbox ───────

    #[test]
    fn discover_never_panics_and_returns_promptly_with_a_short_timeout() {
        // CI sandboxes may have no multicast-capable network interface at all; `discover` must degrade
        // to an empty Vec rather than panicking or hanging past the requested timeout.
        let shares = discover(Duration::from_millis(200));
        // No assertion on contents (environment-dependent) — the guarantee under test is "returns".
        let _ = shares;
    }
}
