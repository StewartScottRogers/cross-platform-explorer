# ADR 0001 — Sidecar platform architecture

**Status:** Accepted (foundation) · **Date:** 2026-07-13 · **Tickets:** CPE-259 (this
ADR), CPE-260 (platform epic), CPE-261 (AI Console epic).

## Context

Cross-Platform Explorer is a fast, small, predictable file explorer. We want to host
large, fast-moving "Mega-Features" — the first being an **AI Console** (an agentic
CLI manager + embedded terminal) — without letting them compromise the core app.
These features will be huge and will need continuous extension to track the coding-
agent market. A tightly-coupled feature would ripple change through the explorer and
could never keep pace.

## Decision

Mega-Features are built as **standalone sidecar processes** hosted by the explorer
through **one small, versioned contract**. The explorer ships a generic *sidecar
platform*; each Mega-Feature is a *tenant* sidecar. This is a platform, not a
one-off: adding a future Mega-Feature is adding a sidecar, not editing the host.

## The five principles

1. **Standalone-capable** — a sidecar can run as its own app; the explorer is one host.
2. **Built for hugeness** — growth is additive; the platform absorbs it.
3. **Zero ricochet** — a change lands only where it belongs.
4. **Plugin-but-more-flexible** — a decoupled subsystem behind a stable contract.
5. **No entanglement / ultra-clear boundaries** — one thin, explicit seam.

## Invariants (non-negotiable)

- **One-way dependency.** Sidecars depend only on the `sidecar-contract` crate; they
  never import explorer internals, and the explorer never imports a sidecar's
  internals. Enforced by CI lint (CPE-272).
- **Process isolation.** Each sidecar is its own OS process = its own crash domain,
  memory space, storage scope (CPE-269), and secrets scope (CPE-268).
- **No ambient authority.** A sidecar gets nothing by default; it *requests*
  capabilities, the user/host *grants* scoped ones (CPE-266, CPE-296).
- **No cross-sidecar visibility.** Sidecars cannot discover or talk to each other
  except through explicit host-brokered channels (CPE-275).
- **The contract is the only throttle.** Small, additive, semver'd, version-negotiated
  at handshake (CPE-263).

## Key decisions

| Topic | Decision | Ticket |
|-------|----------|--------|
| Isolation model | Out-of-process **sidecar** | CPE-259 |
| UI model | Each sidecar **serves its own UI**; host embeds it sandboxed | CPE-271 |
| IPC transport | Decided by the Phase-0 spike (length-prefixed frames over stdio vs local socket; must stream + backpressure) | CPE-294 |
| Trust model | Bundled manifests are first-party; **user/third-party manifests are untrusted executable content** needing provenance + consent before any command runs | CPE-295 |
| Versioning | Contract, sidecar manifests, agent manifests, stored state, and credential profiles **all** carry a schema version with a migration path | CPE-263, CPE-300 |
| Distribution | **Sidecars are first-party and always BUNDLED with the app — never downloaded at runtime.** They ship inside the app install and update only when the app updates. This removes the fetched-binary path entirely: no separate download, no OS code-signing of sidecars needed (they inherit the signed app's trust), no independent update/rollback of binaries. Market pace comes from updating *agent manifests* (data), not sidecar binaries. The binary bundles as a plain resource copied in place; each single-file bundle entry names an explicit destination path (CPE-320). | CPE-276, CPE-320 |

## The enforcement rule — the delete-test

> Remove every sidecar → the explorer still builds, ships, and runs
> fast/small/predictable. Remove one sidecar → nothing else notices.

This single test is the boundary's guarantee, wired into CI (CPE-272). It is the
build/coupling analogue of PURPOSE.md's "with a mode off, the explorer stays
fast/small/predictable."

**Implemented as:** the integration lives behind the `src-tauri` Cargo feature
`sidecar-platform`, **off by default**, so the plain explorer compiles and ships with no
sidecar code. CI proves both halves — the default `backend` job is the delete-test
(builds/tests with zero sidecars), a feature build proves the integration compiles, and
a grep guard fails if any sidecar crate ever depends on the explorer app (the one-way
rule).

## Quality bars & budgets

- **Startup:** zero measurable cost with all sidecars disabled.
- **Footprint:** documented per-sidecar memory ceiling, supervisor-enforced (CPE-297).
- **Security:** passes the end-to-end threat model / review (CPE-304); secrets never
  on disk in plaintext, never logged, never in a webview (CPE-268).
- **Observability:** every sidecar failure is diagnosable from host-side logs
  (CPE-298) and surfaced with an actionable error (CPE-299).
- **Testability:** a sidecar proves compliance via the conformance kit (CPE-301); the
  platform has an E2E harness (CPE-302).

## Non-goals

- Not a general plugin marketplace / sandboxed-WASM runtime.
- Not sidecar-to-sidecar orchestration in v1 (all coordination via the host).
- The explorer does not gain awareness of any specific sidecar's domain.

## Relationship to Agent Watch

Agent Watch (`AGENT-WATCH.md`) **observes** filesystem activity and has "no control
surface." The AI Console **drives** an agent. They are complementary, distinct
surfaces and should integrate (CPE-305): launching an agent in the console can light
up Agent Watch on that repo.

## Glossary

- **Host** — the explorer, running the sidecar platform.
- **Sidecar** — an isolated process implementing the contract.
- **Tenant** — a Mega-Feature realised as a sidecar (e.g. the AI Console).
- **Contract** — the `sidecar-contract` crate: the only shared surface.
- **Capability** — a scoped, brokered permission a sidecar requests.
- **Manifest** — declarative data describing a sidecar (or, inside a tenant, an agent).

## Consequences

- Strong isolation and a genuine standalone story, at the cost of IPC and a second
  runtime per active sidecar — acceptable because sidecars are opt-in modes.
- The contract becomes the most important, most conservatively-evolved artifact in
  the program.
- Two hard gates guard the line: the **delete-test** (boundary) and the **security
  review** (trust).
