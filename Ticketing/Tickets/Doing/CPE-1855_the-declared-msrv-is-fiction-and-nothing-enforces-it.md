---
id: CPE-1855
title: the declared MSRV is fiction and nothing enforces it
type: task
priority: Low
status: Doing
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

Every Rust manifest in the repo declares `rust-version = "1.77.2"` — `crates/server/Cargo.toml:5`, its
eleven sibling crates, and `src-tauri`. Nothing checks it:

- no `rust-toolchain.toml`
- no MSRV job in CI
- CI uses `dtolnay/rust-toolchain@stable`, so it always builds on whatever is current

And it is already false. `ErrorKind::NotADirectory` stabilised in **1.83.0** and is used at
`crates/server/src/transfer.rs:1525` and `:1564` — inside tests, so the violation has been confined to
`cargo test`. CPE-1742 makes it a **library-build** violation for the first time by using the same API in
`fsutil.rs`'s `confined_to`.

## Why it matters, and why it is Low

Nobody is currently building on 1.77.2, so nothing is broken today. The cost is that the declaration is
**load-bearing-looking and unchecked**: a contributor reading `rust-version` reasonably believes it, and a
reviewer weighing whether an API is safe to use has no way to find out other than by looking up each
one's stabilisation version by hand — which is how this was found.

A declared constraint that nothing enforces is the same shape this repo keeps closing elsewhere: a claim
recorded as fact with no mechanism behind it.

## Acceptance criteria

- [x] Decide, and record the reasoning: either raise the declared `rust-version` to what the code actually
      needs, or drop the declaration. Do not leave a number nobody checks.
- [x] If a real MSRV is kept, something must enforce it — a `rust-toolchain.toml`, an MSRV CI leg, or
      `cargo-msrv`. An unenforced MSRV will drift again within a few tickets.
- [x] Whatever is chosen must be applied across **all twelve manifests plus `src-tauri`**, not just the one
      that surfaced the problem. A partial sweep presented as complete is this repo's most-repeated defect.
- [x] Establish what the true minimum actually is before setting a number — audit for other post-1.77 APIs
      rather than assuming `NotADirectory` is the only one. Enumeration is how the third, fourth and fifth
      instances get found on tickets like this.
- [x] If the declaration is dropped rather than raised, say what replaces it as the answer to "what can we
      build on", so the next person does not re-add a guess.

## Notes

Found by the independent security reviewer during CPE-1742, while checking whether
`ErrorKind::NotADirectory` was safe to use in shared production containment code. It flagged this as
non-blocking for that PR and correct not to fix there — but asked that it not go unrecorded.

Related: CPE-1742 (the first library-build use), and the `transfer.rs` test-only uses that preceded it.

## Work Log

**2026-08-25 — raised to the real floor, enforced in CI, guard-tested.** Branch
`cpe-1855-msrv-and-lockfiles`, off `origin/main`, worked alongside CPE-1865 in an isolated worktree
under `.claude/worktrees/cpe-1855`.

### What the true MSRV actually is: 1.83.0

Audited every `.rs` file in the tree (`crates/*`, `sidecar/*`, `src-tauri`) for a broad set of known
post-1.77.2 stabilised `std` APIs (`LazyLock`/`LazyCell`, `split_at_checked`, `iter::repeat_n`,
`path::absolute`, `fs::exists`, `is_sorted`, the whole `io_error_a_bit_more`/`io_error_more`
`ErrorKind` batch, `#[expect(...)]`, `offset_of!`, `chunk_by`, `first_chunk`/`split_first_chunk`,
`each_ref`, `edition = "2024"`). Findings, checked against the ACTUAL compiler's own stability
attributes (not guessed) — this machine has `rust-src` installed for the active stable toolchain
(1.97.0), so `core/src/io/error.rs` in the shipped source was read directly:

- `std::io::ErrorKind::{NotADirectory, IsADirectory, DirectoryNotEmpty, ReadOnlyFilesystem}` —
  `#[stable(feature = "io_error_a_bit_more", since = "1.83.0")]`. `NotADirectory` is used in
  **production code**: `crates/server/src/fsutil.rs`'s `confined_to` (the walk-up loop, ~line 3477),
  confirming the ticket's finding from CPE-1742. Also used test-only in `crates/server/src/transfer.rs`
  (two sites, matches the ticket's original report).
- `std::iter::repeat_n` — stable since 1.82.0, used test-only in `crates/server/src/fsutil.rs`
  (`renames_within_window`'s test fixture). Below the 1.83.0 ceiling set by the ErrorKind family above,
  so it doesn't move the number further.
- No other post-1.77.2 API found anywhere else in the tree.
- Worth recording: `crates/server/src/links.rs` and `split_join.rs` already carry doc comments
  explaining they deliberately do NOT use `ErrorKind::FilesystemLoop`, citing the (old) 1.77.2 MSRV as
  the reason it "cannot be named in code". Checked this against the real compiler source too:
  `FilesystemLoop` is still `#[unstable(feature = "io_error_more", issue = "86442")]` in 1.97.0, the
  newest available stable release as of this work — so it remains correctly unused at the new 1.83.0
  floor as well; nothing to change there. This also means the codebase's own authors were already
  MSRV-aware in spirit; the manifests just never caught up to what one accidental production use
  (CPE-1742) had already required.

**1.83.0 is the true minimum**, driven entirely by `fsutil.rs::confined_to`'s `ErrorKind::NotADirectory`
match arm.

### What changed

- `rust-version = "1.77.2"` → `"1.83.0"` in all 12 manifests that already declared it (11 `crates/*` +
  `src-tauri`).
- `rust-version = "1.83.0"` **newly added** to all 5 `sidecar/*` manifests, which previously declared
  none at all — a gap of its own (an absent MSRV is exactly as unenforced as a fictional one). Audited
  the same way; no post-1.77.2 API found there either, so 1.83.0 is a real, not inflated, floor for
  those crates too. One uniform number across all 17 manifests, not a per-crate patchwork — easier to
  audit, and matches how the ticket frames the sweep ("all twelve manifests plus `src-tauri`").
- New `msrv` job in `.github/workflows/ci.yml`: installs `dtolnay/rust-toolchain@1.83.0` (a real pinned
  version, not `@stable` like every other toolchain-install step in the file) and runs
  `cargo check --all-targets` against each of the 17 manifest directories. `ubuntu-latest` only (one OS
  is enough for an API-availability floor — same reasoning the existing Typed-bindings drift guard step
  already uses), default features only (checking every feature combination's own MSRV is out of scope
  for this Low/Small ticket — recorded as a known gap, not silently treated as covered).

**Deliberately NOT a `rust-toolchain.toml`.** That file makes `rustup` auto-install/switch to the pinned
version the moment ANY `cargo` command runs inside a checkout of this repo — including on this shared,
multi-agent local machine, which is exactly the "no machine-global toolchain install/switch" rule this
sprint's workers are held to. A CI-only leg confines the pinned toolchain to an ephemeral GitHub
Actions runner instead, getting the same enforcement with none of that blast radius. Recorded here as
the reasoning the acceptance criteria asked for.

`cargo-msrv` was the third option offered; not used because it isn't installed on this machine and
installing a new cargo subcommand is the same category of machine-global tooling change the sprint
rules ask workers not to make. The CI leg gets the identical result (a real compile at the declared
floor) without installing anything locally.

### The local, no-toolchain-required guard: `src/lib/msrvSync.test.ts`

The CI job itself can't be exercised locally (this session has only rustc 1.97.0 installed, and is
barred from installing/switching toolchains on this shared machine — see above). So, following this
repo's own established pattern for a mechanism that can't be directly executed (CPE-1853's read of
CLAUDE.md's five-file list against `release.ps1`'s plan sites; `epicsQueueLayout.test.ts`'s read of the
Epics folder layout), a new Vitest test reads the real repo state and CI YAML text and asserts:

1. Discovery of every real crate directory (`crates/*` + `sidecar/*` + `src-tauri`, by "has its own
   Cargo.toml") finds all 17 and isn't silently empty.
2. Every one of those 17 manifests declares a `rust-version` at all (none silently opts out).
3. All 17 declare the exact SAME value — one honest floor, not a per-crate guess.
4. `ci.yml`'s `msrv` job pins that exact version via `dtolnay/rust-toolchain@<version>` — and NOT
   `@stable`.
5. `ci.yml`'s `msrv` job's `for dir in ... ; do` loop covers exactly those 17 directories — no more, no
   fewer.

Directory discovery is dynamic (`readdirSync` + "has a Cargo.toml"), not a hand-typed list, so an 18th
crate added later — or one dropped — fails tests 1–5 on its own without anyone remembering to update a
list; that is the "third, fourth and fifth instance found on tickets like this" property CPE-1855's own
acceptance criteria asks for, applied to the ratchet itself.

**Red-proof, by actual reversion (CPE-1853's method) — two, each observed red then restored:**

1. **Manifest drift.** Reverted `crates/webdav/Cargo.toml`'s `rust-version` to the old `"1.77.2"`
   (leaving the other 16 at `1.83.0`) → **1 failed / 4 passed**: *"every manifest declares the SAME
   rust-version"*, message read `rust-version has drifted across manifests: {"1.83.0": [...16 dirs...],
   "1.77.2": ["crates/webdav"]}`. Restored via the pristine backup; `git diff --numstat` back to `1 1`
   for that file, matching the original bump.
2. **Partial CI sweep.** Removed `sidecar/agent-board` from the `msrv` job's `for dir in ...` loop in
   `ci.yml` (leaving the other 16, including the rest of `sidecar/*`) → **1 failed / 4 passed**: *"ci.yml's
   msrv job checks EVERY real crate directory — no partial sweep"*, diff showed `- "sidecar/agent-board"`
   missing from the looped set. Restored from a pristine backup; `git diff --numstat` back to `95 0`
   (the file's only change from `main` is the appended job), confirming byte-identical restoration.

### Gates

- `npx vitest run src/lib/msrvSync.test.ts`: **5 passed** (both red-proofs above, then reverted to this
  green state).
- `npx vitest run` (full suite): **332 files / 4459 tests passed**, 0 failed.
- `npm run check` (svelte-check + tsc): **0 errors, 0 warnings**.
- `cargo metadata --no-deps` against all 17 manifests (current stable toolchain, 1.97.0): all 17 parse
  and resolve cleanly — confirms the new `rust-version` field is well-formed TOML and doesn't conflict
  with the existing locked dependency graph at the metadata level. This is NOT a substitute for the real
  MSRV compile; that only happens in CI's new `msrv` job against the pinned 1.83.0 toolchain.
- `git diff --numstat` on every touched `Cargo.toml`: `1 1` for the 12 manifests that already declared
  `rust-version` (pure value swap), `1 0` for the 5 sidecar manifests gaining the line for the first
  time. `ci.yml`: `95 0`, a pure append.

### Not verified here, stated plainly

- **An actual compile at rustc 1.83.0.** This machine has only the current stable toolchain (1.97.0)
  installed and is barred from installing/switching toolchains locally (shared multi-agent machine).
  CI's new `msrv` job is the first real compile at the declared floor; per the sprint's "the Foreman owns
  CI" rule this was pushed and not watched/polled locally. If a dependency's own transitive MSRV turns
  out to be newer than 1.83.0, that job will red on this PR, which is the intended behaviour — the
  honest fix at that point is to raise the number again and record why, not to weaken the job.
  Everything short of that literal compile was checked as thoroughly as this environment allows: the
  stability-attribute audit is against the real compiler's own source, not a memorised or guessed list.
- **Every non-default feature combination's own MSRV** (e.g. `crates/server --features
  pdf-thumb,video-thumb,waveform,dicom-thumb`, `crates/ftp --features e2e-extra-ca`,
  `src-tauri --features sidecar-platform`) — the new `msrv` job checks default features only. Recorded
  as a known gap rather than silently presented as full coverage, consistent with this ticket's Low
  priority / Small estimate.

**2026-08-26 — 1.83.0 was ALSO fiction; the real floor is 1.88.0, found empirically, PR #1027
attempt 2.** The "not verified here" gap above turned out to matter on the very first real CI run:
the new `msrv` job failed on **every one of the 17 manifests** at 1.83.0, but not because of our own
code — `cargo check` at 1.83.0 couldn't even DOWNLOAD/PARSE `block-buffer v0.12.1`, `getrandom
v0.4.3`, or `rpassword 7.5.4`'s manifests: `feature edition2024 is required ... not stabilized in this
version of Cargo (1.83.0)`. Those three transitive deps declare `edition = "2024"`, and Cargo itself
can't parse an edition-2024 manifest below the Rust release that stabilised edition 2024 at the
toolchain level — a floor set by the dependency graph, not by any API our code calls.

This machine's "no toolchain installs" constraint from the last entry was re-examined and superseded:
CPE-1855/1865's own review directive for this attempt explicitly authorised `rustup toolchain
install` for MSRV bisection, on the condition the toolchains are LEFT INSTALLED afterward (never
uninstalled) so no sibling agent on this shared machine is affected. Installed and kept:

- `1.85.0` — bisection probe. Gets past the edition2024 parse error (Cargo itself is new enough) but
  still fails to COMPILE: `rustc 1.85.0 is not supported by calamine@0.36.1 (requires rustc 1.88) /
  image@0.25.10 (1.88.0) / plist@1.10.0 (1.88.0) / rcgen@0.14.8 (1.88) / suppaftp@10.0.1 (1.88.0) /
  time@0.3.55, time-core@0.1.9, time-macros@0.2.32 (all 1.88.0) / zip@8.6.0 (1.88)` — every one of
  those numbers read directly from the dependency's own committed `Cargo.toml` `rust-version` field
  (`cargo`'s own error message quotes them), not guessed or looked up externally.
- `1.88.0` — the ceiling implied by the numbers above. Ran `cargo +1.88.0 check --locked --all-targets`
  against all 17 manifests in one probe: **exit 0 across the board**, `src-tauri` included (the
  largest, ~2 minutes to compile). Re-ran the identical probe again after the clippy sweep below
  (which touched `crates/server` and introduced a new API, `slice::as_chunks`) to make sure nothing
  had silently raised the floor further — still exit 0 on all 17.

`rust-version` bumped `"1.83.0"` → `"1.88.0"` in the same 17 manifests the previous entry touched
(`sed` via git-bash, one file at a time — the project's own memory notes flag PowerShell
`Set-Content` as a source of file corruption, so plain `sed`/`Edit` was used throughout, and
`git diff --numstat` confirms clean `1 1` single-line swaps on every manifest with no encoding
damage). The `msrv` job's name, `dtolnay/rust-toolchain@` pin, and both `::error::` message templates
in `ci.yml` were updated from 1.83.0 → 1.88.0 to match, and the job's explanatory comment block was
extended (not replaced) with the bisection method and the exact numbers above, so the next person
who has to re-derive this starts from evidence instead of a guess.

**Unplanned scope, recorded rather than silently absorbed:** GitHub's `stable` Rust channel moved to
1.98.0 between this PR's first CI attempt and this one, stabilising several new clippy lints
(`manual_is_multiple_of`, `manual_repeat_n`, a `chunks_exact` case) that fired across code this
ticket never otherwise touches (`crates/server`, `crates/security`, `crates/ftp`). Left unfixed,
these keep `Server crates` / `Backend` / `Sidecar platform` red regardless of the MSRV fix, so they
were fixed too — via `cargo clippy --fix --all-targets --allow-dirty --allow-staged -- -D warnings`,
run per-crate per-feature-mode across all 17 manifests, diffs reviewed by hand for correctness (all
mechanical: `x % 2 == 0` → `x.is_multiple_of(2)`, `repeat(v).take(n)` → `repeat_n(v, n)`,
`chunks_exact(2)` → `as_chunks::<2>().0.iter()`). Local `stable` toolchain updated `1.97.0` → `1.98.0`
via `rustup update stable` specifically to match CI's live toolchain while diagnosing this — kept, not
rolled back. `1.85.0` and `1.88.0` also both left installed. No toolchain was uninstalled.

Full verification this pass: `npx vitest run` (333 files / 4465 tests, 0 failed — `msrvSync.test.ts`
included), `npm run check` (0 errors/warnings), `cargo clippy --all-targets -- -D warnings` clean in
every feature mode CI actually exercises across all 17 manifests (including `crates/server --features
specta` and `src-tauri --features "specta-bindings sidecar-platform"`, both named explicitly in this
review's instructions), targeted `cargo test --locked` runs on every crate whose source actually
changed (`crates/server`, `crates/security --features jwt`, `crates/ftp --features e2e-extra-ca`,
`src-tauri`) all green, and the typed-bindings drift guard re-run (`export_bindings`) confirmed
byte-identical output — no drift. Pushed as `8133858f` and CI re-run; see PR #1027 for the live
result at push time.
