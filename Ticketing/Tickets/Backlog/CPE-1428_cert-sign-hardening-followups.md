---
id: CPE-1428
title: "cert_issue_from_csr hardening: size-guard the file reads + CSR-requests-CA regression test"
type: Task
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Problem (CPE-1421 / PR #695 reviewer follow-ups — non-blocking)
1. **Missing size guard (consistency/hardening):** the `cert_issue_from_csr` Tauri dispatcher's three
   `fs::read_to_string` calls (CSR/CA-cert/CA-key, `src-tauri/src/lib.rs` ~L1001-1003) lack the
   `ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)` (128 MiB) guard that every other untrusted-file-reading
   command here uses (cert_decode, jwt_preview). Low severity (these files are tiny) but an inconsistency in a
   security-sensitive command handling untrusted input — an oversized file would be read fully into memory uncapped.
2. **Regression test:** add a test that a CSR REQUESTING `IsCa::Ca` still gets issued as a NON-CA leaf (the
   `IsCa::NoCa` override at cert_sign.rs is the property the threat model most depends on; currently only asserted
   on a plain CSR). rcgen 0.14 DOES honor a CSR's BasicConstraints extensionRequest, so this guard is load-bearing.
3. **Comment nit:** the Cargo.toml/module-doc comment says rcgen pins x509-parser "^0.17"; it actually pins 0.18
   (resolves 0.18.1). Fix the comment.

## Fix direction
Add `ensure_previewable_size` to the 3 reads; add the CSR-requests-CA→non-CA regression test; correct the comment.
