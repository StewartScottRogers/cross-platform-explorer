---
id: CPE-801
title: Session-history browser + filtered export UI
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-733
estimate: 3-4h
---

## Summary
The UI for epic CPE-733: browse past sessions (from the CPE-800 journal), filter events, and export the
selection to JSON/CSV/Markdown (CPE-799), with redaction options for sensitive content.

## Acceptance Criteria
- [x] Browse/filter past sessions; export filtered events to JSON/CSV/Markdown.
- [x] Redaction option for exported content; menus follow MENUS.md.
- [x] check + suite green; GUI-verified.

## Resolution
Built `src/lib/components/SessionHistoryDialog.svelte` — the browser UI over the already-tested logic:
loads sessions (`audit_sessions`), reads a session's events (`audit_read`), filters them by kind
(checkboxes) + path substring (`filterEvents`), optionally redacts the home dir (`redactEvents`, toggle),
and exports the current selection to JSON / CSV / Markdown (`toJson`/`toCsv`/`toMarkdown`). Export content
is dispatched to App, which saves via the existing `saveFileDialog` + `write_file_text` flow (mirrors the
tags export). Opened from the command palette ("Session history…", `tool.sessionHistory`);
`palette.sessionHistory` added to all 12 locales.

**GUI-verified in the running dev app (CDP):** seeded two sessions via `audit_record` (also exercising the
CPE-800 backend end-to-end) → opened the dialog from the palette → both sessions listed, the last
auto-selected showing its events with formatted timestamps + kinds → kind filter (`removed` → 1 event) and
path filter (`.ts` → 2, `proj` → 3) narrowed correctly → switching sessions reloaded (alpha 3 / beta 2
events) → export buttons enable with events and disable when the filter empties → redact toggle present.
(Export *content* correctness is covered by the `auditExport` unit tests; the Export buttons were not
clicked in CDP because they open a native save dialog that would block the automation.) Seeded test
sessions were deleted from the real journal afterward. `npm run check` clean; full suite 862 green.

## Notes
Prereq: CPE-799, CPE-800. Attended GUI.

## Work Log
- 2026-07-20 (nightshift) — Picked up. The whole data path already exists and is tested: CPE-800 backend
  (`audit_sessions` / `audit_read`) + CPE-799 `auditExport.ts` (`filterEvents`, `toJson/toCsv/toMarkdown`).
  The only missing *logic* piece for the ACs was **redaction** (AC2) — everything else is the navigable
  Svelte view (a thin render) that needs a running app to verify. Built the redaction core headlessly and
  deferred the view to attended GUI work, matching this session's foundation-first pattern.
- 2026-07-20 (nightshift) — Added `redactEvents(events, opts)` + `RedactOptions` to
  `src/lib/auditExport.ts`: collapse the home-dir prefix to `~` (separator-aware, case-insensitive,
  anchored so `alicebob` ≠ `alice`), mask literal usernames → `<user>`, apply custom regex patterns →
  `<redacted>` (invalid patterns skipped, not thrown), over `path` and (opt-out) `detail`. Pure, never
  mutates input; compose as filter → redact → export. 4 vitest cases (home collapse incl. non-match,
  username+pattern in path/detail + no-mutation, `redactDetail:false` + invalid-regex skip, empty-opts
  no-op + undefined-detail preserved). Full suite 9/9; `npm run check` clean.
- 2026-07-20 (nightshift) — **Deferred.** Redaction core (AC2 logic) + the data path are done and
  headlessly green; the remaining scope is the **navigable session-history view** — list sessions, pick
  one, render/filter events, an export menu (MENUS.md) with a redaction toggle — a thin render over the
  now-complete logic, but it needs the running app for AC3's "GUI-verified".
  - *deferred-on:* the attended GUI view + its GUI verification (this ticket is tagged "Attended GUI").
  - *revisit-when:* an attended session — build the Svelte view wiring `audit_sessions`/`audit_read` →
    `filterEvents` → `redactEvents` → exporters, add its `Section`→doc-slug entry + `src/docs/*.md` page
    (CPE-579 guard), and GUI-verify. No external gate; pickable anytime.

## Resolution (partial — redaction core landed, GUI view deferred)
Landed the one missing pure-logic piece: `redactEvents` + `RedactOptions` in `src/lib/auditExport.ts`,
which privacy-redacts events before export (home→`~`, usernames→`<user>`, custom patterns→`<redacted>`,
across path + optional detail; invalid patterns skipped; input never mutated). With this, the entire
CPE-801 data + logic path (browse → filter → redact → export) is complete and unit-tested; only the
attended Svelte view + GUI verification remain. Deferred with a turnkey revisit note.
