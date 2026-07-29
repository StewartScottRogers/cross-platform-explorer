---
id: CPE-1129
title: Realign the sidecars to the Ticketing/ container (verify ticket/epic/sprint directory reads)
type: Task
status: Open
priority: High
component: Backend
estimate: 2h
created: 2026-07-29
closed:
tags: [ready]
---

## Summary

CPE-1128 restructured `Tickets/` into a `Ticketing/` container: the status-flow queue in
`Ticketing/Tickets/`, with `Epics/` and `Sprints/` lifted to **sibling** queues under `Ticketing/`.
The in-process Agent Board (`src-tauri` `board_*` + `crates/server`) and the standalone
`sidecar/agent-board` **status-column** reads were updated in that change, and `bin/ticket_mcp` now
walks the whole `Ticketing/` tree.

This ticket **audits and completes** the realignment across **every** out-of-process component that
reads the ticket tree, so the shipped/installed build (which bundles the sidecars — see
[[always-install-sidecar-build]], [[two-board-implementations]]) is fully consistent with the new
layout. Known gap: the standalone `agent-board` sidecar reads only the status columns today — it does
**not** surface Epics (`Ticketing/Epics/`) or Sprints (`Ticketing/Sprints/`) from their new sibling
locations, so it lags the in-app board's Epics view. Any `ai-console` swarm/gate/metrics code that
resolves ticket, epic, or sprint directories must also be verified against the new paths.

## Acceptance Criteria

- [ ] Audit every sidecar + binary that reads the ticket tree for the `Ticketing/` container layout:
      `sidecar/agent-board` (board.rs / lib.rs / ui.rs), `sidecar/ai-console` (swarm / gate / metrics
      readers), and `crates/server/bin/ticket_mcp`.
- [ ] The standalone `agent-board` sidecar reads status tickets from `Ticketing/Tickets/`, epics from
      `Ticketing/Epics/`, and sprints from `Ticketing/Sprints/` — at parity with the in-app board, or
      the intentional gap is documented if parity is out of scope.
- [ ] Project-root detection (`nearest_project_root` / `resolve_board_root` / `CPE_BOARD_ROOT`) keys on
      `Ticketing/` consistently across all readers.
- [ ] Tests cover the new layout for each reader; `cargo clippy --all-targets -D warnings` clean (both
      feature modes); the 3-OS CI matrix is green.
- [ ] In-app Agent Board docs + any user-facing sidecar strings reflect the reads.

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

Follow-up to CPE-1128. Watch for the same trap that broke CI there: never blanket-`sed` a path that
already contains its replacement (`Ticketing/Tickets/` must not become `Ticketing/Ticketing/`), and run
each crate's Rust tests + clippy **after** the final edit (there is no cargo workspace — check each
crate dir separately). Regenerate `src/lib/bindings.gen.ts` if any specta-exposed type/doc changes.
Related: [[two-board-implementations]], [[sidecar-host-changes-need-host-rebuild]].
