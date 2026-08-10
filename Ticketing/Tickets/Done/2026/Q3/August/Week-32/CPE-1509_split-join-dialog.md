---
id: CPE-1509
title: "File split/join dialog + context-menu entries (consume the CPE-1491 backend)"
type: Feature
status: Backlog
priority: Low
component: Frontend
tags: [ready]
parent: CPE-1491
created: 2026-08-08
---
## Context
CPE-1491 built the headless backend for file split/join — the classic orthodox-commander utility
(Total Commander / Multi Commander staple) for chunking a large file into fixed-size numbered parts
(`.001`, `.002`, …) plus a small JSON manifest, and rejoining them back into the original with a
SHA-256 verify. The backend half is done and shipped: two Tauri commands
(`cpe_server::split_join::split_file` / `join_files`, bounded/streamed both directions, no new
dependency) available via `src/lib/bindings.gen.ts`:

```ts
type SplitManifest = {
  originalName: string; totalSize: number; partCount: number; partSize: number; sha256: string;
}
async splitFile(path: string, partSize: number, outDir: string): Promise<Result<SplitManifest, string>>
async joinFiles(firstPartOrManifest: string, outPath: string): Promise<Result<null, string>>
```

This ticket is the **frontend half only**: wire those two commands into a dialog + context-menu entries.
Least differentiating item from the competitive-landscape survey (honest framing carried over from
CPE-1491) — a CLI/script does the same job — so keep this small and skip it over higher-value queue items
if the queue is contested.

## Scope
- **Split dialog:** triggered from a file's context menu ("Split file…"). Lets the user pick a part size
  (common presets — e.g. 1.44 MB floppy, 650 MB CD, 4 GB FAT32, plus a free-entry MiB/GiB field) and an
  output folder (path picker — every path/folder field needs a native Browse dialog per house convention,
  not just typing). Calls `splitFile`; on success show the manifest summary (part count, per-part size,
  output folder) or route through the transfer/progress surface if that fits better for a multi-part write.
- **Join dialog:** triggered from a `.NNN` part file's (or a `.split-manifest.json`'s) context menu ("Join
  parts…"). Lets the user pick/confirm the output path (path picker, pre-filled with the manifest's
  `originalName` in the same folder as a sensible default) and calls `joinFiles`. Surface the backend's
  refusal-to-overwrite as a clear message (it errors rather than silently clobbering an existing
  `out_path` — see CPE-1491's Work Log) with an explicit "replace existing" choice only if product wants
  one; otherwise just surface the error and let the user pick a different path or delete the target first.
- **Context-menu entries:** follow docs/design/MENUS.md (item text `var(--text)`, no hard-coded colours,
  icon per docs/design/MENU items convention). "Split file…" appears on a single selected non-empty
  regular file. "Join parts…" appears on a selected `.NNN` part or `.split-manifest.json` file.
- **Error surfacing:** every backend error (`part_size == 0`, part-count cap, missing/corrupt part,
  checksum mismatch, already-exists) must reach the user as a readable message, not a swallowed promise
  rejection — these are exactly the cases CPE-1491's 12 backend tests exercise, so the dialog's job is
  just to not lose that signal.

## How
- Frontend: two dialogs (or one dialog with a Split/Join mode toggle, if that reads cleaner in the
  existing dialog system) + context-menu entries. Every dialog/overlay needs a clearly-visible thin border
  per house convention, not just a shadow.
- Busy-cursor convention: call `splitFile`/`joinFiles` through `invoke` from `src/lib/invoke.ts` (never
  `@tauri-apps/api/core` directly) per BUSY-CURSOR.md — both are real (if bounded, streamed) I/O work.
- If the source file is large, consider whether the dialog should show its own progress rather than
  relying solely on the busy cursor (split/join of a multi-GB file streams but is still synchronous from
  the frontend's perspective as written) — decide and note in the Work Log; a spinner-only wait is
  acceptable for a first cut given this ticket's Low/small scope, but say so explicitly rather than by
  omission.
- In-app docs (CPE-579): this adds a user-facing feature — add/update its page in `src/docs/*.md` and its
  `section → doc slug` entry in `src/lib/sectionDocs.ts` (the guard test `src/lib/sectionDocs.test.ts`
  fails CI otherwise).

## Verify
`npm run check`. gui-smoke can exercise the dialogs once relevant (split a small fixture file, confirm
part files + manifest appear; join them back, confirm the reconstructed file; missing-part and
already-exists error paths surface a visible message, not a silent failure). Manual/GUI-verify: build →
install the sidecar build → run the real app (never `tauri dev`) and confirm split-then-join round-trips a
real file via the context menu end to end.

## Notes
Backend engine + its own 12-test suite (`cargo test` in `crates/server`, module
`crates/server/src/split_join.rs`) already shipped in CPE-1491 — do not re-litigate the manifest shape,
the bounded/streamed I/O, the overwrite-refusal policy, or the part-cap/hostile-manifest guards here; this
ticket only consumes the existing `splitFile`/`joinFiles` commands. If the overwrite-refusal UX turns out
too blunt for real use (e.g. product wants an in-dialog "replace" confirmation instead of a bare error),
that's fine to build client-side (delete-then-retry) without needing a backend change — the commands
already refuse cleanly either way. Parent CPE-1491.
