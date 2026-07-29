---
id: CPE-574
title: "Swarm sessions are never adopted into the console — no tabs appear when a swarm runs"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 2-4h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Running a swarm shows *"Swarm started — its agents launch in their own tabs…"* but **no tabs/sessions
appear**. Root cause: `SwarmDriver` launches each session by calling `engine.launch()` **directly** and
consuming its output on a private worker thread (`swarm_live.rs`), so the session is **never registered
with the console** — it never gets `adopt_session`'d, never inserted into `ConsoleState.sessions`, and
never emits the Agent-Watch `started`/`ended` announcements that surface a session in the UI. The
endpoint's own comment (*"progress/end surface through the normal per-session Agent-Watch
announcements"*, `console.rs`) describes wiring that was never built — this is the flagged live tail of
[[CPE-541]] / [[CPE-528]].

Compounding: `SessionIo::take_output()` is **single-consumer**, so the driver and the console can't both
read the stream. The driver must hand session ownership to the console instead of consuming it.

## Fix (this ticket)
Introduce a `SessionRunner` seam so the driver launches *through* the console:
- **`swarm_live.rs`** — new `pub trait SessionRunner { fn launch(task_id, agent_id, launch, done) }`.
  `SwarmDriver` holds an `Arc<dyn SessionRunner>` instead of a bare `Arc<dyn SessionEngine>`. The old
  direct-launch + PTY-read logic moves verbatim into `EngineRunner` (the default, used by the headless
  unit tests — so all 4 real-subprocess tests stay green).
- **`console.rs`** — `SwarmSessionRunner` adopts each launched session into the console: announces
  `started`, wires the session through the shared reader pipeline (`adopt_into`, extracted from
  `adopt_session`) so it streams live + records history + announces `ended`, and observes completion via
  an `on_end` hook (no contention for the single-consumer output channel). A `try_wait` watcher kills the
  PTY on the agent's self-exit so the reader EOFs on **Windows ConPTY** (where a held master won't EOF).
  `ConsoleState.sessions` becomes `Arc<Mutex<…>>` so the background mission thread can insert sessions
  (all `.lock()` sites unchanged).
- **`handle_swarm_run`** builds a `SwarmSessionRunner` from the console's shareable field Arcs and passes
  it to the driver.

Result: swarm agents become **real console sessions** — visible + live-streaming + interactive +
recorded — exactly like an interactively launched agent. They surface in **Agent Watch** (the
announcement-driven left pane).

## Out of scope / follow-on
- **Launcher tab-strip**: the `launcher.html` tab strip is *client-initiated* (a tab is created from a
  `/api/launch` response id). It does not auto-adopt server-created sessions, so swarm agents show in
  Agent Watch but not the launcher's own tab row. Surfacing them there (poll or announce their ids to the
  launcher and open `/api/session/<id>/ws` per agent) is a separate frontend follow-up.
- **GUI QA**: real-agent visual verification is still the CPE-541 / CPE-528 live tail — headless build +
  unit tests validate compilation + the pure loop, not the on-screen result.

## Acceptance Criteria
- [x] `SwarmDriver` no longer calls `engine.launch` directly; it launches via `SessionRunner`.
- [x] `EngineRunner` preserves the existing behaviour; all `swarm_live` unit tests pass unchanged.
- [x] A swarm session is inserted into `ConsoleState.sessions` and emits `started` + `ended`.
- [x] `cargo build` + `cargo clippy --all-targets -D warnings` clean; sidecar test suite green
      (284 lib + 1 e2e passed locally — Defender did not block this run).
- [x] Adoption is **tested**, not just asserted: a unit test drives a real session through
      `SwarmSessionRunner` and checks it's inserted into `sessions` + announces `started`→`ended` +
      reports completion (`console::a_swarm_session_is_adopted_into_the_console_and_announced`).
- [~] GUI QA (real-agent visual): folded into [[CPE-582]] — headless adoption is now proven; the on-screen
      confirmation with a real agent rides the CPE-582 smoke.

## Work Log
2026-07-17 — Filed + picked up. Root cause confirmed by reading `swarm_live::try_launch` (direct
`engine.launch` + private reader) vs. `console::adopt_session` (the only path that registers a session +
announces it).
2026-07-17 — **Implemented the `SessionRunner` seam.**
- `swarm_live.rs`: `pub trait SessionRunner` + `EngineRunner` (the old direct-launch + PTY-read logic,
  moved verbatim — cross-platform completion detection intact); `SwarmDriver` now holds
  `Arc<dyn SessionRunner>`; `try_launch` delegates. `Completion` made `pub`.
- `console.rs`: extracted the reader pipeline into free fn `adopt_into(...)` (adds an `on_end` hook);
  `adopt_session` delegates with `on_end=None`. New `SwarmSessionRunner` adopts each swarm session
  (announce `started` → `adopt_into` streams/records → `on_end` reports completion to the driver), with a
  `try_wait` watcher that kills the PTY on self-exit to drive ConPTY to EOF. `sessions` is now
  `Arc<Mutex<…>>` so the mission thread can insert. `handle_swarm_run` builds the runner from the
  console's field Arcs and passes it to the driver.
- `launcher.html`: success copy now says agents appear in **Agent Watch** (accurate — see out-of-scope).
- Verified: `cargo build` ✓; `clippy --all-targets -D warnings` ✓; `cargo test --lib` **284 passed / 0
  failed**; `cargo test --test swarm_end_to_end` **1 passed**. Updated both `SwarmDriver::new` call sites
  in tests (`swarm_live` helper + `swarm_end_to_end.rs`) to wrap `LocalEngine` in `EngineRunner`.
- **Remaining:** GUI QA (real-agent run, live in Agent Watch) — the CPE-541/CPE-528 live tail; not
  headlessly verifiable. Not committed (awaiting user).

2026-07-17 (validated + committed from the CLI) — The above was left uncommitted in the working tree by
the desktop surface. Picked it up on the user's "work CPE-574": reviewed the `SessionRunner` seam +
`adopt_into`/`SwarmSessionRunner` (sound; layered cleanly on the CLI's CPE-541/584 changes — nothing
clobbered), then **found and fixed a real Windows bug** it introduced:

- **Bug:** an adopted session holds `io` in the `Session` map forever, so `SwarmSessionRunner`'s ConPTY
  watcher `kill()`ed the child but the master stayed open → on **Windows ConPTY the output reader never
  EOFs** → `on_end` never fires → the mission stalls (the driver never advances). `EngineRunner` avoided
  this only because it *owns and drops* `io`; the console can't (it keeps the session for interactivity).
  Passed on Unix, would hang on the user's Windows box — exactly what a real GUI run would have shown.
- **Fix (`pty.rs`):** `PtySession.master` is now `Option`; `kill()` drops it, **closing the ConPTY** so
  the reader EOFs even while the console still holds the session. Idempotent; `reader`/`writer`/`resize`
  return "session terminated" after kill.
- **Test:** added `console::a_swarm_session_is_adopted_into_the_console_and_announced` — drives a real
  session through the runner and asserts adoption + `started`→`ended` + completion. It **timed out (20s)
  before the fix, passes in ~0.1s after** — proof the fix is load-bearing.

Verified: `cargo test` **285 lib passed** + `swarm_end_to_end` 1 passed; `clippy --all-targets -D
warnings` clean.

## Resolution
Swarm sessions are now **adopted into the console** as real, live, recorded sessions that surface in
Agent Watch (announced `started`→`ended`, streamed, history-recorded), via a `SessionRunner` seam so the
headless driver path (`EngineRunner`) is unchanged. The desktop surface built the seam; the CLI validated
it, fixed the Windows-ConPTY completion bug that would have stalled every real run, added the adoption
test, and landed it. Files: `swarm_live.rs` (SessionRunner/EngineRunner), `console.rs`
(adopt_into/SwarmSessionRunner + test), `pty.rs` (kill closes the ConPTY), `swarm_end_to_end.rs` (wrap
LocalEngine in EngineRunner). The one remaining piece — watching it on screen with a real agent — rides
[[CPE-582]].
