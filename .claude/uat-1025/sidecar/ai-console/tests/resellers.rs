//! CPE-448: the bundled reseller manifests load cleanly and cohere. Guards against a typo'd or
//! schema-drifted `resellers/*.json` shipping broken — the manifests are data, so this is their test.

use std::path::PathBuf;

use ai_console::model_catalog::ResellerRegistry;

fn resellers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resellers")
}

#[test]
fn the_bundled_reseller_catalog_loads_cleanly() {
    let reg = ResellerRegistry::load_from_dirs(&[resellers_dir()]);
    assert!(reg.warnings().is_empty(), "manifest load warnings: {:?}", reg.warnings());
    // OpenRouter (the first-class source) plus the researched Tier-1/2 set.
    assert!(reg.len() >= 9, "expected the full reseller set, got {}", reg.len());
    assert!(reg.get("openrouter").is_some(), "OpenRouter manifest missing");
    assert!(reg.get("github-models").is_some(), "GitHub Models manifest missing");
}

#[test]
fn every_reseller_is_coherent() {
    let reg = ResellerRegistry::load_from_dirs(&[resellers_dir()]);
    for m in reg.all() {
        assert!(m.models_endpoint.starts_with("https://"), "{} endpoint must be https", m.id);
        assert!(!m.api_hosts.is_empty(), "{} has no api_hosts", m.id);
        // The endpoint's host must be covered by the egress allow-list (no un-listed egress).
        let host = m
            .models_endpoint
            .strip_prefix("https://")
            .and_then(|r| r.split('/').next())
            .unwrap_or("");
        assert!(m.api_hosts.iter().any(|h| h == host), "{}: endpoint host {host} not in api_hosts", m.id);
    }
}

#[test]
fn known_model_hosts_are_in_the_egress_allow_list() {
    let reg = ResellerRegistry::load_from_dirs(&[resellers_dir()]);
    let allow = reg.egress_allow_list();
    for host in ["openrouter.ai", "api.groq.com", "models.github.ai", "api.together.xyz"] {
        assert!(allow.iter().any(|h| h == host), "{host} should be egress-allowed");
    }
}
