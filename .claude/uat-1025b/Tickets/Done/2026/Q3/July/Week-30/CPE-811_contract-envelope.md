---
id: CPE-811
title: Transport-neutral contract envelope (serde-JSON RPC)
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810. Define the transport-neutral request/response **envelope** that both the local
in-process path and the future network Client(Rust) speak. Own **serde-JSON** framing (decision:
wire format), reusing the proven `sidecar/contract` pattern: a `ContractVersion` + `negotiate()`,
a Hello/Welcome handshake, and a `schema_version` field. Pure crate, no Tauri, headless-testable.

Carry a `principal`/`session` slot in the envelope from day one so the future **multi-client** model
(decision: both, single-user first) is not precluded — even though single-user ships first and leaves
it defaulted.

## Acceptance Criteria
- [x] A `contract` crate with the request/response envelope, `ContractVersion`, and `negotiate()`.
- [x] Hello/Welcome handshake + `schema_version`, mirroring `sidecar/contract`'s approach.
- [x] Envelope reserves a `principal`/`session` field (defaulted for single-user) for later multi-client.
- [x] Version-mismatch negotiation is unit-tested; no Tauri dependency; clippy clean both modes.

## Work Log
2026-07-20 — Picked up. Estimate: 3-4h (unchanged; scoped to a fresh standalone crate + CI wiring).
Plan: new pure crate `crates/contract` (pkg `cpe-contract`), mirroring `sidecar/contract`'s
`ContractVersion`/`negotiate()`/Hello-Welcome/`schema_version`, but shaped for the GUI↔Server RPC
(method/params Request + result/error Response). Envelope reserves a defaulted `session`/`principal`
slot for the future multi-client model. Standalone (no Tauri, no workspace) so `src-tauri` can `path`-dep
it later (CPE-815), exactly like sidecar-contract. Add a 3-OS CI job so it's linted+tested on every
shipped platform.
2026-07-20 — Built `crates/contract` (pkg `cpe-contract`): `ContractVersion` + `negotiate()` (equal
major, min minor — same rule as sidecar), `Hello`/`Welcome`/`Rejected` handshake, `Envelope` with
`schema_version` (= `ENVELOPE_SCHEMA_VERSION`) + correlation `id` + a `Session`/`Principal` slot, a
`Request`(method+params)/`Response`(Ok|`ContractError`) RPC pair, `StreamItem`/`StreamEnd` reserving
the streaming seam (CPE-819), a snake_case `ErrorCode`/`RejectCode` taxonomy (incl. reserved
`Unauthenticated`/`Unauthorized` for the security layer), and a `codec` line framer. `Session` is
`#[serde(default, skip_serializing_if = "Session::is_local")]` on the envelope, so the single-user
local frame carries NO session bytes (verified by test) yet round-trips to the default local session —
honoring the local fast/small tiebreaker.
2026-07-20 — Verified locally: `cargo test` 9/9 green (incl. version-mismatch negotiation, local-frame
omits-session, remote-session round-trip, full handshake + every-variant round-trips); `cargo clippy
--all-targets -D warnings` clean. Crate depends only on serde + serde_json — no Tauri. "clippy clean
both modes" holds trivially: `src-tauri` is untouched, so both its default and `sidecar-platform` builds
are unaffected; the new crate has no features. No frontend files changed, so `npm run check` is N/A.
2026-07-20 — Wired CI: added a dedicated 3-OS `contract` job (clippy + test in `crates/contract`),
mirroring the standalone `sidecar` job so the boundary stays out of the app workspace but is still
linted/tested on every shipped OS. Added `crates/.gitignore` (`target/`) and committed `Cargo.lock`
(matching the sidecar crates) for reproducible CI.

## Resolution
Added a new standalone, Tauri-free crate **`crates/contract`** (package `cpe-contract`, v0.1.0) that is
the versioned, transport-neutral wire contract for the GUI↔Server boundary of epic CPE-810. It reuses
the `sidecar/contract` versioning approach but is shaped for a client/server RPC.

Files:
- `crates/contract/Cargo.toml` — pure crate (serde + serde_json only), standalone (no workspace), so
  `src-tauri` can `path`-depend on it later (CPE-815) exactly as it does `sidecar/contract`.
- `crates/contract/src/lib.rs` — `ContractVersion`/`negotiate()`, `Hello`/`Welcome`/`Rejected`
  handshake, `Envelope` (`schema_version` + correlation `id` + defaulted `Session`/`Principal` slot),
  `Request`/`Response`, `StreamItem`/`StreamEnd` (streaming seam), `ErrorCode`/`RejectCode`/`ContractError`
  taxonomy, and a `codec` line framer. 9 unit tests.
- `crates/contract/Cargo.lock` — committed for reproducible CI (matches sidecar crates).
- `crates/.gitignore` — ignores `target/`.
- `.github/workflows/ci.yml` — new 3-OS `contract` job (clippy + test).

Tradeoffs / decisions:
- **Standalone crate, not a `src-tauri` workspace member.** Preserves the one-way boundary and keeps the
  crate Tauri-free/headless — the prerequisite for extracting the Server (CPE-815). Consistent with the
  established sidecar precedent; needs its own CI job (added) since the `backend` job won't cover it.
- **Local frame omits the session slot** (`skip_serializing_if`), so single-user mode pays zero extra
  bytes — protecting the local fast/small/predictable tiebreaker while still reserving multi-client.
- Named the package `cpe-contract` (not `contract`) to avoid colliding with `sidecar-contract` and to
  signal it's the app's own client↔server contract.
- Reserved (but did not implement) the security-related error/reject codes and the streaming variants;
  they are wired up by later children (CPE-816+ security, CPE-819 streaming) — this ticket only lays the
  transport-neutral foundation, per its scope.