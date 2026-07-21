---
id: CPE-851
title: Agent Board sidecar — crate foundation (handshake + loopback UI + manifest)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-850
estimate: 2-3h
---

## Summary
Foundation of the out-of-process Agent Board (CPE-850). The scaffolded `sidecar/agent-board` crate, made
a compliant, running sidecar on the `repos` template: it emits `Hello`, reaches `Ready` on `Welcome`,
answers `sidecar.shutdown`, and serves its **own** dependency-free loopback HTTP UI (a board-branded page
to start, real data is CPE-852), announcing the URL via a `ui:<url>` Status event. Depends only on
`sidecar-contract` (ADR 0001). `sidecar.json` names it "Agent Board" with a `local_port` UI. Added to CI's
3-OS `sidecar` job.

## Acceptance Criteria
- [x] `sidecar/agent-board` builds; a `lib.rs` holds the pure protocol (`hello`, message handling) with
      `main.rs` a thin stdio wrapper (mirrors `repos`).
- [x] On `Welcome` it emits `Lifecycle::Ready`, starts the loopback UI server, and announces `ui:<url>`.
- [x] It serves an HTML page (board-branded placeholder) on an ephemeral 127.0.0.1 port; `sidecar.shutdown`
      exits cleanly.
- [x] `sidecar.json`: `name` "Agent Board", `ui: { local_port, announced }`; depends only on the contract.
- [x] cargo-tested (protocol handshake + the UI HTML), added to CI's `sidecar` job; clippy clean.

## Work Log

## Resolution
Scaffolded `sidecar/agent-board` (via `create_sidecar`, CPE-303) and made it a compliant, running sidecar
on the `repos` template — depends only on `sidecar-contract` (ADR 0001 / CI one-way guard):

- `src/lib.rs` — pure protocol: `hello()` (announces id `agent-board` + the `context` capability),
  `on_message()` → `Reaction` (`Welcome`→Ready, `sidecar.shutdown`→Shutdown, else ack). 4 unit tests.
- `src/ui.rs` — the dependency-free loopback HTTP server (mirrors `repos`) + `board_ui()` (a board-branded
  placeholder; real Kanban is CPE-852). 2 tests (serves over loopback; valid HTML).
- `src/main.rs` — thin stdio wrapper: emit Hello; on Ready serve the UI + announce `ui:<url>`.
- `sidecar.json` — `name: "Agent Board"`, `ui: { local_port, announced }`, `capabilities: [context]`.
- `.github/workflows/ci.yml` — added `agent-board` to the 3-OS `sidecar` job (cache + clippy/test step).

Verification (local, Windows): `cargo test` → **6 passed**; `cargo clippy --all-targets -D warnings`
clean. The one-way guard holds (only `sidecar-contract` + `serde_json`). CPE-852 puts the real board data
in; CPE-853 wires host launch; CPE-854 bundles it so it appears in Settings.

## Work Log
- 2026-07-21 — Scaffolded the crate, customized to the repos template (lib protocol + loopback UI + thin
  main), manifest named "Agent Board" with a local_port UI, added to CI. Fixed a test (`Welcome` has no
  `Default` — constructed it). 6 tests + clippy clean. Closing.
