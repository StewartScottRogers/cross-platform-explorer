---
id: CPE-818
title: AuthZ (path-scope + capability) + transport security (TLS/mTLS)
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
Child of CPE-810. Full-stack authorization + channel crypto (decision: full security stack now).
**AuthZ**: a `path-scope` allowlist provider and a `capability`-grant provider behind the CPE-816
trait, composed `all-must-pass`, deciding whether a `Principal` may run *this op on this path* — the
file-explorer-specific risk surface. **Transport security**: TLS + mTLS channel providers. Prereq: CPE-816.

## Acceptance Criteria
- [x] `path-scope` + `capability` AuthZ providers; `all-must-pass` composition enforced.
- [x] Path-scope denies traversal outside the granted roots (tested with escape attempts).
- [x] TLS + mTLS transport-security providers; local mode stays null/passthrough.
- [x] Deny paths audited; unit-tested; clippy clean both modes.

## Work Log
2026-07-20 — Picked up. Estimate: 4h+ (unchanged). Prereq CPE-816 Done (#77); AuthN siblings CPE-817
also merged (#78). Plan: two dependency-free modules in `cpe-security`:
- `authz`: `PathScopeAuthorizer` (pure lexical path normalization — resolves `.`/`..` without touching
  the fs, denies any resource that escapes the granted roots) + `CapabilityAuthorizer` (method→required
  capability, principal→granted capabilities); composed `all-must-pass`. Both `Abstain` when they don't
  apply (no resource / no capability required) so a non-path op isn't spuriously denied by path-scope.
- `transport`: `RequireTls` / `RequireMtls` policy providers that assert channel requirements over
  transport attributes (`tls.established`, verified `tls.client_cert.subject`) which the socket layer
  (CPE-820) populates; `is_local` short-circuits to Allow so local stays null/passthrough.
Prove `all-must-pass` AuthZ + path-escape denial + transport requirement + audited denies, headless.
Note: lexical (not fs-canonical) containment — real deployments canonicalize at integration (CPE-820);
documented. No new deps.
2026-07-20 — Built `crates/security/src/authz.rs` (`PathScopeAuthorizer` + `CapabilityAuthorizer`) and
`transport.rs` (`RequireTls` + `RequireMtls`), registered both modules in `lib.rs`. Path-scope uses a
lexical `normalize_segments` (splits on `/` and `\`, resolves `.`/`..`, returns None on an upward
escape) + prefix containment. Transport providers assert `tls.established` / verified
`tls.client_cert.subject` and short-circuit to Allow on `is_local`.
2026-07-20 — Verified: `cargo test` 26/26 green (9 new: within/outside root, 5 traversal-escape attempts
all denied, separator-agnostic, abstain-without-resource, capability grant/deny/abstain, end-to-end
all-must-pass allow + no-capability-deny + path-escape-deny-audited; TLS/mTLS require-encrypted /
require-cert / local-bypass). `cargo clippy --all-targets -D warnings` clean (fixed one lint: `..`
escape uses `out.pop()?`). No new deps; `src-tauri` untouched (both modes unaffected). No CI change —
the `Server crates` job already covers `crates/security`.

## Resolution
Added the reference **authorization** and **transport-security** providers to `cpe-security`, completing
the three-plane provider set the CPE-816 core defined.

AuthZ (`crates/security/src/authz.rs`), composed `all-must-pass`:
- **`PathScopeAuthorizer`** — the resource must resolve under one of the granted roots. Containment is
  **lexical**: `normalize_segments` splits on `/` and `\`, drops `.`, pops on `..`, and treats a `..`
  that pops above the root as an escape (denied) — so `..`/re-rooting traversal can't reach outside the
  grant without touching the filesystem. Abstains for ops with no resource.
- **`CapabilityAuthorizer`** — method→required-capability and principal→granted-capabilities; denies when
  the principal lacks the op's capability, abstains for methods that require none.

Transport security (`crates/security/src/transport.rs`) — the **policy** side of channel crypto:
- **`RequireTls`** / **`RequireMtls`** assert `tls.established` (and, for mTLS, a verified
  `tls.client_cert.subject`) over transport attributes the socket layer sets after the real handshake
  (CPE-820). Both short-circuit to Allow when `ctx.is_local`, so **local stays null/passthrough**.

Files: `crates/security/src/authz.rs`, `crates/security/src/transport.rs` (+ two `pub mod` lines in
`lib.rs`). 9 unit tests added (26 total).

Scope/tradeoffs:
- **Lexical, not fs-canonical, containment** — deterministic + headless + symlink-agnostic; a real remote
  deployment should also canonicalize against the filesystem at the socket boundary (CPE-820). Documented
  in the module. Case-sensitive comparison (a Windows case-fold refinement is left to integration).
- The transport providers enforce *requirements*; the actual rustls/socket TLS setup lives in the
  Client/Server binaries (CPE-820), keeping this crate transport-agnostic and dependency-light.