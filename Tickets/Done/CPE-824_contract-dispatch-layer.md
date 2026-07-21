---
id: CPE-824
title: Server-side contract dispatch — route Request envelopes to cpe-server
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-810
estimate: 2-3h
---

## Summary
Child of CPE-810; the Server half of the remote RPC loop (CPE-820), buildable **headless now**. Add a
`cpe_server::dispatch` layer that turns a `cpe_contract::Request { method, params }` into a
`cpe_contract::Response { result }` by looking the method up in a registry of handlers, deserializing the
JSON `params`, calling the matching `cpe-server` domain function, and serializing the result — or returning
a structured `ContractError` (unknown method → `NotFound`, bad params → `BadRequest`, domain error →
`Internal`). This makes the contract **real end-to-end** (not just types) and is exactly what a network
Client(Rust) will drive over a socket in CPE-820 — with no transport yet, so it's fully unit-testable.

Adding a method = register a handler, no core changes (the same config-driven-registry shape as the
security core). Wire a representative set of methods to prove the pattern; the full 113-method surface is
completed alongside the typed bindings (CPE-812) so method names stay a single source of truth.

## Acceptance Criteria
- [x] `cpe_server::dispatch::Dispatcher` with `register(method, handler)` + `dispatch(&ctx, Request) ->
      Response`; a `ServerCtx` is threaded through so handlers can resolve dirs / emit.
- [x] Error taxonomy mapped: unknown method → `NotFound`, params that don't deserialize → `BadRequest`,
      a domain `Err(String)` → `Internal`; never a panic.
- [x] A representative set of methods registered (`list_dir`, `hash_file`, `text_stats`, `tags.load`,
      `tags.set`) with round-trip tests over `HeadlessCtx`.
- [x] Headless-testable; clippy clean both modes.

## Work Log
2026-07-21 — Built `cpe_server::dispatch`: a `Dispatcher` registry (`register`/`methods`/`dispatch`) over
`Handler = Fn(&dyn ServerCtx, Value) -> Result<Value, ContractError>`, with `params::<T>()` (deserialize →
`BadRequest`), `result::<T>()` (serialize → `Internal`), and `domain()` (domain `Err(String)` →
`Internal`) helpers. `dispatch(ctx, Request)` looks the method up (miss → `NotFound` response) and calls
the handler. `with_builtins()` wires a representative set spanning the shapes: path-arg no-ctx
(`list_dir`/`hash_file`/`text_stats`), no-arg ctx-using (`tags.load`), and multi-arg + ctx (`tags.set`).
Verified: `cpe-server` **112 tests** green (6 new: list_dir round-trip, unknown→NotFound, bad-params→
BadRequest, domain-err→Internal, tags set/load round-trip over `HeadlessCtx`, builtins-registered); clippy
`-D warnings` clean. App unchanged (this is the Server half of CPE-820, not wired to a transport yet) —
both app feature modes still clippy-clean.

## Resolution
Added the Server-side contract dispatch layer (`crates/server/src/dispatch.rs` + `pub mod dispatch;`) that
turns a `cpe_contract::Request { method, params }` into a `Response { result }` by looking the method up
in a `Dispatcher` registry and calling the matching `cpe-server` domain function — the exact loop a
network `Client(Rust)` will drive in CPE-820, minus the transport, so it's fully headless-testable. The
boundary error taxonomy (`NotFound`/`BadRequest`/`Internal`, never a panic) is enforced and tested. A
representative method set proves the pattern; the full ~113-method registration lands with the typed
bindings (CPE-812) so method names stay a single source of truth. No app changes — the transport seam
(CPE-819) + Client proxy/reference server (CPE-820) wire this into an actual RPC loop.
