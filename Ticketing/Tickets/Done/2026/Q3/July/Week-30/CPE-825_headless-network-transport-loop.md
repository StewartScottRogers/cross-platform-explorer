---
id: CPE-825
title: Headless network transport loop — Client(Rust) proxy + reference Server binary + conformance/benchmark
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810, carved from CPE-820 as its **fully headless-verifiable half** (the GUI end-to-end tail
stays in CPE-820, gated on CPE-819 + an attended session). Close the remote loop **in Rust, with no
frontend and no specta**: a new `cpe-net` composition crate providing

- a transport-neutral **wire** layer that reads/writes the CPE-811 `Envelope` (JSON-lines codec) over any
  `Read`/`Write`, with a concrete **loopback TCP** transport;
- a **Server runtime** that accepts a connection, runs the CPE-811 Hello/Welcome handshake (version
  `negotiate`), and drives each `Request` through the CPE-824 `Dispatcher` **behind the CPE-816
  `SecurityChain`** — so authN/authZ/transport enforce on the network path and a denial maps to a
  structured `ContractError` (`Unauthorized`/`Unauthenticated`), never a leaked domain result;
- a **Client(Rust) proxy** implementing the contract over the socket: connect → handshake → `call(method,
  params) -> Result<Value, ContractError>`;
- a deployable **reference headless Server binary** (`cpe-server-ref`) wrapping the dispatcher + a
  configurable security chain over a loopback listener.

`cpe-net` depends on `cpe-contract` + `cpe-server` + `cpe-security` and keeps `cpe-server` pure (it must
not gain a security dep). Proves `Client(Rust) → Server(Rust)` end-to-end over loopback, entirely under
`cargo test`.

## Acceptance Criteria
- [x] `cpe-net` crate: wire framing + loopback `Server` runtime + `Client(Rust)` proxy; end-to-end
      browse over loopback (`list_dir` via the dispatcher) returns real entries.
- [x] Security stack enforces on the remote path: a `default_deny` chain denies at the boundary
      (mapped to a security error — `Unauthenticated` for a transport/authN deny, `Unauthorized` for an
      authZ deny), the `local`/passthrough chain allows; the Server never dispatches a denied request.
- [x] **Conformance:** a client with a mismatched contract **major** is cleanly `Rejected`
      (`IncompatibleVersion`) at handshake and `Client::connect` returns a typed error; a compatible
      lower-minor client negotiates and succeeds.
- [x] **Local-fast guard:** a benchmark test proves the in-process dispatch path (no socket) is not
      taxed by the remote machinery — it runs strictly faster than the same calls over loopback, measured
      in-run (self-calibrating, no absolute wall-clock assertion, so CI-stable).
- [x] Reference headless Server binary builds (`cargo build -p cpe-net --bin cpe-server-ref`).
- [x] CI "Server crates" job builds/tests `cpe-net` on all 3 OSes; `cargo clippy --all-targets -D
      warnings` clean.
- [x] Architecture note for the transport loop added under `docs/design/`.

## Notes
GUI-verified end-to-end browse/preview + one real (non-loopback) remote + the frontend transport seam
remain in **CPE-820** (needs CPE-819 + attended GUI). This ticket delivers the Rust plumbing they sit on.

## Resolution
Added a new standalone, Tauri-free crate **`crates/net` (`cpe-net`)** — the composition layer that closes
the epic's remote loop in Rust, headless, with no frontend and no codegen. It depends on
`cpe-contract` + `cpe-server` + `cpe-security` and keeps `cpe-server` pure (no security/transport dep; the
one-way boundary points `cpe-net → {contract, server, security}`).

Files:
- `crates/net/Cargo.toml` — the crate + a `[[bin]] cpe-server-ref`. std-only sockets, no async runtime, no
  new heavy deps.
- `crates/net/src/wire.rs` — `read_envelope`/`write_envelope`: the CPE-811 `Envelope` framed as JSON-lines
  (the contract's `codec`) over any `Read`/`Write`. `Ok(None)` = clean EOF vs. an error for a malformed
  frame. 2 unit tests.
- `crates/net/src/server.rs` — `ServerRuntime` (dispatcher + `SecurityChain` + `Arc<dyn ServerCtx>`):
  `serve()` (thread-per-connection) and `handle()` (one connection). Runs the `Hello`/`Welcome` handshake
  via `negotiate()`, then evaluates **every** request through the security chain (Transport → AuthN →
  AuthZ) *before* dispatch; a denial becomes a structured `ContractError` and is never dispatched.
- `crates/net/src/client.rs` — `Client(Rust)` proxy: `connect`/`connect_as` (handshake, typed
  `ConnectError` on rejection) + `call(method, params) -> Result<Value, ContractError>`.
- `crates/net/src/lib.rs` — crate root + 7 end-to-end tests over an ephemeral loopback port: loopback
  browse, connection reuse, default-deny refusal, major-mismatch rejection, compatible negotiation,
  unknown-method `NotFound`, and the local-fast benchmark guard (in-process dispatch strictly faster than
  loopback, N=200, relative measurement).
- `crates/net/src/bin/cpe-server-ref.rs` — the deployable reference Server (`cpe-server-ref [ADDR]`).
- `.github/workflows/ci.yml` — added `cpe-net` to the `Server crates` 3-OS job (cache workspace + a
  `net — clippy + test` step) and refreshed the job comment to name all four crates.
- `docs/design/SERVER-ARCHITECTURE.md` — added `cpe-net` to the crate table, a new "The network transport
  loop" section (dispatcher + wire + runtime + client, the security-at-the-boundary flow, the local-fast
  guard, the reference binary), and moved the loop from "planned" to "shipped".

Verification (local, Windows): `cargo test` in `crates/net` → **9 passed**; `cargo clippy --all-targets -D
warnings` clean; `cargo build --bin cpe-server-ref` builds. The app is untouched (nothing depends on
`cpe-net`), so the local explorer path and its build are unaffected — the remote loop is purely additive.

Tradeoff / scope: this is the headless (cargo-verifiable) half of CPE-820, carved out so its ACs are met
without an attended GUI session. The GUI end-to-end path, a real non-loopback remote, and the
`src/lib/invoke.ts` transport seam stay in CPE-820 (gated on CPE-819 + attended verification). Security v1
is the local/passthrough chain; the remote AuthN/AuthZ/transport providers are config-driven and swap in
without touching `cpe-net` (CPE-817/818). Per-request evaluation (rather than per-connection handshake
auth) was chosen for v1 because it is both simpler and more thorough — every request is checked.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous). Estimate 3-4h. Surveyed the queue: Doing holds CPE-676
  (attended-only core refactor, left at its safe checkpoint); the three CPE-810 backlog children
  (813/819/820) all carry GUI-verified ACs and chain on CPE-812 (deferred) or CPE-819 (frontend GUI).
  Carved this headless-completable child out of CPE-820's Rust half rather than ship a half-done GUI ticket.
- 2026-07-21 — Read the three composing crates (contract/server/security) to design against their real
  APIs. Built `cpe-net`: wire framing, `ServerRuntime` (handshake + guarded dispatch), `Client` proxy,
  reference binary. First `cargo test` failed to compile — the major-mismatch test formatted `Ok(Client)`
  and `Client` isn't `Debug`; fixed by matching `Ok`/`Err` explicitly instead of `{other:?}`.
- 2026-07-21 — `cargo test` → 9 passed; `clippy --all-targets -D warnings` clean; `cpe-server-ref` builds.
  Wired `cpe-net` into CI's `Server crates` job (3-OS) and documented the loop in SERVER-ARCHITECTURE.md.
  All ACs met headlessly; closing.
