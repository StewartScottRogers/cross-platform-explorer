//! CPE-322: real OS-keychain round-trip. The unit tests in `providers::secrets` use an in-memory
//! MOCK; this exercises the actual [`KeyringBackend`] against the native store — Windows Credential
//! Manager, macOS Keychain, or Linux Secret Service — proving the "secrets only in the OS keychain"
//! invariant (ADR 0001 / CPE-268) holds on the platform it runs on.
//!
//! Marked `#[ignore]` so a normal `cargo test` never touches the real keychain (and never fails on a
//! headless box with no Secret Service). A dedicated CI step runs it per-OS with a keychain available
//! (`cargo test --test keyring_roundtrip -- --ignored`); on Linux that step first starts an unlocked
//! gnome-keyring inside a D-Bus session.

#![cfg(any(windows, target_os = "macos", target_os = "linux"))]

use sidecar_host::providers::secrets::{KeyringBackend, SecretBackend};

#[test]
#[ignore = "hits the real OS keychain; run explicitly (`-- --ignored`) where one is available"]
fn keyring_backend_round_trips_against_the_real_os_store() {
    let backend = KeyringBackend;
    // A unique service so a prior/parallel run (or a real app secret) can never collide.
    let service = format!("com.cross-platform-explorer.test.cpe322.{}", std::process::id());
    let account = "roundtrip";
    let secret = "s3cr3t-cpe322-value";

    // Start from a clean slate (delete is a no-op if absent).
    backend.delete(&service, account).expect("pre-delete should not error");

    // set → get returns exactly what we stored, from the native store.
    backend.set(&service, account, secret).expect("set into the OS keychain");
    let got = backend.get(&service, account).expect("get from the OS keychain");
    assert_eq!(got.as_deref(), Some(secret), "secret did not round-trip through the OS store");

    // delete → get returns None (the entry is really gone).
    backend.delete(&service, account).expect("delete from the OS keychain");
    assert_eq!(
        backend.get(&service, account).expect("get after delete"),
        None,
        "secret was not removed from the OS store",
    );
}
