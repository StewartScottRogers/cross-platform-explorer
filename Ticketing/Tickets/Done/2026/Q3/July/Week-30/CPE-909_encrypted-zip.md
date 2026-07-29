---
id: CPE-909
title: Password-protected (AES-256) zip create + extract
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-705
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The backend for password-protected archives (CPE-705). Added to `cpe_server::archive`:
- `compress_to_zip_encrypted(paths, dest, password)` — packs into an **AES-256** encrypted `.zip` (the
  `zip` crate's `aes-crypto`, a default feature — pure Rust, no sys libs). Empty selection / empty
  password error.
- `extract_zip_encrypted(path, dest, password)` — extracts with the password; a **wrong password is a
  clear error** (not a silent garbage extraction), and entries keep the same zip-slip guard as the plain
  extractor.

Refactored the shared `zip_add_path` walker to take `FileOptions<'_, ()>` so it works with both plain and
encryption-bearing options (encrypted options borrow the password, so they aren't `'static`).

## Acceptance Criteria
- [x] Create an encrypted zip and extract it back byte-exact with the right password.
- [x] A wrong password → clear error; empty selection / empty password → error.
- [x] Zip-slip guard preserved on the encrypted extractor. 12 archive tests, clippy `-D warnings` clean, 3-OS.

## Work Log
- 2026-07-22 — Second headless slice of CPE-705. The GUI prompt (ask for a password on read / offer one on
  create) is the attended remainder; the read+create crypto is done here.
