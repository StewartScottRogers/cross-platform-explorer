---
id: CPE-061
title: Preview pane Phase 2 — wire PreviewPane into the app (asset protocol + read command)
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
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

Added a size-capped Rust `read_file_text` command (+ 3 unit tests, registered in `generate_handler!`),
enabled the Tauri asset protocol (`app.security.assetProtocol`, scope `**`) plus the required
`protocol-asset` cargo feature, and wired `PreviewPane` into the right pane with a persisted
Preview/Details toggle (`cpe.showPreview`). Images load via `convertFileSrc`, text via `read_file_text`
(256 KB cap).

Verified: frontend `npm run check` 0 errors, `npm test` 157 passed (incl. 3 preview integration tests),
`vite build` clean. Backend could not be built locally (no cargo on the dev machine), so it was gated
via **PR #1**: CI round 1 caught a missing `protocol-asset` feature (backend red on all 3 OSes); after
adding it, CI went fully green (cargo check + clippy + test on ubuntu/windows/macos) and the PR was
merged to `main` (merge commit 65a559d).

**Residual (human/GUI):** on-screen pane layout/appearance and real image/text rendering in the packaged
app — folds into [[CPE-053]]'s visual smoke-check. Markdown rendering + syntax highlighting remain a
later phase per [[CPE-059]].

## Work Log

2026-07-11 — Filed from the Nightshift after CPE-060. These integration points (asset protocol config, Rust read command, CSP) can't be verified locally here, so they are grouped for a CI-backed / live-verified pass.

## Notes

Markdown rendering and syntax highlighting are deliberately left to a later phase (bundle-size and
library choice per CPE-059 open questions). Related: [[CPE-059]] [[CPE-060]].
