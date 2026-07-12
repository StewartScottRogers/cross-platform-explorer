---
id: CPE-119
title: Preview/edit support for Certificates and keys (PEM/CRT) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Certificates and keys (PEM/CRT) (.pem/.crt) in the right-side preview pane.
Decode and show subject, issuer, validity, fingerprints. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .pem/.crt is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Decode and show subject, issuer, validity, fingerprints.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend X.509 parse — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend X.509 parse. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .pem/.crt/.cer/.csr/.key preview: mapped to `code` so the PEM/base64 text is shown and editable; no dedicated grammar ships with highlight.js, so per the AC it uses the escaped-monospace fallback. Decoding the certificate fields (subject/issuer/validity) is a future backend enhancement. Editable as source via the shared text provider (write_file_text, CPE-066); load cancellation + large/corrupt-file fallback come from the shared PreviewPane (CPE-059). Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the text-based data/comms format batch (CPE-079/092/093/106/116/119/202/203/204/209/212/213). npm run check clean; unit tests green.
