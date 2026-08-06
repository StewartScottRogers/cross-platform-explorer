---
id: CPE-1360
title: "Archive inner-file preview: selecting a file inside a .zip/.rar shows an error, not its contents (virtual path never extracted)"
type: Bug
status: Backlog
priority: High
component: Multiple
tags: [ready]
epic: CPE-111
created: 2026-08-06
closed:
---

## Problem (reported v0.57.51; root-caused)

You can drill INTO a `.zip`/`.rar` (browse it like a folder), but selecting a file inside shows "can't
display" instead of its contents. Diagnosed root cause:

**Inner-file preview was never implemented.** When `archive` state is set, `archiveChildren` (`App.svelte`
~1522-1547) synthesizes `DirEntry` rows whose `path` is a **virtual, archive-relative string** (e.g.
`"notes/todo.txt"`), not a real filesystem path. On single-selection that entry is handed to `PreviewPane`
(`App.svelte:5081`), which picks a provider by extension and calls a backend loader (`readFileText`,
`read_image_data_url`, `read_preview_info`, `convertFileSrc`, …) with `entry.path`. That path **doesn't exist
on disk** (the bytes are inside the archive) → every loader returns `Err`/broken → "doesn't display
correctly." There is NO extract-to-temp step wired into preview; the only extract path
(`extractArchiveEntry`/`extractArchiveEntryAny`) is used solely by the double-click "open in external app"
flow, not by preview-on-select. Subfolder navigation works; **preview-follows-selection does not.**

Secondary (NOT a code bug): the archive-file entry TABLE is correct but the current samples are trivial
(zip = 1 file) so it looks empty — fixed by the substantial samples in CPE-1361.

## Fix

### Part A — extract-then-preview on inner selection (frontend; ZIP-family/tar/7z)
When `archive` is set and a single non-directory inner entry is selected, extract it to a temp file and give
`PreviewPane` a `DirEntry` whose `.path` is the **temp path** (keep the inner entry's `name`/`extension` so
provider selection still works). Reuse the existing `commands.extractArchiveEntry` (ZIP-family) /
`extractArchiveEntryAny` (tar/tgz/7z). Add request-id supersession + a cache keyed by `zipPath + inner`
(mirror the existing preview loaders) so rapid selection changes don't race, and clean up temp files. Change
sits at the `PreviewPane entry={…}` binding (`App.svelte:5081`) + a new "resolve inner selection → temp path"
reactive step near the archive helpers (~`App.svelte:1608`). Both `App.svelte` and `FloatPreview.svelte`
hosts. This fixes zip/jar/apk/war/ear/ipa/xpi/tar/tgz/7z inner-file preview (and, via the same temp path,
subfolder-nested files).

### Part B — RAR entry extraction (backend; the gap)
RAR has NO extractor today: `rar.rs` is list-only, and `extract_archive_entry_any` (`archive.rs:~260-282`)
misroutes `.rar` into the ZIP extractor (which fails). So RAR inner files can't preview OR open.
- Add a `rar_extract_entry(archive_path, inner_name) -> Result<Vec<u8> or temp path, String>` that extracts a
  **STORED** (uncompressed, method 0x30) RAR entry by copying its packed bytes (no decompression needed — pure
  Rust, no new deps). For a **compressed** entry, return a clear `Err` (e.g. "compressed RAR entries aren't
  supported for preview") — a real RAR decompressor is out of scope (proprietary/heavy; the non-free UnRAR lib
  and thin `rar` crate were both rejected earlier). The CPE-1361 substantial sample.rar uses STORED entries,
  so it previews; real compressed RARs degrade gracefully.
- Route `.rar` in `extract_archive_entry_any` to the new extractor (fixing external-open too), and wire it into
  Part A's extract-for-preview path. Add a `#[tauri::command]` + bindings regen.

## Acceptance criteria

- Inside a `.zip` (and tar/7z): selecting an inner file (incl. one in a subfolder) shows its real preview
  (text/image/etc.), not an error. A directory selection doesn't try to preview.
- Inside a `.rar`: selecting a STORED inner file previews its contents; a compressed entry shows a clean
  "can't preview" note (no crash); external-open of a stored RAR entry also works.
- Temp files are cleaned up; rapid selection changes don't show stale content (request-id guard).
- jsdom/vitest for the frontend resolve-and-preview wiring (mock extract → temp path → provider); cargo tests
  for `rar_extract_entry` (stored extracts; compressed → Err; malformed → Err no panic). `npm run check`,
  `cargo test`, clippy (both modes) green. Bindings regen (drift guard).
- Verified on a real build against the CPE-1361 substantial multi-folder zip + rar (attended/gui-smoke).

## Notes

Root-caused by the 2026-08-06 investigation. Pairs with CPE-1361 (substantial archive samples are the test
fixtures). Part A is the high-impact frontend fix; Part B closes RAR (stored) + fixes RAR external-open.
Epic CPE-111. Different files from CPE-1361 (parallelizable).
