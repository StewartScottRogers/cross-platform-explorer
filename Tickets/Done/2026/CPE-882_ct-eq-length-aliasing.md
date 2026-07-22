---
id: CPE-882
title: Fix constant-time token compare aliasing lengths differing by a multiple of 256
type: bug
component: Security
priority: high
tags: ready
epic: CPE-810
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
`cpe-security::authn::ct_eq` (the constant-time byte comparison behind API-token auth) seeded its diff
accumulator with `(a.len() ^ b.len()) as u8`. The `as u8` **truncates** the length-difference, so two
lengths whose XOR is a multiple of 256 alias to `0` — the length check silently passes. Because the content
scan reads the exhausted slice as `unwrap_or(&0)`, a short token followed by NUL padding then compares
equal: e.g. `ct_eq(b"A", &[b'A', 0,0,…256 zeros])` returned **`true`**. A token-equality primitive in the
authentication path that reports unequal tokens as equal is a real defect (a crafted padded token could
authenticate as a short configured token).

Fix: fold any length difference to a single bit (`if a.len()==b.len() {0} else {1}`) instead of truncating.
Branching on *length* leaks nothing secret; the content scan stays branch-free over the max length.

## Acceptance Criteria
- [x] `ct_eq(b"A", "A"+256×0x00)` is `false`; a token of that shape does not authenticate against a 1-byte
      configured token.
- [x] Existing equal/unequal/length cases still hold; content comparison remains non-early-out.
- [x] Full `cpe-security` suite (27) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — Found the `as u8` length-aliasing while auditing the security crate's authn
  providers for real vulns (path-scope + OIDC checked out clean). Fixed the fold + added a regression test
  that returns `true` against the old code. 27/27 tests pass; clippy clean.
