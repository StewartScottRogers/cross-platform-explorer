---
id: CPE-064
title: Preview pane — list ZIP archive contents (CPE-063 child)
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Child of [[CPE-063]], now doable locally because Rust is installed on this machine (backend
`cargo test` verified). Add a preview provider that lists the entries of a ZIP archive without
extracting it: a Rust `read_archive_entries` command (using the `zip` crate) plus an `archive` preview
kind that renders the entry list.

## Acceptance Criteria

- [ ] Rust `read_archive_entries(path)` returns each entry's name, size, and is_dir, without extracting
- [ ] Registered in `generate_handler!`; unit test creates a zip and asserts the listing
- [ ] `archive` provider matches `.zip`; `PreviewPane` renders the entry list (name + size)
- [ ] Frontend gets entries via an injected `loadEntries` accessor (mockable in tests)
- [ ] `cargo test` + `cargo clippy -D warnings` green locally; `npm run check` + JS suite green
- [ ] CI (push to main) stays green cross-OS

## Resolution

Added the `zip` crate + a `read_archive_entries(path)` Rust command (lists name/size/is_dir from the
zip directory, no extraction), registered in `generate_handler!`, with 2 unit tests (lists a created
zip; errors on a non-zip). Added an `archive` provider (`.zip`) + `ArchiveEntry` type, and a
`PreviewPane` archive branch rendering the entry list (name + size) via an injected `loadEntries`
accessor, wired in `App` to `read_archive_entries`.

Verified locally now that Rust is installed: `cargo test` green (incl. the two new tests),
`cargo clippy --all-targets -D warnings` clean, `npm run check` 0 errors, JS suite 172 passed,
`vite build` clean. Merged to `main`; push-to-main CI confirms cross-OS.

**Residual (human/GUI):** the archive list's on-screen appearance (folds into [[CPE-053]]). Remaining
CPE-063 items (markdown render, syntax highlighting, Office) are unaffected.

## Work Log

2026-07-11 — Rust installed locally (rustup stable 1.97.0, msvc), backend cargo test verified (34 passed). Picking up the zip-listing piece of CPE-063 that previously needed CI to build.

## Notes

ZIP only for now (`zip` crate). Other formats (7z/rar/tar) would need separate crates — out of scope.
Markdown render / syntax highlighting / Office remain in [[CPE-063]].
