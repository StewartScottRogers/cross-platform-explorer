---
id: CPE-059
title: File preview pane (right side) with a pluggable, bundled provider architecture
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Add a preview pane on the right side of the window that renders a live preview of the currently
selected file, for as many file types as possible. The preview logic should be structured as a set of
**preview providers ("plugins")** behind a common interface, chosen at runtime by file type. We ship
with every provider we have built in (no third-party/dynamic loading in v1 — "plugin" here means an
internal, registered provider module, which keeps the architecture open without the security surface of
executing external code).

This extends the existing right-side surface: the app already has a `DetailsPane` (metadata, toggled
with Alt+P). The preview pane should share that region — decide whether preview replaces, tabs with, or
sits above the metadata (see Open Questions).

## Research goals (do first, record findings in the Work Log)

1. **Provider architecture** — define a `PreviewProvider` interface, e.g.
   `{ id, label, canPreview(entry): boolean | score, render(target, fileRef): Promise<Cleanup> }`,
   plus a registry that picks the best provider for a selection (by extension/category/MIME, highest
   score wins, with a metadata-only fallback). Lazy-load heavy providers so startup stays fast.
2. **File access from the webview** — how to feed bytes/text to a provider safely: Tauri asset
   protocol (`convertFileSrc`) for images/media, a Rust `read_file_text`/`read_file_bytes` command
   with a size cap for text/binary, and streaming for large media. Respect the CSP.
3. **Performance & safety** — size limits, cancellation when the selection changes rapidly, memory for
   large files, never block the UI thread, sandbox untrusted content (e.g. HTML/SVG) so a preview
   can't run arbitrary script or make network calls.
4. **Survey the file-type landscape** and prioritise providers by value vs. effort.

## Target file types (as many as feasible; phase them)

- **Text/code** — plaintext, Markdown (rendered), source code with syntax highlighting, JSON/YAML/TOML
  (pretty-printed), CSV (table), logs.
- **Images** — png/jpg/jpeg/gif/webp/bmp/ico/svg (svg sandboxed); consider heic/avif/tiff support.
- **PDF** — first-page/scrollable render.
- **Audio/Video** — inline `<audio>`/`<video>` for web-playable formats (mp3/wav/ogg/m4a, mp4/webm),
  with a graceful "no inline preview" for the rest.
- **Archives** — list entries (zip at least) without extracting.
- **Office/OpenDocument** — investigate feasibility; likely metadata-only or a later phase.
- **Fallback** — the existing metadata view for anything without a richer provider.

## Acceptance Criteria

- [ ] A right-side preview pane, toggleable, that updates on selection change and integrates with the existing DetailsPane region
- [ ] A documented `PreviewProvider` interface + registry with best-match selection and a metadata fallback
- [ ] Providers shipped bundled; heavy ones lazy-loaded
- [ ] Backend file-read command(s) with a size cap; media via the asset protocol; CSP respected
- [ ] Phase 1 providers at minimum: images, text/code (highlighted), Markdown, and the metadata fallback
- [ ] Selection changes cancel in-flight previews; large files are capped, not hung on
- [ ] Untrusted content (SVG/HTML) is sandboxed — no script execution, no network
- [ ] Each provider is unit-testable (pure "canPreview"/selection logic) with jsdom coverage for the pane wiring
- [ ] A user setting to enable/disable the preview pane, persisted (like showDetails)
- [ ] `npm run check` clean; Rust `cargo test` green in CI; full JS suite green
- [ ] Docs: how to add a new provider (README or a short PROVIDERS.md)

## Suggested phasing

- **Phase 1 (MVP):** pane + registry + interface; image, text/code (syntax highlight), Markdown providers; metadata fallback; enable/disable setting.
- **Phase 2:** PDF, audio/video, SVG (sandboxed), CSV table, JSON/YAML pretty-print.
- **Phase 3:** archive listing, Office/OpenDocument investigation, HEIC/AVIF/TIFF, thumbnails for the list view.

Split each phase into its own child ticket when work starts, so it lands incrementally.

## Open Questions (resolve during design; pick sensible defaults if unattended)

- Preview vs. metadata: replace the DetailsPane, tab with it, or stack? (Default: a toggle within the
  right pane switching between Preview and Details, preview default when a file is selected.)
- Bundle-size budget for syntax-highlight/markdown/pdf libraries, and which libraries (all inlined,
  CSP-safe, no CDN).
- Which syntax highlighter (e.g. Shiki vs highlight.js) given the no-external-request constraint.

## Progress

- **Phase 1 — [[CPE-060]] (Done, 2026-07-11):** `PreviewProvider` registry + `pickProvider` + a
  self-contained `PreviewPane` component (image + text/markdown + fallback), fully unit/jsdom-tested
  with file access injected. No backend/config yet.
- **Phase 2 — [[CPE-061]] (Done, 2026-07-11):** wired the pane into the app; asset protocol + `protocol-asset`
  feature for images; Rust `read_file_text` (size-capped) for text; Preview/Details toggle + persisted
  setting. Merged via PR #1 after CI verified the Rust across all three OSes.
- **Phase 3a — [[CPE-062]] (Done, 2026-07-11):** audio, video, PDF, JSON pretty-print, CSV table.
  (SVG already previews as an image via the asset protocol.)
- **Phase 3b — [[CPE-063]] (Open):** markdown rendering, syntax highlighting, archive listing (needs
  Rust), Office investigation, list-view thumbnails. Blocks final epic closure.

**Epic complete (2026-07-11).** All phases landed: registry + pane (CPE-060), app wiring + asset
protocol + Rust text read (CPE-061), media/PDF/JSON/CSV (CPE-062), ZIP archive listing (CPE-064),
code highlighting + sanitized markdown (CPE-065). Office preview declined and list-view thumbnails
deferred as documented decisions in [[CPE-063]]. Residual = live/visual verification only, tracked in
[[CPE-053]].

## Notes

"Plugin" is interpreted as an internal provider registry (bundled), not runtime-loaded third-party
code — this matches "ship with every plugin we have" while avoiding the security/complexity of dynamic
code loading. A future ticket could add true external plugins if ever wanted. Relates to the disabled
"Gallery" affordance in the Sidebar and the existing `DetailsPane`/`showDetails` toggle.
