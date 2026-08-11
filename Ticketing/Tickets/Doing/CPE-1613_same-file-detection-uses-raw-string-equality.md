---
id: CPE-1613
title: "Batch media decides \"is this the same file?\" by raw string equality — a JPG→jpg convert overwrites the original on Windows"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found by the independent reviewer on CPE-1599 (PR #812) while probing the new engine-side overwrite guard.
**Not a regression from that PR** — the flaw is in the definition of "same file" that both the new guard and
the *pre-existing* non-destructive guarantee have always shared. It is filed High because the worked example
below is an ordinary thing to do on the user's primary platform, and it destroys an original in the mode
that promises not to.

## The bug
`any_in_place` (`crates/server/src/batch_execute.rs:129`) and `overwritesInPlace`
(`src/lib/batchMedia.ts:126`) both decide whether an operation overwrites its input with **raw string
equality**: `it.input == it.output`.

Worked example, entirely mundane:
- Input `IMG_1.JPG`, operation **Convert → jpg**, "write to new files" **off**.
- `plan()` lower-cases the extension (`batch_media.rs:176`), so `output = "IMG_1.jpg"`.
- `"IMG_1.JPG" != "IMG_1.jpg"` → the guard does **not** fire, no confirmation is required, and the engine
  does not refuse.
- On Windows and default macOS the filesystem is **case-insensitive**: that write lands on the same file.
  The original is gone, with no confirmation and no checkpoint.

The same string comparison also misses: symlinks and junctions pointing at one underlying file, trailing
separators, `.`/`..` segments, 8.3 short names, and Unicode normalisation differences (NFC vs NFD, which
macOS produces routinely).

## Why it matters more than it looks
`plan()`'s **non-destructive** mode has always used the identical comparison to keep its "output != input"
promise (the `used` collision set at `batch_media.rs:154-230`). So the gap is not only in the new guard —
**the safe mode itself can silently overwrite an original** on a case-insensitive filesystem. A user who
never unticks the box, and therefore never sees a confirmation, can still lose the file.

## Fix
Canonicalize path comparison **once**, in one shared helper, and use it for both:
1. `plan()`'s non-destructive "output must differ from input" guarantee, and
2. the `confirmed_overwrite` refusal check.

They must share a single definition of "same file" — fixing one and not the other just moves the hole.
Consider what canonicalization is right per platform: at minimum case-folding on case-insensitive
filesystems and normalising separators/`.`/`..`; ideally resolving to the same underlying file
(`std::fs::canonicalize`, or comparing file identity where the OS exposes it) so symlinks and junctions are
caught too. Beware that canonicalize fails for a path that doesn't exist yet — the output usually won't —
so canonicalize the parent and compare the final component appropriately.

## Acceptance criteria
- `IMG_1.JPG` + Convert→jpg with "write to new files" **on** produces a genuinely different file, or refuses.
- The same with the box **off** requires confirmation (the guard fires) rather than silently overwriting.
- Symlink/junction, trailing-separator, `.`/`..` and case variants of the same file are all treated as the
  same file by both call sites.
- Tests cover each of those on Windows; note in the work log which cases can't be exercised on the CI matrix.

## Notes
Conflict surface: `crates/server/src/batch_media.rs`, `batch_execute.rs`, `src/lib/batchMedia.ts` and their
tests. Related: [[CPE-1599]], [[CPE-1590]]. Model: sonnet.

## Work Log

- 2026-08-11 (sprint) — Fixed. Added one shared `same_file(a, b) -> bool` helper in
  `crates/server/src/batch_media.rs`, used by BOTH call sites named in the ticket:
  1. `plan()`'s non-destructive "output must differ from input" guarantee + its `used` collision set
     (previously `HashSet<String>` exact-match; now a `Vec<String>` scanned with `same_file` since
     equality is no longer a trivial string compare — batch sizes here are small enough that the O(n)
     scan is not a concern).
  2. `batch_execute::any_in_place` / the `confirmed_overwrite` refusal check (previously
     `it.input == it.output`), including the refusal message's file count.

  **Canonicalisation strategy (strongest signal first, falling back only when unavailable):**
  1. If BOTH paths exist on disk: `std::fs::canonicalize` each and compare. This resolves
     symlinks/junctions to their real target, AND — because canonicalize returns the file's own
     *stored* path/casing rather than the literal input string — folds case-only differences on a
     case-insensitive filesystem and (on macOS/APFS, which resolves lookups normalisation-insensitively)
     Unicode NFC/NFD differences, for free, with zero per-platform special-casing in this branch.
  2. The common case — a planned OUTPUT usually doesn't exist yet: canonicalize each path's *parent*
     directory (for every path `plan()` produces, that's the input's own already-existing directory, so
     this almost always succeeds) and compare the resolved parent plus the final path component,
     case-folded per-platform (see below). Catches the worked example and trailing-separator/`.`/`..`
     variants of an existing directory.
  3. Neither path (nor its parent) exists on disk (e.g. bare in-memory strings, most unit tests): a
     purely lexical, filesystem-free normalisation (`lexical_normalize`: dual `/`+`\` separator
     handling, `.`/`..` resolution, root-marker preservation), still case-folded per-platform.

  **Case-folding decision (platform, not live filesystem probe):** fold case on Windows and macOS
  (`fold_case`, `#[cfg(any(target_os = "windows", target_os = "macos"))]`); never on Linux/other Unix,
  per the ticket's explicit instruction not to make the check unconditionally case-insensitive there —
  folding would wrongly treat two distinct real files as one. This is a *default-assumption* gate (a
  case-sensitive volume mounted on Windows/macOS, or a case-insensitive exFAT/vfat volume mounted on
  Linux, is out of scope — matches how the ecosystem generally treats this, e.g. git's `core.ignorecase`
  default). 8.3 short names are NOT specially handled — out of scope; `canonicalize` happens to defeat
  them when the real file exists (branch 1), but the lexical/parent-only fallbacks (branches 2–3) would
  not catch an 8.3 alias of a name that doesn't yet exist.

  **Unicode NFC/NFD:** only resolved via the OS canonicalize path (branch 1) on macOS/APFS, which is the
  one platform whose filesystem normalises lookups; Windows/NTFS and Linux/ext4 etc. do not, and no
  Unicode-normalisation crate was added (lean-core "no new dependencies" guardrail) for the lexical
  fallback, so an NFC vs NFD spelling of a *non-existent* output path is not folded there. Documented as
  a known, deliberately-scoped gap.

  **Frontend mirror (`src/lib/batchMedia.ts`):** `overwritesInPlace` now filters via a new `sameFile(a, b,
  platform?)`, a **lexical-only** port of the Rust fallback chain's branch 3 (no synchronous filesystem
  access in the webview, so no canonicalize/symlink/NFD resolution client-side — documented in the
  function's own doc comment). `platform` defaults to the live `navigator.platform`/`navigator.userAgent`
  sniff (mirrors `terminalClient.ts`'s `shellChoicesFor` pattern: a pure function taking an explicit
  `platform: string` so tests don't need a DOM), with case-folding gated the same way as the Rust side
  (`/win/i` or `/mac/i` in the platform string).

  **Tests added** (all passing locally; see verification below):
  - `crates/server/src/batch_media.rs`: 9 new `same_file` unit tests (worked example platform-gated,
    trailing separator, `.`/`..` segments, separator-style-agnostic, reflexive/distinct, a REAL file
    resolved via canonicalize despite a case-variant path, a symlink followed to its target, a Windows
    directory-junction followed to its target (`#[cfg(windows)]`, ran green in this dev environment — a
    real Windows box, not just CI), and a macOS-only NFC/NFD test (`#[cfg(target_os = "macos")]` — not
    exercised in this Windows dev environment; will run on the macOS CI leg). Plus 2 new `plan()`-level
    integration tests pinning the worked example's exact planned output in both non-destructive and
    overwrite modes.
  - `crates/server/src/batch_execute.rs`: 1 new end-to-end test with a REAL file on disk reproducing the
    ticket's worked example through `any_in_place` + `execute_plan_walk`'s refusal + a confirmed run.
  - `src/lib/batchMedia.test.ts`: mirrored `sameFile` unit tests (platform passed explicitly per test —
    Win32/MacIntel/Linux x86_64 strings) + 2 new `overwritesInPlace` tests pinning the worked example
    platform-gated.
  - **What can't be exercised on the 3-OS CI matrix:** the Windows-junction test only runs on the Windows
    CI leg (`#[cfg(windows)]`); the macOS NFC/NFD test only runs on the macOS CI leg
    (`#[cfg(target_os = "macos")]`); the case-insensitivity assertions in the worked-example/real-file
    tests branch their `assert!` per `#[cfg(any(target_os = "windows", target_os = "macos"))]` so each OS
    leg checks the behaviour actually correct for itself rather than a hard-coded expectation. The
    symlink test uses the existing unprivileged-Windows skip pattern from `links.rs` (symlink creation
    can be gated without Developer Mode/elevation) so it degrades to a no-op rather than a false failure
    there. 8.3 short-name variants and non-macOS Unicode normalisation are not tested at all — out of
    scope per the reasoning above, not a coverage gap in what was promised.

  **Docs:** updated `src/docs/explorer-batch-media.md`'s "Choosing non-destructive vs. in-place" section
  to explain that Convert's same-name check is now judged by same underlying file, not exact text, and to
  spell out the Windows/macOS-vs-Linux case behaviour using the ticket's own `IMG_1.JPG` example.

  **Verification (all run synchronously, this environment):**
  - `cargo build` (crates/server) — clean.
  - `cargo test` (crates/server, full suite) — 1868 passed, 0 failed, 1 ignored (pre-existing, unrelated).
  - `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean, 0 warnings.
  - `cargo clippy --all-targets --features specta -- -D warnings` (crates/server) — clean, 0 warnings.
  - `npm run check` — 0 errors, 0 warnings.
  - `npx vitest run` (full frontend suite) — 272 files, 3319 tests passed, 0 failed.
  - No `specta::Type` struct was touched (only free functions + doc comments on modules/functions), so
    `bindings.gen.ts` did not need regeneration; confirmed by `npm run check` staying clean.
  - No Cargo dependency was added/changed, so `src-tauri/Cargo.lock` needed no regeneration.
