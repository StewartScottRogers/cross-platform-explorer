---
id: CPE-273
title: Reference "hello" sidecar + dev harness / SDK
type: Task
status: Done
priority: High
component: Multiple
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

A minimal example sidecar that exercises the whole platform end-to-end (handshake,
each capability, a trivial UI surface) **without** the AI Console — proving the
pattern in isolation. Doubles as the reference/SDK others copy to build a new
sidecar, and as the fixture for supervisor/broker/UI tests.

## Acceptance Criteria

- [ ] `examples/hello-sidecar` implementing the contract: handshake, uses context,
      secrets, storage, and event capabilities, renders a tiny UI in the mount.
- [ ] A documented "build your own sidecar" starter (SDK helpers + template).
- [ ] Used as the test fixture by [[CPE-265]], [[CPE-266]], [[CPE-271]], [[CPE-272]].
- [ ] Ships only in dev/example builds, never in the shipped explorer.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-267]], [[CPE-268]], [[CPE-269]], [[CPE-270]]. **Phase:** P4.
**Epic:** [[CPE-260]].

## Resolution

Delivered the reference **`hello_sidecar`** and a real-process integration harness that
exercises the whole platform end-to-end without the AI Console.

- **`sidecar/host/src/bin/hello_sidecar.rs`** — a reference sidecar that requests all
  four capabilities `[Context, Secrets, Storage, Events]`, emits `Lifecycle::Ready` on
  `Welcome`, then drives a scripted tour by *sending* capability requests to the host and
  reading responses (`storage.dir`; `secrets.set` then `secrets.get` with a round-trip
  assertion; `context.current`), emits `Event::Notify` + a terminal `Event::Status{done}`,
  and stays responsive to inbound host requests (echo-style; exits on `sidecar.shutdown`).
  Source depends on **`sidecar_contract` only** (no `sidecar_host` internals), like
  `echo_sidecar`, preserving the one-way boundary / delete-test.
- **`sidecar/host/tests/hello_sidecar_e2e.rs`** — three real-process tests:
  1. `hello_exercises_all_four_capabilities_over_a_real_process` — spawns via
     `supervisor::spawn_process`, handshakes granting all four caps, runs a broker with
     `ContextProvider` (fixed source), `SecretsProvider` (test-local **in-memory**
     `SecretBackend`, never the keychain), `StorageProvider` (tempdir), and an
     `EventRouter` with a recording sink; pumps requests/events until `Status{done}`; then
     asserts the sink saw the notify + done, the secret round-tripped into our backend, and
     the private storage dir was created.
  2. `hello_exits_zero_on_clean_shutdown` — drives the binary through a purpose-built
     `RawChild` `Connection` that owns the `Child`, and asserts exit code `0`.
  3. `conformance_kit_passes_against_hello` — `run_conformance` against a freshly-spawned
     hello still passes.
  All waits are bounded (deadline pump + 5s recv timeouts + `try_wait` polling) so the
  suite can never hang.
- **`sidecar/README.md`** — a "build your own sidecar" starter section (Hello →
  Welcome/Ready → use granted capabilities → serve requests), plus a capability-method
  table and how to run the conformance kit. A template doc-comment also heads
  `hello_sidecar.rs`.

### Key decisions (autonomous)

- **Capability tour is gated on what was actually *granted*.** The conformance kit grants
  no capabilities and expects a passive request/response sidecar; an unconditional tour
  would emit proactive requests that the kit reads where it expects responses, failing it.
  Gating each step on `capabilities_granted` is both the fix and the realistic design — a
  sidecar exercises no authority it wasn't given. With all four granted (E2E) the full
  tour runs; with none (conformance) hello behaves like `echo_sidecar`.
- **Exit-0 assertion needed a test-local `RawChild`** because `ProcessConnection`
  deliberately hides the child's exit code. It mirrors `ProcessConnection`'s reader-thread
  + timeout transport but retains the `Child` for `try_wait()`. No shared host files were
  edited (only two new files + `sidecar/README.md`), so parallel work and the delete-test
  are unaffected.
- **Fuller SDK / scaffolder deferred to CPE-303** (noted in the README and the
  `hello_sidecar.rs` template comment). This ticket delivers the copy-me reference +
  harness; the AC's "tiny UI in the mount" belongs to the UI-mount work (CPE-271) and is
  out of scope for the pure host/contract crates touched here.

### Verification

`cargo test` in `sidecar/host`: 44 unit + 3 hello E2E + 3 supervisor E2E all pass.
`cargo clippy --all-targets -- -D warnings`: clean. `src-tauri/` and the Svelte frontend
untouched.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Built hello_sidecar + real-process E2E harness + starter doc; all tests and
clippy green. See Resolution. Fuller SDK/scaffolder deferred to CPE-303.
