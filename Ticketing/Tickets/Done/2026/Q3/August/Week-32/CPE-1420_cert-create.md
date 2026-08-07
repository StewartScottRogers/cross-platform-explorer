---
id: CPE-1420
title: "Certificate create (keypair + self-signed cert, RSA/EC) — backend + tests"
type: Feature
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Add `cert_create(params) -> {cert_pem, key_pem}` to `crates/server` (`cert_create.rs`) via `rcgen`: params =
subject CN + optional SANs (DNS/IP), validity days, key type (EC-P256/P384 or RSA-2048/4096), is_ca. Return PEM
cert + PEM private key. The `#[tauri::command]` dispatcher writes them to a chosen path (cert + key; the key with
restrictive perms where the OS supports it; NEVER log key material). NEW DEP: `rcgen` (pure Rust; RSA keygen may
need the `rsa`/`ring`/`aws-lc-rs` backend — pick the pure-Rust path, no OpenSSL; justify in PR). Round-trip TEST:
create → feed the cert to `cert_decode` (CPE-1419) and assert the expected subject/validity/keytype/SANs; assert
the key pairs with the cert. specta::Type + REGEN bindings (drift guard). `cargo test -p cpe-server` +
`cargo clippy -p cpe-server --all-targets -- -D warnings` clean (Defender note). Foundation for CPE-1421 (sign)
and CPE-1423 (the create dialog).
