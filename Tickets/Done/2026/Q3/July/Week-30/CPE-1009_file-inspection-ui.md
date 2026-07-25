---
id: CPE-1009
title: File Inspection panel — surface the detectors in Properties
type: feature
component: Multiple
priority: medium
tags: ready
status: Done
created: 2026-07-24
closed: 2026-07-24
epic: CPE-1002
---

## Summary
First **UI** for the CPE-1002 file-inspection suite (user chose "build the UIs", 2026-07-24). Surfaces the
per-file detectors in the existing **Properties** dialog: a file's true text **encoding**, its **line
endings**, its **true type** (from magic bytes), and a **content/extension mismatch warning** (a disguised
file). Attended — the visual result is verified by the user in the running app.

## What was built
- **Backend composition** `cpe-server::inspect`: `FileInspection { encoding, line_endings, file_type,
  type_mismatch }` + `inspect_bytes(name, bytes)` composing `text_encoding` + `file_type`. Pure, 5 tests.
- **Tauri command** `inspect_file(path)` (src-tauri): reads a 64 KiB leading sample + the name, calls
  `inspect_bytes`; thin `spawn_blocking` dispatcher; registered in **both** `generate_handler!` and
  `collect_commands!` (the CPE-968 rule); typed binding `commands.inspectFile` regenerated.
- **Frontend** `PropertiesDialog.svelte`: auto-loads the inspection for a single file (best-effort, like the
  image metadata) and renders Encoding / Line endings / File type rows + a red **⚠ mismatch** row.
- **i18n** `prop.encoding` / `prop.lineEndings` / `prop.fileType` / `prop.typeMismatch` added to all **12**
  locales (parity guard green).

## Verification
- `cargo test --lib inspect` (5), clippy clean; bindings regenerated (`export_bindings` compiles the app).
- `npm run check` → 0 errors; `vitest` i18n parity (34) + PropertiesDialog (5) green.
- **Attended:** the user opens Properties on a file to see the rows, and on a disguised file (a `.jpg` that's
  really a PNG) to see the ⚠ warning.

## Acceptance Criteria
- [x] `inspect_file` command + `cpe-server::inspect` composition, tested; bindings generated.
- [x] Properties dialog shows encoding / line-endings / true-type rows + a mismatch warning; i18n in 12
      locales; type-check + i18n guard green.
- [x] **Visual verify (attended):** rows render correctly; the mismatch warning shows on a disguised file.
      — **User-confirmed 2026-07-24** on the running v0.57.33 (sidecar) build: Properties Encoding/Line-endings/
      File-type rows render, and the ⚠ content/extension mismatch shows on a disguised file ("looks good").

## Notes
- Folder-level checks (empty-folder / orphaned-sidecar / near-dup-folder / dangling-links) are a later UI
  slice. This is the file-level inspection surface.
