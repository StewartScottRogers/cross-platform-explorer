---
id: CPE-1419
title: "Certificate decoder (X.509 PEM/DER + CSR + public key) — backend + tests"
type: Feature
status: Backlog
priority: High
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Add `cert_decode(bytes: &[u8]) -> CertPreview` to `crates/server` (`cert_decode.rs`). Auto-detect + parse: X.509
cert (PEM or DER), PKCS#10 CSR, and a public-key file. Surface: subject + issuer DN fields, serial, version,
validity notBefore/notAfter (human + expired/not_yet_valid), signature algorithm, public-key algo + size/curve,
SANs, basicConstraints is_ca, key-usage + extended-key-usage, SHA-256 + SHA-1 fingerprints. CSR: subject +
requested SANs + pubkey. PRIVATE KEY file: report ONLY algo + size, NEVER key material. New dep: `x509-parser`
(pure Rust, nom-based, no OpenSSL) — justify in PR. Never panic on malformed input (typed Err + panic-safety
battery). specta::Type + regen bindings. Unit-test against the CPE-1425 sample certs (RSA self-signed, EC, DER,
expired, CSR).
