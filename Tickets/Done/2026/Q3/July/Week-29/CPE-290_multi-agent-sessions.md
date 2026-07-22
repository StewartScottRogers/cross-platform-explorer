---
id: CPE-290
title: Multi-agent sessions / tabs
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
estimate: 3-4h
created: 2026-07-13
---

## Summary

Run several agent sessions at once — tabbed/split consoles, each its own PTY,
agent, provider, and cwd — so you can compare agents or run one per repo
concurrently.

## Acceptance Criteria

- [ ] Multiple concurrent console sessions with independent PTYs and lifecycles.
- [ ] Tab/split UI to switch and manage sessions; per-session title (agent+model).
- [ ] Closing a session cleans up its process; no cross-session leakage.
- [ ] Bounded resource use; sessions survive pane hide/show.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-289]]. **Phase:** C5. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.

2026-07-13 (Dayshift) — Reconciled against shipped work. The bulk of this ticket was
delivered by the CPE-334/335/336 line:
- **Independent concurrent PTYs/lifecycles** — each Launch spawns its own `PtySession` on
  the sidecar (`console.rs`), which runs many at once. ✔
- **Tab UI to switch/manage; per-session title** — the tabbed multi-session dock (CPE-336):
  click to switch, × to close. ✔ Title now reads **agent · provider · model** (this pass —
  `launcher.html` `launch()` appends the model; omitted when blank, e.g. LM Studio pre-
  detection). ✔
- **Close cleans up the process; no cross-session leakage** — closing drops the PTY, which
  reaps the child; sessions are keyed independently. ✔
- **Bounded resource use; survive pane hide/show** — per-session bounded scrollback ring
  (`RING_CAP`, CPE-334) replays on reattach; pop-out window (CPE-335) preserves the session. ✔

Only the "agent+model" title detail was missing; added it here. `cargo build` (embeds
`launcher.html`) clean. Split-pane layout was listed as an alternative to tabs, not a
separate requirement — tabs satisfy "switch and manage". Closing as delivered. Done.
