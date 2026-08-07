---
id: CPE-1421
title: "Certificate sign / issue-from-CSR (with a CA) — backend + tests"
type: Feature
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Add certificate signing to `crates/server` (`cert_sign.rs`) via `rcgen` (already a dep from CPE-1420):
`cert_issue_from_csr(csr_pem, ca_cert_pem, ca_key_pem, validity_days) -> issued_cert_pem`. Parse the PKCS#10
CSR, issue a leaf cert signed by the CA cert+key, honoring the CSR's subject + requested SANs; set issuer = CA
subject. Also expose a self-sign convenience if not already covered by cert_create. NEVER panic on a malformed
CSR / mismatched CA key (typed Err). NEVER log/return the CA private key beyond what's needed. `#[tauri::command]`
thin dispatcher(s) reading the CSR/CA paths, writing the issued cert to a chosen path. specta::Type + REGEN
bindings (drift guard). ROUND-TRIP TEST: cert_create a CA (CPE-1420) → build a CSR (via rcgen) → issue → cert_decode
shows issuer=CA subject, subject=CSR subject, SANs carried over, and the issued cert's signature VERIFIES against
the CA public key (use the ring/x509 verify path like the CPE-1420 UAT did). `cargo test -p cpe-server` +
`cargo clippy -p cpe-server --all-targets -- -D warnings` clean (Defender note). No frontend (that's CPE-1423).
