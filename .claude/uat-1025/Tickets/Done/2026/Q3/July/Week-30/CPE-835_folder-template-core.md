---
id: CPE-835
title: Folder-template core — capture + token substitution + stamp
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-740
estimate: 2h
---

## Summary
First child of CPE-740 (folder templates & scaffolding). The pure + real-fs core of the feature, as a
Tauri-free `cpe-server::folder_template` module:

- **`Template { name, nodes }`** with `Node::Dir { name, children }` / `Node::File { name, contents }` — a
  serde-JSON tree (import/export is just this JSON).
- **`capture(folder, name)`** — walk a folder into the model; small UTF-8 files (≤64 KB) keep their
  contents as boilerplate, larger/binary files become empty placeholders; unreadable entries are skipped.
- **`substitute(s, vars)`** — replace `{key}` tokens from a variable map in folder names, file names, and
  file contents; unknown `{tokens}` pass through verbatim.
- **`stamp(template, dest, vars)`** — create the structure under `dest` with substitution, sanitizing each
  name to a single path component (no separators / `..`, so a token can't escape `dest`) and refusing to
  overwrite an existing file; returns the created paths.

Fully cargo-tested with a real capture→stamp round-trip on the running OS. Persistence + Tauri commands
(CPE-836) and the gallery/UI (CPE-837) build on this.

## Acceptance Criteria
- [x] `Template`/`Node` serde model round-trips through JSON.
- [x] `capture` reproduces a folder's structure (nested dirs + files), captures small text file contents,
      and yields empty placeholders for oversized/non-UTF-8 files.
- [x] `substitute` replaces known `{key}` tokens (in names + contents) and leaves unknown tokens verbatim.
- [x] `stamp` creates the substituted structure, is path-safe (a token value with `/` or `..` cannot escape
      `dest`), and refuses to overwrite an existing file.
- [x] End-to-end round-trip test: capture a temp tree → stamp elsewhere with vars → structure + substituted
      names/contents match.
- [x] `cargo test` green in `crates/server` (144 passed); `cargo clippy --all-targets -D warnings` clean.
      App untouched.

## Resolution
Added **`cpe-server::folder_template`** — the capture/stamp core of folder scaffolding:

- `Template { name, nodes }` + `Node::Dir/File` — a serde-JSON tree (import/export is the JSON).
- `capture(folder, name)` — walks a folder into the model; small UTF-8 files (≤64 KB) keep contents, larger
  or non-text files become empty placeholders, unreadable entries are skipped (list_dir spirit); a
  non-folder is an error.
- `substitute(s, vars)` — `{key}` replacement from a variable map in names + contents; unknown `{tokens}`
  pass through verbatim; a dangling `{` is literal.
- `stamp(template, dest, vars)` — creates the substituted structure; **path-safe** (each name sanitized to a
  single component — separators → `_`, bare `..` neutralised — so a token value can't escape `dest`) and
  **non-destructive** (refuses to overwrite an existing file); returns created paths.

Files: `crates/server/src/folder_template.rs` (impl + 6 tests), registered in `lib.rs`. No new dependency.

Verification (local, Windows): the **capture→stamp round-trip runs against the real filesystem** — build a
temp source tree, capture it, stamp to a fresh dir with `{name}` substitution in folder names + file
bodies, assert the structure/contents; plus the traversal-safety test (a `../escaped` token value stays a
single component under `dest`) and no-clobber. `cargo test` in `crates/server` → **144 passed** (was 138);
`cargo clippy --all-targets -D warnings` clean. App untouched (CPE-836 adds persistence + Tauri commands;
CPE-837 the gallery/UI).

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous) after activating epic CPE-740. Estimate 2h. Built the
  capture + substitute + stamp core with path-safety + no-clobber; verified the full round-trip against the
  real filesystem on Windows. 6 tests; full suite 144 green; clippy clean. Closing.
