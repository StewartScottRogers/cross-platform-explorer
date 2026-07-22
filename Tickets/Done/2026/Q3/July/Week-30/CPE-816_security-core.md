---
id: CPE-816
title: Security core — 3-plane traits, composable chain, default-deny
type: feature
component: Backend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. The pluggable security foundation at the contract boundary. Three plane traits —
**Transport security**, **Authentication**, **Authorization** — plus an **ordered interceptor chain**
with a per-plane *combine policy* (`all-must-pass` / `any-passes` / `first-match`) so multiple
providers run at once (the "run multiple security solutions" requirement). Config-driven registry:
adding a provider = implement the trait + register, no core changes. A **null/passthrough** provider
for local mode keeps the plain explorer fast (security bills only remotely). Reuse `audit_journal.rs`
for the audit hook on every decision. Prereq: CPE-811 (envelope carries principal/session).

Two invariants, both tested:
- **Default-deny at the boundary** — structurally impossible to leave a command unsecured.
- **Local = null/passthrough** — trusted in-process principal, no transport crypto.

## Acceptance Criteria
- [x] `TransportSec` / `AuthN` / `AuthZ` traits + a provider registry + config-driven activation/order.
- [x] Interceptor chain supports all-must-pass / any-passes / first-match combine policies (unit-tested).
- [x] Null/passthrough local provider; a default-deny test proves an unconfigured boundary denies.
- [x] Every decision hooks the audit journal; headless-testable; clippy clean both modes.

## Work Log
2026-07-20 — Picked up. Estimate: 4h+ (unchanged). Prereq CPE-811 is Done (merged #76) — the
`cpe-contract` `Principal`/`Session` slot exists, so security can key off it. Plan: new pure crate
`crates/security` (pkg `cpe-security`), path-dep on `cpe-contract`. Three plane traits
(`TransportSecurity`/`Authenticator`/`Authorizer`); a generic `combine()` implementing
all-must-pass / any-passes / first-match with **structural default-deny** (empty/all-abstain →
Deny); a `ProviderRegistry` + `SecurityConfig` (serde) for config-driven activation/order; a
null/passthrough local provider set + `SecurityChain::local()` fast path; an `AuditSink` trait
(the hook the app will bridge to `audit_journal.rs` at integration, CPE-818/820) with a `NullAudit`
and a test `MemoryAudit`. Concrete providers (token/mTLS/OAuth, path-scope) are CPE-817/818 — this
ticket is the core only. Add the crate to the CI crates job.
2026-07-20 — Built `crates/security` (pkg `cpe-security`, path-dep on `cpe-contract`). Delivered:
`Verdict` (Allow/Deny/Abstain); `CombinePolicy` (AllMustPass/AnyPasses/FirstMatch, serde snake_case);
a generic `combine()` with **structural default-deny** (empty chain OR all-abstain → Deny under every
policy) and correct short-circuiting (AnyPasses/FirstMatch stop at first allow so a later authenticator
can't clobber an established principal); the three plane traits `TransportSecurity`/`Authenticator`
(mutates principal)/`Authorizer`; `SecurityContext` (principal from the envelope + method + optional
resource + is_local + generic attribute bag so providers need no core change); `SecurityChain` running
Transport→AuthN→AuthZ with `local()` passthrough fast path + `default_deny()` empty stack; `AuditSink`
trait + `NullAudit` + test `MemoryAudit`, hooked on every terminal decision; and a `ProviderRegistry`
(+ `SecurityConfig`/`PlaneConfig`, serde) that assembles a chain from named providers in order and
errors (`BuildError`) on an unknown name — the config-driven, no-core-change extension point.
2026-07-20 — Verified: `cargo test` 11/11 green (each combine policy, empty+all-abstain default-deny,
local allows, unconfigured boundary denies at the Transport plane, audit sink fires exactly once with
the denying plane/provider, unknown-provider build error, config JSON round-trip); `cargo clippy
--all-targets -D warnings` clean. serde + `cpe-contract` only — no Tauri. "clippy clean both modes":
`src-tauri` untouched, so both app feature builds are unaffected; the new crate has no features.
2026-07-20 — CI: generalized the `contract` job into a `Server crates` job that lints+tests both
`crates/contract` and `crates/security` on all 3 shipped OSes (caches both target dirs). Future
CPE-815 `server` crate joins the same job.

## Resolution
Added **`crates/security`** (package `cpe-security` v0.1.0) — the pluggable, composable security core
that sits at the CPE-810 contract boundary, so the Server logic stays security-agnostic and receives an
already-authorized request.

Design (all in `crates/security/src/lib.rs`):
- **Three plane traits** — `TransportSecurity`, `Authenticator` (may establish `ctx.principal`),
  `Authorizer` — each provider implements one and is registered by name.
- **Composable chain** — a generic `combine()` implements `AllMustPass` / `AnyPasses` / `FirstMatch`
  with lazy short-circuit, and a `SecurityChain` runs Transport→AuthN→AuthZ, denying on the first plane
  that fails.
- **Default-deny is structural** — an empty plane or an all-abstain plane denies under *every* policy,
  so you cannot leave the boundary open by forgetting to configure it (`SecurityChain::default_deny()`
  proves it in a test).
- **Local = null/passthrough** — `SecurityChain::local()` / `SecurityConfig::local()` wire the built-in
  `Passthrough` provider in every plane with no audit, preserving the local fast/small tiebreaker.
- **Config-driven registry** — `ProviderRegistry` + `SecurityConfig`/`PlaneConfig` (serde) assemble a
  chain from provider *names* + order + per-plane policy; adding a provider = implement the trait +
  register + name it in config, with `BuildError` on an unknown name. No core edits to extend.
- **Audit hook** — an `AuditSink` trait (`NullAudit`, test `MemoryAudit`) receives every terminal
  `AuditDecision`; the app bridges this to `audit_journal.rs` when the layer is integrated (CPE-818/820).

Files: `crates/security/Cargo.toml`, `crates/security/src/lib.rs`, `crates/security/Cargo.lock`;
`.github/workflows/ci.yml` (renamed `contract` job → `Server crates`, added the security step + cache).

Scope/tradeoffs:
- **Core only** — concrete providers (API-token/mTLS/OAuth AuthN via `AnyPasses`; path-scope + capability
  AuthZ via `AllMustPass`; TLS/mTLS transport) are CPE-817/818, which now have their extension seam.
- Reused `cpe-contract`'s `Principal` for identity (re-exported) rather than a parallel type.
- Sequencing note: landed before the "first slice" CPE-812 because CPE-816 depends only on CPE-811
  (done) and is independent scaffolding — no code dependency on 812. CPE-812 (tauri-specta bindings) is
  left for an attended session: its final AC is an explicit GUI-verify across the 113-command migration,
  which shouldn't be shipped blind (same discipline as CPE-676).