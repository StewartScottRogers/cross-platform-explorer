---
id: CPE-061
title: Preview pane Phase 2 — wire PreviewPane into the app (asset protocol + read command)
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Second increment of [[CPE-059]], building on [[CPE-060]] (which delivered the provider registry and a
self-contained `PreviewPane` with injected file access). This ticket wires the pane into the running
app and supplies the real file access — the parts that could not be verified on the Nightshift machine
(no Rust toolchain, no live GUI) and so need building/running in CI and a human/automation check.

## Acceptance Criteria

- [ ] Right pane offers a Preview/Details toggle; Preview is the default when a single file is selected
- [ ] A persisted setting enables/disables the preview pane (mirror `showDetails` in `settings.ts`)
- [ ] Image previews load via `convertFileSrc` with the Tauri **asset protocol enabled** in
      `tauri.conf.json` (`app.security.assetProtocol`, scoped) and any needed capability entry
- [ ] Text/markdown previews load via a new Rust `read_file_text(path, maxBytes)` command with a size
      cap, registered in `generate_handler!` and permitted in `capabilities/default.json`
- [ ] CSP allows the asset protocol and inlined preview assets only (no external requests)
- [ ] Rust `cargo test` for the read command (cap enforced, unreadable files error cleanly) — green in CI
- [ ] jsdom integration test: selecting an image/text file shows the preview in the pane
- [ ] `npm run check` clean; full JS suite green

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-11 — Filed from the Nightshift after CPE-060. These integration points (asset protocol config, Rust read command, CSP) can't be verified locally here, so they are grouped for a CI-backed / live-verified pass.

## Notes

Markdown rendering and syntax highlighting are deliberately left to a later phase (bundle-size and
library choice per CPE-059 open questions). Related: [[CPE-059]] [[CPE-060]].
