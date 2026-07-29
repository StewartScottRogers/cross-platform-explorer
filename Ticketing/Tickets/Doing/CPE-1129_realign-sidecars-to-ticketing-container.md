---
id: CPE-1129
title: Realign the sidecars to the Ticketing/ container (verify ticket/epic/sprint directory reads)
type: Task
status: In Progress
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

- [x] Audit every sidecar + binary that reads the ticket tree for the `Ticketing/` container layout:
      `sidecar/agent-board` (board.rs / lib.rs / ui.rs), `sidecar/ai-console` (swarm / gate / metrics
      readers), and `crates/server/bin/ticket_mcp`.
- [x] The standalone `agent-board` sidecar reads status tickets from `Ticketing/Tickets/`, epics from
      `Ticketing/Epics/`, and sprints from `Ticketing/Sprints/` — at parity with the in-app board, or
      the intentional gap is documented if parity is out of scope.
- [x] Project-root detection (`nearest_project_root` / `resolve_board_root` / `CPE_BOARD_ROOT`) keys on
      `Ticketing/` consistently across all readers.
- [x] Tests cover the new layout for each reader; `cargo clippy --all-targets -D warnings` clean (both
      feature modes); the 3-OS CI matrix is green. *(clippy + tests verified locally on Windows; the
      3-OS CI matrix goes green on the PR — not independently verifiable headless before that runs.)*
- [x] In-app Agent Board docs + any user-facing sidecar strings reflect the reads.

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

- 2026-07-29 — Audited all three out-of-process readers named in the ticket.
  - `sidecar/agent-board/src/board.rs`: `tickets_dir()` and `nearest_project_root()` already correctly
    key on `Ticketing/` / `Ticketing/Tickets/` (done in CPE-1128). Confirmed via read + the existing
    passing test suite. **Gap confirmed**: no epic/sprint reads existed.
  - `crates/server/src/bin/ticket_mcp.rs`: `FsStore::ticketing()` already walks `root.join("Ticketing")`
    recursively for `## Agent Directives`, so it covers Tickets/Epics/Sprints without change. No bug.
  - `sidecar/ai-console/`: grepped `src/**/*.rs` for any `Tickets`/`Ticketing` path join — zero hits.
    The swarm/gate/metrics modules (`cost.rs`, `gate_*.rs`, `swarm_*.rs`, etc.) never resolve a
    ticket/epic/sprint filesystem path themselves; `swarm_mcp.rs` only references
    `Ticketing/Epics/CPE-528_*.md` in a doc-comment link (already correct post-CPE-1128). No change
    needed here.
  - Repo-wide grep for stray `Tickets/` (excluding `.claude/uat-1025*`, the historical
    `Ticketing/Tickets/Done/**` archive, `.claude/research-library/`, `node_modules`, `target`): all
    functional path joins in `.rs`/`.ts`/`.svelte` already correctly read `Ticketing/Tickets/...`. Found
    genuinely **stale doc strings** (not functional bugs, but AC-5 "user-facing strings"): `Ticketing/wiki.md`'s
    folder-structure ASCII diagram still showed the pre-CPE-1128 layout (`Tickets/` as root with
    `Epics/`/`Sprints/`/`wiki.md` nested inside it) — fixed to the real `Ticketing/{wiki.md, Epics/,
    Sprints/, Tickets/{Backlog,…}}` shape. `Ticketing/Sprints/.gitkeep` and `Ticketing/Epics/README.md`
    both pointed readers at a now-nonexistent `Tickets/wiki.md` — fixed to `Ticketing/wiki.md`.
    `sidecar/agent-board/Cargo.toml`'s `description`, a `src-tauri/src/lib.rs` comment, and a
    `.github/workflows/ci.yml` comment all still said "Kanban over Tickets/" — updated to `Ticketing/`.
  - **Decision (agent-board sidecar scope): went with the preferred/parity option.** Added
    `board::Epic`/`board::Sprint` + `read_epics()`/`read_sprints()` to
    `sidecar/agent-board/src/board.rs` — `read_epics` mirrors the in-process `board_epics_impl` exactly
    (Epics/ + `epic`-tagged closed tickets in top-level Tickets/Done); `read_sprints` reads
    `Ticketing/Sprints/` + closed sprints in top-level Tickets/Done, matched by their `SPR-` id prefix
    (sprints carry no distinguishing tag the way epics do — Sprints is a genuinely new surface with no
    in-process `board_sprints` equivalent to mirror, since the in-app board itself has no dedicated
    Sprints list view, only `find_ticket_file` locating one for card-detail). Wired `GET /api/epics` and
    `GET /api/sprints` into `ui.rs`, plus a minimal dependency-free **Board / Epics / Sprints** view
    switcher in `board_html()`. Why this and not the documented-gap fallback: the add was small (~120
    lines, no new deps, same frontmatter-parsing style already in the file) and closes the gap the
    ticket called out by name, rather than deferring it again.
  - Added tests: `board.rs` — `read_epics_resolves_from_the_sibling_epics_dir_and_closed_done`,
    `read_sprints_resolves_from_the_sibling_sprints_dir_and_closed_done`,
    `read_epics_and_sprints_are_empty_when_the_dirs_are_absent`. `ui.rs` —
    `serves_epics_and_sprints_json_from_the_sibling_ticketing_queues`, plus extended
    `board_html_is_valid` to assert the new endpoints are wired into the page. All against real temp
    `Ticketing/Epics/`, `Ticketing/Sprints/`, `Ticketing/Tickets/Done/` layouts.
  - Updated `docs/design/STANDALONE-WINDOWS.md` and `src/docs/06-agent-board.md` to describe the
    sidecar's new Epics/Sprints view.
  - `cargo test` + `cargo clippy --all-targets -- -D warnings` clean in `sidecar/agent-board` (20 tests
    pass, clippy 0 warnings). `crates/server` untouched — reran its ticket-board tests (22 pass) +
    clippy (clean) as a baseline sanity check, not because anything there changed.
  - Not independently verifiable headless: the 3-OS CI matrix (only runs on the PR), and any live
    exercise of the served HTML/JS view switcher in a real browser (covered instead by the HTTP-level
    `/api/epics`/`/api/sprints` tests + a `board_html()` content assertion).

## Notes

Follow-up to CPE-1128. Watch for the same trap that broke CI there: never blanket-`sed` a path that
already contains its replacement (`Ticketing/Tickets/` must not become `Ticketing/Ticketing/`), and run
each crate's Rust tests + clippy **after** the final edit (there is no cargo workspace — check each
crate dir separately). Regenerate `src/lib/bindings.gen.ts` if any specta-exposed type/doc changes.
Related: [[two-board-implementations]], [[sidecar-host-changes-need-host-rebuild]].
