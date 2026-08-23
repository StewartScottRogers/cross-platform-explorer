---
id: CPE-1872
title: the plain Release workflow has failed on every platform for 27 days and nothing told anyone
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

Every run of the plain **`Release`** workflow since **2026-08-04** has failed, on **all three
platforms**, at the same step:

```
Verify updater manifest + signatures (CPE-1058)
  → verify-release-artifacts: no latest.json found under ../../src-tauri/target (searched: 1)
  → Process completed with exit code 1
```

Verified on run `32645968177` (tag `v0.57.69`, 2026-08-23): `release (ubuntu-22.04)`,
`release (macos-latest, --target universal-apple-darwin)` and `release (windows-latest)` all
failed at that step, and the dependent **`catalog`** job was therefore `skipped` — as it has been
on every release for 27 days. The agent-catalog bundle (CPE-377/308) has not been signed and
published alongside an installer in that whole window.

The step is `.github/workflows/release.yml:184-191`:

```yaml
- name: Verify updater manifest + signatures (CPE-1058)
  if: steps.sig.outputs.has == 'true'
  working-directory: crates/updater-verify
  run: |
    cargo run --release --bin verify-release-artifacts -- \
      --conf ../../src-tauri/tauri.conf.json \
      --search ../../src-tauri/target
```

`tauri-action` runs with `includeUpdaterJson: true`, so the manifest is *produced* — but
`verify-release-artifacts` cannot find a `latest.json` beneath `src-tauri/target`. Its own message
reports `searched: 1`, i.e. it considered a single candidate location. So the fault is one of:
the manifest is no longer written under the bundle tree at all (tauri-action attaches it to the
release directly), or it is written somewhere the `--search` root/depth does not reach, or the
`--target universal-apple-darwin` arm changes the path shape. **Diagnose before fixing** — do not
guess and widen the search until it stops erroring, because a search that finds nothing and a
search that is looking in the wrong place both go green if you simply relax the check.

## Why High, and why the second half matters more than the first

`release.yml` is not the workflow that ships the installed app — **`release-sidecar.yml`** is
([[always-install-sidecar-build]]), and it is green. That is exactly why this went unnoticed:
the failing pipeline is the one nobody watches, so it stayed red through **six** releases.

Two consequences, in order of severity:

1. **Nothing noticed.** A release workflow failed 100% of the time, on every platform, for 27 days,
   across at least six version tags — and the only reason it surfaced at all is that a sprint worker
   happened to read a run list. There is no signal that a release pipeline has gone dark. This is
   the same **fails-by-succeeding** shape this repo keeps closing (CPE-1806, CPE-1814, CPE-1815,
   CPE-1780): the *absence* of an alarm read as the absence of a problem.
2. **The updater manifest is unverified.** CPE-1058 exists to re-check every platform's minisign
   signature against the configured pubkey over the real artifact bytes before the manifest ships.
   That check has not run since 2026-08-04. The plain release's `latest.json` — the thing the
   auto-updater consumes — has been going out (or not going out) unverified.

## What to do

1. **Find out where `latest.json` actually is** on a real release run before changing anything.
   Read the tauri-action output on run `32645968177`, and/or list the bundle tree. Record the
   finding in the work log — the path, per platform, with evidence.
2. **Fix the locate step so it verifies the real manifest.** Whether that means correcting
   `--search`, teaching `verify-release-artifacts` the current tauri-action layout, or fetching the
   manifest from the draft release, the invariant is: **the signatures are checked over the bytes
   that actually ship.**
3. **Prove it can still fail.** Corrupt a signature (or point it at a manifest with a version
   mismatch) and show the step goes red. A verify step that passes because it found nothing is
   precisely the bug being fixed — if the manifest is missing, that must be a **failure**, never a
   skip. Note the existing `if: steps.sig.outputs.has == 'true'` guard is the *legitimate* skip
   (no signing key configured, forks stay green); do not widen it to swallow a missing manifest.
4. **Make a dark release pipeline visible.** Add a signal so a wholly-failing release workflow
   cannot sit unnoticed for a month — the repo already has the `ffmpeg-pin-freshness` workflow as a
   precedent for "file an issue when a scheduled check goes bad", and a daily status task exists on
   the desktop surface. Pick the cheapest mechanism that actually reaches a human and pin it.
5. Confirm the **`catalog`** job runs again once `release` is green, since it has been skipped for
   the entire window.

## Acceptance criteria

- [x] The root cause is stated with evidence (where `latest.json` is, per platform), not inferred.
- [ ] `Release` completes green on all three platforms for a real tag. **Cannot be proven without
      pushing a version tag**, which this ticket's working rules explicitly forbid a worker from
      doing. Proven locally instead (fixture red/green + the exact command-line shape); the real
      three-platform green is the first thing the next tagged release must show — see Notes.
- [x] The verify step demonstrably goes **red** on a bad/missing manifest — shown, not asserted.
      `crates/updater-verify/tests/release_guard.rs` (7 tests, all passing) plus a manual dedupe-logic
      harness for the new watchdog — see Work Log below.
- [ ] The `catalog` job runs rather than being skipped. Same blocker as the green-release criterion
      above: `catalog` only runs after a real `release` job succeeds on a real tag push.
- [x] Some mechanism now surfaces a persistently-failing release workflow to a human.
      `.github/workflows/release-pipeline-watchdog.yml` added.

## Work Log

- **2026-08-23 11:35 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  The previous run noticed this repeatedly while working other tickets, recorded it as context
  inside them, and never filed it on its own; the closing checkpoint flagged that omission as the
  first open item. Failure confirmed live against run `32645968177` before filing.

- **2026-08-23 (Worker, CPE-1872)** — Picked up, worktree `.claude/worktrees/cpe-1872`, branch
  `cpe-1872-fix-release-updater-verify`.

  **Root cause (evidence, not inferred).** Downloaded the full job logs for all three legs of run
  `32645968177` (`gh api repos/.../actions/jobs/<id>/logs`, not the truncating `gh run view --log`)
  and confirmed the identical failure shape on ubuntu (job `97210161557`), macos (`97210161652`), and
  windows (`97210161653`): each log shows `tauri-action` print `Uploading latest.json...` right
  before the doomed verify step runs — `latest.json` is uploaded straight to the draft release, never
  written under `src-tauri/target`. Confirmed *why* by reading `tauri-apps/tauri-action`'s own source
  at the pinned `v0` tag (`src/upload-version-json.ts`):
  ```ts
  const versionFilename = 'latest.json';
  const versionFile = resolve(process.cwd(), versionFilename);
  ...
  writeFileSync(versionFile, JSON.stringify(versionContent, null, 2));
  ```
  `process.cwd()` is the step's working directory, which for "Build and publish release" is
  unset — i.e. `$GITHUB_WORKSPACE`, the **repo root**, on all three platforms (checkout, Node, and
  Rust steps all run at the default working directory; nothing in `release.yml` overrides it for that
  step). So `latest.json` lands at `<repo root>/latest.json`, a *sibling* of `src-tauri/`, and the old
  `--search ../../src-tauri/target` (run from `working-directory: crates/updater-verify`) structurally
  could never find it — not a flaky check, a guaranteed miss on every run. `src-tauri/target` itself
  is still correct as the location of the signed installers/`.sig` files the manifest's platform
  entries point at (confirmed in the same logs: `.../src-tauri/target/release/bundle/deb/....deb.sig`
  etc.) — only the manifest itself was in the wrong search root.

  **Fix.** `.github/workflows/release.yml`: dropped `working-directory: crates/updater-verify` from
  the verify step (it now runs at the repo root, the same cwd tauri-action just wrote into) and
  switched to `cargo run --manifest-path crates/updater-verify/Cargo.toml`. Added an explicit
  `--manifest latest.json` pointing straight at the known write location instead of widening
  `--search` to walk the whole repo (which would recurse into `node_modules`/`.git`/`dist` for no
  reason and risks matching a stray `latest.json` — exactly the "search until something turns up"
  trap the ticket calls out). `--conf`/`--search` became repo-root-relative
  (`src-tauri/tauri.conf.json`, `src-tauri/target`) to match. No change to
  `verify-release-artifacts.rs` itself was needed — its `--manifest` flag and its "missing manifest is
  a hard failure" behavior already existed and are already correct; the bug was purely in *where the
  workflow told it to look*.

  **Red/green proof (local, per the ticket's guidance since a real tag can't be pushed).** Added 4 new
  integration tests to `crates/updater-verify/tests/release_guard.rs` (existing 3 kept, all still
  passing) using a fixture tree shaped exactly like the real repo (`<root>/latest.json`,
  `<root>/src-tauri/tauri.conf.json`, `<root>/src-tauri/target/.../artifact`):
  - `manifest_at_repo_root_is_not_found_by_search_under_target_alone` — **RED**, reproduces the exact
    real bug: old-style invocation (`--search src-tauri/target`, no `--manifest`) against the real
    layout fails with `no latest.json found`, byte-for-byte the message from the live runs.
  - `manifest_at_repo_root_is_found_and_verified_via_explicit_manifest_flag` — **GREEN**, the fix:
    same fixture, new-style invocation (`--manifest latest.json --search src-tauri/target`, run from
    the fixture root) verifies successfully.
  - `missing_manifest_at_expected_location_is_a_hard_failure` — **RED**: even under the fixed
    invocation, if `latest.json` genuinely isn't at the expected path, the step fails
    (`cannot read ...`), never silently passes.
  - `tampered_artifact_at_repo_root_layout_still_fails_the_release` — **RED**: a corrupted artifact
    still fails signature verification under the new invocation shape (crypto check unweakened by the
    path fix). Pre-existing `tampered_artifact_fails_the_release` / `version_mismatch_fails_the_release`
    also still pass unchanged.
  All 20 tests in the crate pass (`cargo test --release` in `crates/updater-verify`): 13 lib unit
  tests + 7 `release_guard.rs` integration tests (3 pre-existing + 4 new). `cargo clippy --all-targets
  -- -D warnings` is clean.

  **Release-pipeline visibility (criterion 4/5).** Added
  `.github/workflows/release-pipeline-watchdog.yml`, following `ffmpeg-pin-freshness.yml`'s shape (a
  deduped GitHub issue + the run itself staying red as a backstop) but reactive
  (`on: workflow_run: workflows: ["Release", "Release (sidecar-enabled)"]`) instead of scheduled — a
  weekly poll on a workflow that only runs per version tag could still miss a break for weeks, where
  reacting to the run itself reports the *first* failed attempt, not the sixth. **Judgment call:**
  watches both release workflows by name (`Release` and `Release (sidecar-enabled)`), not just the one
  this ticket is about — `release-sidecar.yml` is green today, but "currently green" is exactly the
  condition that let `release.yml` go unwatched for a month; the extra name in one `workflows:` list is
  free. Deliberately did **not** copy `ffmpeg-pin-freshness.yml`'s dedupe line
  (`existing=$(gh issue list ... || true)`) verbatim — that's the exact shape CPE-1794 flagged as
  swallowing a transient `gh` failure into "no existing issue" and filing a duplicate. The new
  workflow captures the lookup's exit status separately and fails loud (creates nothing) on a lookup
  failure. Verified this with a fake `gh` stub on `PATH` driving the extracted `run:` script through
  three scenarios: (1) `gh issue list` fails transiently → script exits 1, **zero** `gh issue create`/
  `gh issue comment` calls logged (the CPE-1794 defect does not reproduce); (2) lookup succeeds, no
  existing issue → `gh issue create` called once, script exits 1 (backstop); (3) lookup succeeds,
  existing issue `#42` → `gh issue comment 42` called, no `gh issue create` (no duplicate). Also
  workflow_run-triggered workflows only activate once merged to `main` and only watch workflows
  present on `main` at trigger time — so this cannot be tested end-to-end pre-merge; the fake-`gh`
  harness above is the closest available proof of the dedupe logic itself.

  **YAML validity.** Both `release.yml` and the new `release-pipeline-watchdog.yml` parse cleanly
  under `yaml.safe_load`; both `run:` blocks pass `bash -n`.

  **Not done / left for the next tag (Definition-of-done item 8).** Did not push a version tag —
  forbidden by this ticket's working rules. The real proof of criteria 2 and 4 (`Release` green on all
  three platforms; `catalog` running instead of skipped) only exists once the **next** `vX.Y.Z` tag is
  pushed and `release.yml` runs for real. What that run should show: all three `release` matrix legs
  green through "Verify updater manifest + signatures (CPE-1058)" (its stdout should read `OK: manifest
  + 1 platform signature(s) verified against the configured pubkey` per leg, not
  `no latest.json found`), the `catalog` job actually executing (no longer `skipped`), and — if
  anything *does* go red on that run or a later one — `release-pipeline-watchdog.yml` should file a
  `release-pipeline-red`-labeled issue within minutes rather than the failure sitting undetected.
