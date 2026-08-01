---
id: CPE-1168
title: "Automated click-through for the standalone agent-board sidecar UI (retire MVD row #9)"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
QA-Architect / PM-scouted (2026-07-31). `MANUAL-TEST-BURNDOWN.md` row #9 — the standalone **`agent-board`
sidecar UI** click-through — is still manual but is **genuinely headless-automatable**: the sidecar serves a
loopback HTTP UI (no user, no creds), so a WebDriver/`gui-smoke`-style drive can launch it and click each
Board / Epics / Sprints view button and assert the list swaps.

## Build
- Add an automated click-through (extend `gui-smoke` or a focused harness) that: launches the `agent-board`
  sidecar (find its bin/launch under `sidecar/agent-board` + how it's normally started; it serves loopback
  HTTP), drives its UI (Chromium/WebDriver against the loopback URL), clicks each top-level view
  (Board/Epics/Sprints), and asserts the rendered list changes per view. Snap a screenshot per view for the
  Visual Critic if cheap.
- Time-bounded + non-blocking; tear the sidecar down after. No user, no network beyond loopback.

## Acceptance Criteria
- [x] A headless run launches the agent-board sidecar, clicks Board/Epics/Sprints, and asserts each view's
      list renders/swaps; tears the sidecar down cleanly.
- [x] Flips burndown row #9 to automated + names the pinning job; `npm run check` / relevant checks green.

## Work Log
- 2026-07-31 — **Built** `sidecar/agent-board/clickthrough.mjs`: a zero-dependency Node harness that
  launches the built agent-board sidecar, speaks the ADR-0001 stdio handshake to reach `Ready` (emits
  `Hello` → harness replies `Welcome` → sidecar announces `Status { ui:http://127.0.0.1:<port> }`),
  points **headless Edge** (`msedgedriver`, raw WebDriver HTTP — no WebdriverIO/tauri-driver, no
  `npm install`) at that loopback URL, clicks each view (Board/Epics/Sprints), asserts the list swaps,
  and snaps a screenshot per view into `.screenshots/` (gitignored). Sidecar + browser torn down in a
  `finally`. Pointed at the repo's real `Ticketing/` via `CPE_BOARD_ROOT`, so all three views carry
  real content.
- **Found + fixed a real bug (the harness's payoff).** First run passed on the `hidden` DOM *property*
  but the per-view screenshots showed the Kanban columns *still on screen* under the Epics list. Root
  cause: `switchView` toggles the `hidden` attribute, but `.cols`/`.list { display:flex }` are AUTHOR
  rules that override the UA `[hidden]{display:none}` rule — so a "hidden" pane never actually hid; it
  only *looked* right when a pane was empty (zero height). Strengthened the harness to assert **real
  computed visibility** (`getComputedStyle().display !== 'none'`), which then correctly failed, and
  fixed the UI by adding `[hidden] { display: none !important; }` to `src/ui.rs`'s `board_html()`.
  Pinned with a new assertion in the `board_html_is_valid` Rust test so the fix can't silently regress.
- **Verify:** `node sidecar/agent-board/clickthrough.mjs` → `PASS — all three views drove and the list
  swapped per view` (Board: 5 columns / 42 cards; Epics: 38 rows; Sprints: 0 rows → empty-state, which
  is correct — no `SPR-*.md` exist). Screenshots confirm the swap visually. `cargo test` (agent-board)
  20 passed; `cargo clippy --all-targets -D warnings` clean. Flipped burndown row #9 → ✅ (MVD 8→7),
  documented the harness in `sidecar/agent-board/README.md`.
- **Not wired into CI this shift** (bounded scope): needs Edge + `msedgedriver` on the runner, same as
  `gui-smoke/`. A follow-up can add it to `.github/workflows/gui-smoke.yml`. Noted in the burndown row +
  README.

## Notes
- Lowest priority of this shift's batch — dispatch only if crew budget/disk allows after CPE-1166/1167.
  Epic CPE-579. Two board implementations exist ([[two-board-implementations]]) — this pins the standalone
  sidecar one specifically.
