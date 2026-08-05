---
id: CPE-1332
title: "In-app docs: pages for Declutter, near-duplicate cleanup, and Metadata Studio (CPE-579 guardrail)"
type: docs
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-579
---

## Summary
Three user-facing Tools features shipped recently with NO in-app documentation, violating the self-maintaining-
docs guardrail (CPE-579 / CLAUDE.md "In-app docs are self-maintaining"):
- **Declutter** (CPE-1329) — new Tools dialog, no doc page.
- **Near-duplicate cleanup** (CPE-1324) — the docs/folders near-dup dialog gained safe Move-to-Bin cleanup;
  `src/docs/18-similar-images.md` covers *similar images*, not this dialog.
- **Metadata Studio batch/revert** (CPE-1326/1327) — the studio doc (if any) predates the new batch ops +
  per-field revert + checkpoint-before-save.

## Build
- Add/extend `src/docs/*.md` pages for these features. Check `src/lib/sectionDocs.ts` (the single source of truth
  mapping `Section → doc slug`) + the existing `src/docs/` numbering to decide new-page vs extend-existing:
  - **Declutter:** a new page (e.g. `src/docs/23-declutter.md`) — what it finds (empty files, installers,
    temp/partial downloads, backups), that nothing auto-deletes, selection + Move-to-Bin (recoverable) +
    snapshot-first safety.
  - **Near-duplicate cleanup:** extend the relevant page (or a new one) to document the keeper-guarded Move-to-Bin
    for near-duplicate documents/folders (can't delete every copy in a group; snapshot-first).
  - **Metadata Studio:** ensure a page documents checkpoint-before-save (undo), batch Strip / Copy-from-first,
    and per-field revert / Reset-all.
- **If any of these is a new `Section`,** add its `section → slug` entry in `src/lib/sectionDocs.ts` so the
  `sectionDocs.test.ts` guard passes (every Section mapped, every slug exists in DOCS). If they're reachable
  from an existing documented section, extending that page is fine — match the guard's expectations.
- Keep the prose consistent with the existing docs library voice.

## Acceptance criteria
- `npx vitest run src/lib/sectionDocs.test.ts` passes (no unmapped section, no missing slug).
- `npm run check` clean; the new/updated pages render in the in-app Documents library.
- Each of the three features has accurate, plain-language documentation reachable in-app.

## Notes
- FRONTEND/docs-only — merge on the Frontend CI job. No backend.
- Conflict surface: `src/docs/*.md` (new + edited), `src/lib/sectionDocs.ts`. Isolated from the gui-smoke work
  in CPE-1331. Reference: `src/docs/22-file-health.md` (recent example) + `src/lib/sectionDocs.ts`.
