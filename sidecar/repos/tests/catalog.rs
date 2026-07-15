//! The bundled forge-provider catalog (CPE-431) loads and is coherent — the "list all of them" data
//! validated against the registry.

use std::path::PathBuf;

use repos::providers::ProviderRegistry;

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("providers")
}

#[test]
fn the_bundled_provider_catalog_loads_cleanly() {
    let reg = ProviderRegistry::load_from_dirs(&[catalog_dir()]);
    assert!(reg.warnings().is_empty(), "warnings: {:?}", reg.warnings());
    // Tier-1 git forges + the generic fallback, plus the experimental non-git set.
    for id in [
        "github", "github-enterprise", "gitlab", "bitbucket", "gitea", "forgejo", "codeberg",
        "sourcehut", "azure-devops", "aws-codecommit", "generic-git", "mercurial", "subversion",
        "perforce",
    ] {
        assert!(reg.get(id).is_some(), "missing provider '{id}'");
    }
    assert!(reg.len() >= 14);
}

#[test]
fn every_provider_is_coherent() {
    let reg = ProviderRegistry::load_from_dirs(&[catalog_dir()]);
    for p in reg.all() {
        assert!(!p.name.trim().is_empty(), "{} has no name", p.id);
        assert!(!p.capabilities.is_empty(), "{} advertises no capabilities", p.id);
        // Every provider can at least clone (that's the point of a source-control provider).
        assert!(p.supports_capability("clone"), "{} can't clone", p.id);
        // Non-git backends are flagged experimental for now.
        if p.kind != "git" {
            assert!(p.experimental, "{} is a non-git backend but not flagged experimental", p.id);
        }
    }
}

#[test]
fn known_api_hosts_are_in_the_egress_allow_list() {
    let reg = ProviderRegistry::load_from_dirs(&[catalog_dir()]);
    let allow = reg.egress_allow_list();
    for host in ["api.github.com", "gitlab.com", "api.bitbucket.org", "codeberg.org", "dev.azure.com"] {
        assert!(allow.contains(&host.to_string()), "allow-list missing {host}: {allow:?}");
    }
    // The generic-git provider contributes no API host (git-only, no browse).
    assert!(reg.get("generic-git").unwrap().api_hosts.is_empty());
    assert!(!reg.get("generic-git").unwrap().supports_capability("browse"));
}
