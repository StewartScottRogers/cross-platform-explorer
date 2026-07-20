---
id: CPE-801
title: Session-history browser + filtered export UI
type: feature
status: Deferred
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-733
estimate: 3-4h
---

## Summary
The UI for epic CPE-733: browse past sessions (from the CPE-800 journal), filter events, and export the
selection to JSON/CSV/Markdown (CPE-799), with redaction options for sensitive content.

## Acceptance Criteria
- [~] Browse/filter past sessions; export filtered events to JSON/CSV/Markdown.
      *(All logic exists & tested — CPE-800 `audit_sessions`/`audit_read` backend + CPE-799 `filterEvents`/`toJson`/`toCsv`/`toMarkdown`; the navigable view that wires them is the attended GUI tail.)*
- [~] Redaction option for exported content; menus follow MENUS.md.
      *(**Redaction core landed & tested** — `redactEvents` in auditExport.ts; the toggle/menu wiring is the GUI tail.)*
- [~] check + suite green; GUI-verified.
      *(`npm run check` clean + vitest green now; **GUI-verified** is the attended part.)*

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
