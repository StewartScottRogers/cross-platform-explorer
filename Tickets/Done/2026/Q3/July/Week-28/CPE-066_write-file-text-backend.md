---
id: CPE-066
title: Backend command to save edited text back to a file (write_file_text)
type: Feature
status: Done
priority: Medium
component: Backend
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The preview pane can view files but not save changes. Add a Rust `write_file_text(path, contents)`
command that writes UTF-8 text back to a file, so the content editor (CPE-068) can persist edits.

## Acceptance Criteria

- [ ] `write_file_text(path, contents)` writes the string to the file, replacing its contents
- [ ] Returns the new byte length (or errors with a friendly message on failure)
- [ ] Registered in `generate_handler!`
- [ ] Rust unit test: write then read back returns the same content; overwrite replaces
- [ ] `cargo test` + `cargo clippy -D warnings` green locally; CI green cross-OS

## Resolution

Added `write_file_text(path, contents) -> Result<u64,String>` (writes UTF-8, returns new byte length),
registered in `generate_handler!`, with a unit test (write then read-back; overwrite replaces). Verified
locally: `cargo test` + `cargo clippy -D warnings` green.

## Work Log

2026-07-11 — Part of the content-editor set (viewer + edit-if-appropriate). Backend can be verified locally now that Rust is installed ([[rust-toolchain-installed-locally]]).

## Notes

Pairs with the size cap on `read_file_text`; large files are gated on the read side already. Relates to
[[CPE-067]] (editable model) and [[CPE-068]] (editor UI).
