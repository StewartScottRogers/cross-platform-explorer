---
id: CPE-1398
title: "Security: adversarial panic-safety battery for WebDAV parse_multistatus (untrusted network XML)"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-616
created: 2026-08-07
---

## Problem (hardening scout, Vein C — highest-integrity value)
`crates/webdav/src/lib.rs` (~L149-177) `parse_multistatus` hand-parses PROPFIND-response XML that is
**network-controlled** (a malicious/buggy WebDAV server). No `.unwrap()`/index panics found on inspection, but
there is ZERO adversarial coverage — only a happy-path in-process test server. The repo's proven
`crates/server/tests/parser_panic_safety.rs` methodology never reaches this crate.

## Fix direction
Add a small table-driven panic-safety battery (a `#[test]` in `crates/webdav/`) feeding `parse_multistatus`:
garbage bytes, truncated XML, deeply-nested elements, huge/negative `getcontentlength`, missing `href`,
duplicate/empty tags, non-UTF8, mismatched close tags — asserting it NEVER panics (returns Ok/Err, never
unwinds). Mirror the shape of `parser_panic_safety.rs`. `cargo test -p cpe-webdav` must pass (note: local
`os error 225` = Windows Defender quarantine, not a code failure — compile still validates; CI's 3-OS matrix
is authoritative). Report any actual panic found as a real bug.
