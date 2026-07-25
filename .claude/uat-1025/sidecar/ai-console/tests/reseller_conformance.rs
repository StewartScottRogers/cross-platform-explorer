//! CPE-480: reseller conformance kit — a data-driven check over EVERY bundled `resellers/*.json`, so
//! a malformed or internally-inconsistent reseller can't ship. Runs in CI with the rest of the tests.
//!
//! The host's `models_egress` allow-list is the authoritative SSRF boundary and lives in `src-tauri`
//! (tested there); this kit validates the *sidecar* side — that each manifest parses, derives a
//! consistent launch descriptor when it declares one, and that its declared hosts actually cover the
//! URLs it will use.

use std::path::PathBuf;

use ai_console::model_catalog::ResellerRegistry;

fn resellers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resellers")
}

/// The host part of an `https://host/...` URL, lowercased.
fn host_of(url: &str) -> Option<String> {
    url.strip_prefix("https://").map(|rest| rest.split('/').next().unwrap_or("").to_ascii_lowercase())
}

#[test]
fn every_bundled_reseller_manifest_is_valid_and_self_consistent() {
    let reg = ResellerRegistry::load_from_dirs(&[resellers_dir()]);
    // 1. Nothing was skipped as malformed.
    assert!(reg.warnings().is_empty(), "malformed reseller manifests: {:?}", reg.warnings());
    // 2. There is a healthy number of resellers (guards an empty/mis-pathed dir).
    assert!(reg.len() >= 15, "expected the full reseller set, got {}", reg.len());

    for m in reg.all() {
        let id = &m.id;
        // 3. Every reseller declares at least one egress host, and its model-list endpoint's host is
        //    among them (so the host allow-list, keyed on the same hosts, actually covers the fetch).
        assert!(!m.api_hosts.is_empty(), "reseller '{id}' has no api_hosts");
        if let Some(mh) = host_of(&m.models_endpoint) {
            assert!(
                m.api_hosts.iter().any(|h| h.eq_ignore_ascii_case(&mh)),
                "reseller '{id}' models_endpoint host '{mh}' is not in api_hosts {:?}",
                m.api_hosts
            );
        }
        // 4. A launch-capable reseller (protocol + launch_base_url) must derive a descriptor whose
        //    base URL is https and whose host is covered by api_hosts.
        match m.descriptor() {
            Some(d) => {
                assert!(d.base_url.starts_with("https://"), "reseller '{id}' launch base must be https");
                assert!(matches!(d.protocol.as_str(), "anthropic" | "openai"), "reseller '{id}' bad protocol");
                if let Some(bh) = host_of(&d.base_url) {
                    assert!(
                        m.api_hosts.iter().any(|h| h.eq_ignore_ascii_case(&bh)),
                        "reseller '{id}' launch_base_url host '{bh}' is not in api_hosts {:?}",
                        m.api_hosts
                    );
                }
            }
            None => {
                // Model-list-only is allowed, but then it must NOT half-declare launch fields.
                assert!(
                    m.protocol.is_none() && m.launch_base_url.is_none(),
                    "reseller '{id}' half-declares launch fields (protocol XOR launch_base_url)"
                );
            }
        }
    }
}

#[test]
fn a_reseller_added_as_pure_data_is_launch_capable_with_no_code_change() {
    // The extensibility claim (CPE-467): drop a valid manifest into a dir, and it becomes a launch
    // descriptor with zero code — no registration, no allow-list edit needed on the sidecar side.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("acme.json"),
        r#"{"schema_version":1,"id":"acme","name":"Acme AI",
           "models_endpoint":"https://api.acme.example/v1/models","auth":"bearer",
           "api_hosts":["api.acme.example"],"normalizer":"openai",
           "protocol":"openai","launch_base_url":"https://api.acme.example/v1"}"#,
    )
    .unwrap();
    let reg = ResellerRegistry::load_from_dirs(&[dir.path().to_path_buf()]);
    assert!(reg.warnings().is_empty());
    let d = reg.get("acme").unwrap().descriptor().expect("launch-capable purely from data");
    assert_eq!(d.protocol, "openai");
    assert_eq!(d.base_url, "https://api.acme.example/v1");
}
