---
id: CPE-898
title: known_hosts honours @revoked / @cert-authority markers (a revoked key must be refused)
type: bug
component: Backend
priority: high
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Found auditing the CPE-897 host-key core I'd just shipped: `parse_known_hosts` **skipped every line
starting with `@`**, so a `@revoked` entry was ignored — a revoked host key would be treated as
`Unknown`/`Trusted` and could be TOFU-accepted. OpenSSH refuses a `@revoked` key outright; skipping the
marker is a real security gap. (`@cert-authority` lines were likewise dropped, but silently — a CA key
was never a host key, so that was safe; still worth parsing explicitly.)

Fix (crypto-free, plain-hostname): parse an optional leading `@revoked` / `@cert-authority` marker
(shifting the remaining fields), add a `Revoked` verdict, and make `verify_host_key` refuse a presented
key that matches a `@revoked` entry **before** any trust decision (revocation wins). `@cert-authority`
entries are parsed but excluded from host-key matching (certificate validation is out of scope); an
unknown `@marker` line is skipped rather than mis-parsed.

## Acceptance Criteria
- [x] `@revoked` entries parsed; a presented key matching one → `HostKeyVerdict::Revoked` (refused), even
      when a separate non-revoked entry would trust it.
- [x] `@cert-authority` parsed but never establishes host-key trust; unknown `@marker` lines skipped.
- [x] Existing Trusted/Unknown/Changed behaviour unchanged for normal entries.
- [x] `cargo test` (8: 5 + 3 new) + `cargo clippy --all-targets -D warnings` clean.

## Work Log
- 2026-07-22 (nightshift) — Caught in a byte-slice/parser audit of the pure-logic modules (the slice audit
  itself came back clean — all string slicing is char-boundary-safe). Hardening the just-shipped CPE-897.
  Hashed-hostname (`|1|salt|hash`) support is still the remaining known_hosts follow-up (needs hmac-sha1 +
  a real interop vector).
