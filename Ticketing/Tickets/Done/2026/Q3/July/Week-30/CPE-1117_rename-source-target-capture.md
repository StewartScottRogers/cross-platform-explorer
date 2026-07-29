---
id: CPE-1117
title: "Agent Watch: capture & emit rename source→target pairs"
type: feature
component: Backend
priority: medium
status: Done
tags: big-design
created: 2026-07-26
epic: CPE-730
---

## Summary
CPE-730 prerequisite for competing-rename detection (CPE-1118). Today the fs watcher classifies a rename as a
single-path `"renamed"` activity item (`{kind,path,actor}`) — there is **no from→to pairing** on the wire, so
`conflict_rename` divergence/collision cannot be computed. Capture the source→target pair in the watcher/pump
and add it to the activity item. BACKEND (+ a thin frontend type). Design + ground truth:
`.claude/research-library/entries/conflict-radar-close-plan.md` (Ticket B).

## Design (buildable)
- `src-tauri/src/lib.rs`:
  - `classify_fs_event` (~:4556) — handle `Modify(ModifyKind::Name(RenameMode::Both))`: `event.paths ==
    [from, to]` → emit one paired record.
  - `fs_activity_pump` (~:4590) — on Windows/Linux where notify splits into `RenameMode::From` then
    `RenameMode::To` (correlated by a tracker cookie), keep a small pending-rename map keyed by cookie and pair
    From+To within the existing 200ms coalesce window; unpaired leftovers fall back to today's single-path
    `renamed`.
  - `flush_fs_batch` (~:4643) — carry optional `from`/`to` through the flush (extend the coalesce value from a
    `&'static str` kind to a small struct, or a side-channel `HashMap<to, from>` drained here). Keep `actor`
    resolution unchanged.
- `src/lib/sidecar.ts` (`FsActivity` ~:152) — add optional `from`/`to` (or `oldPath`) to the `renamed` item,
  defaulting absent on non-pairing platforms. Regen `bindings.gen.ts`.

## Decide-and-log (Foreman)
- **Graceful platform degradation is acceptable, not a user-stop.** Some Linux notify backends never emit
  `RenameMode::Both`; where a pair can't be formed, degrade to today's single-path `renamed` (no crash, no
  false pair) and document the fidelity ceiling here. The 3-OS CI matrix is available to prove behaviour.

## ⚠ Guardrails / risk
- **HIGH risk:** cross-platform `notify` rename semantics differ per OS — MUST pass the 3-OS `sidecar-platform`
  CI (`cargo test` + `clippy -D warnings` both feature modes). No new deps.
- **Conflict surface = `src-tauri/src/lib.rs` pump/flush region (same #413 hot region as actor-tag work) +
  `sidecar.ts` + bindings.** Do NOT run concurrently with any other `lib.rs` pump/flush ticket, and only ONE
  bindings-regenerating backend build in flight at a time.
- Off-means-off + single-session cost unchanged.

## Acceptance Criteria
- [ ] A rename inside a watched tree emits a `renamed` item carrying both `from` and `to` where the platform
      provides them; Windows From/To split events are paired within the flush window.
- [ ] Unpaired events degrade to single-path `renamed` (no crash, no false pair); `actor` tagging + off-means-off
      unchanged; single-session cost unchanged.
- [ ] `cargo test` + `cargo clippy --all-targets -D warnings` (both feature modes) green on 3-OS CI; no new deps.

## Tests
- Extend `classify_fs_event` tests (`lib.rs` ~:6548+) for `RenameMode::Both/From/To`.
- A pump-level pairing test: cookie-correlated From+To → one paired record; orphan → single-path.

## Work Log
2026-07-26 (workshift) — Filed from the CPE-730 close plan. Prerequisite for CPE-1118 (competing-rename fold).
Dispatched to an opus worker (genuinely-hard cross-platform slice). Fidelity-ceiling degradation decided-and-
logged (not a user-stop) — 3-OS CI proves it.

2026-07-26 (workshift) — Built (PR #433, merged 80601b45). Reviewer APPROVE + UAT PASS: cookie-correlated From/To pairing + RenameMode::Both, every non-pairable case (Any/Other, cookieless, orphan From, never-arriving To, in-window cookie reuse) degrades to single-path `renamed` with NO wrong pair; off-means-off + no map leaks + coalesce dedup integrity all preserved; clippy+22 tests green both feature modes. NOTE for CPE-1118/replay owners: a PAIRED rename now writes ONE audit-journal row (target only) — the from/to pair is LIVE-ONLY (on the fs-activity emit), not persisted in the CPE-1108 audit journal. CPE-1118 consumes the live stream so it's unaffected; replay-of-renames would need a separate AuditEvent.from/to field (out of scope here).
