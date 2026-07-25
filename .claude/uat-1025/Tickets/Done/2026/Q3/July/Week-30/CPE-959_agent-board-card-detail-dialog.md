---
id: CPE-959
title: Agent Board — click a ticket/epic card to open a details popup
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
closed: 2026-07-23
created: 2026-07-23
epic: CPE-503
---

## Summary
On the Agent Board, clicking a ticket card (or an epic card) should open a popup dialog showing the full
ticket/epic — as many details as reasonable: the frontmatter fields (id, title, type, priority, status,
tags, epic, sprint, its folder) plus the rendered markdown body (Summary, Acceptance Criteria, Work Log,
Notes, …). Read-only view; closing returns to the board.

## Acceptance Criteria
- [x] Backend `board_card_detail(root, id)` — reuses `find_ticket_file` (recursive, covers Epics/Done/**),
      returns `CardDetail { id, location, fields, body }`. Registered + in the typed `collect_commands!`.
- [x] Pure `ticket_board::detail_from(md)` — ordered `(key,value)` frontmatter pairs + body; 2 tests.
- [x] `CardDetailDialog.svelte` — themed popup w/ visible border; id + title header, a fields table, and the
      body via `renderMarkdown` (sanitized); loading / not-found / error states; epic gets a "View tickets →"
      button (dispatches `drill`).
- [x] Ticket cards + epic cards open the dialog on body-click (role/keydown for a11y); copy-id / dispatch
      buttons `stopPropagation` so they still work; drag unaffected.
- [x] `npm run check` 0/0; vitest **929 pass**; `cargo test` detail_from (2) + app 67 + clippy clean;
      regenerated `bindings.gen.ts` (typed `commands.boardCardDetail`).
- [ ] GUI-verify in the installed 0.57.24: clicking a card shows its full details. *(attended, done together)*

## Notes
Reuse `renderMarkdown` (src/lib/preview/markdown.ts, marked+DOMPurify). Register the command in
`generate_handler!` + `collect_commands!` and regenerate `bindings.gen.ts`
(`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`). Dialogs get a visible
border (per the dialogs-need-visible-border convention).

## Agent Directives
_Machine-readable: an agent acts on an `open` directive, appends a reply, then flips it to `done`._

### ▸ open · to `any` · 2026-07-24T01:48:22.842Z
Summarize the risks in this ticket
