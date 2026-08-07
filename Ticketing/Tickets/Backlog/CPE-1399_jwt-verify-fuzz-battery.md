---
id: CPE-1399
title: "Security: adversarial fuzz/property battery for HmacJwtVerifier::verify (attacker-controlled bearer tokens)"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-810
created: 2026-08-07
---

## Problem (hardening scout, Vein C — security boundary)
`crates/security/src/jwt.rs` (~L75-131, feature `jwt`) `HmacJwtVerifier::verify` parses a **fully
attacker-controlled** bearer token (from an HTTP header): base64 → JSON → HMAC decode chain. Uses Result/Option
throughout (no panics found on inspection), but a security-critical external-input parser with ZERO adversarial
coverage anywhere in the repo.

## Fix direction
Add a property/table battery (a `#[test]` in `crates/security/`, gated on feature `jwt`): random + truncated +
malformed token strings — bad base64, wrong segment count (0/1/2/4 dots), huge payload, non-UTF8 bytes, empty,
oversized, valid-shape-wrong-signature — asserting `verify` NEVER panics and always returns `Err` for anything
not a genuinely-signed token (never accepts a forged/tampered token). `cargo test -p cpe-security --features jwt`
must pass (local `os error 225` = Defender, not a code fail; CI 3-OS matrix authoritative). Report any panic OR
any input that wrongly verifies as a real security bug.
