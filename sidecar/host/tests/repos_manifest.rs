//! CPE-432 (AC3): the `repos` sidecar is a registered, contract-compatible tenant.
//!
//! Bundling + wiring "behind the sidecar-platform feature" means the host's generic registry
//! discovers the repos manifest and accepts it (right contract major, well-formed capabilities), so
//! it shows up in the management panel (enable/disable, compat) exactly like the AI Console — no
//! bespoke per-sidecar code. This loads the source-tree manifest the app also points at in dev.

use std::path::{Path, PathBuf};

use sidecar_contract::{Capability, CONTRACT_VERSION};
use sidecar_host::registry::Registry;

fn repos_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../repos")
}

#[test]
fn the_repos_manifest_is_discovered_and_contract_compatible() {
    let reg = Registry::load_from_dirs(&[repos_dir()]);

    // Nothing was skipped as malformed / incompatible.
    assert!(reg.warnings().is_empty(), "manifest load warnings: {:?}", reg.warnings());

    let repos = reg
        .all()
        .find(|m| m.id == "repos")
        .expect("the repos manifest should be registered");

    assert_eq!(repos.name, "Repositories");

    // Same contract major as the host, and its minor is not ahead of ours → compatible.
    assert_eq!(repos.contract_version.major, CONTRACT_VERSION.major, "contract major must match the host");
    assert!(repos.contract_version.minor <= CONTRACT_VERSION.minor, "contract minor must not exceed the host");

    // It declares the capabilities the sidecar actually requests at handshake (secrets + network are
    // the load-bearing ones for forge tokens and host-brokered egress).
    for cap in [Capability::Context, Capability::Secrets, Capability::Storage, Capability::Events, Capability::Network] {
        assert!(repos.capabilities.contains(&cap), "manifest missing capability {cap:?}");
    }

    // It announces its own UI (the loopback server the process starts on Welcome).
    let ui = repos.ui.as_ref().expect("repos declares a UI mount");
    assert_eq!(ui.kind, "local_port");
    assert_eq!(ui.source, "announced");
}
