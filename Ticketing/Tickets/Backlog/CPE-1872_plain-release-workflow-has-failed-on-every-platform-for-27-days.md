---
id: CPE-1872
title: the plain Release workflow has failed on every platform for 27 days and nothing told anyone
type: bug
priority: High
status: Backlog
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

- [ ] The root cause is stated with evidence (where `latest.json` is, per platform), not inferred.
- [ ] `Release` completes green on all three platforms for a real tag.
- [ ] The verify step demonstrably goes **red** on a bad/missing manifest — shown, not asserted.
- [ ] The `catalog` job runs rather than being skipped.
- [ ] Some mechanism now surfaces a persistently-failing release workflow to a human.

## Work Log

- **2026-08-23 11:35 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  The previous run noticed this repeatedly while working other tickets, recorded it as context
  inside them, and never filed it on its own; the closing checkpoint flagged that omission as the
  first open item. Failure confirmed live against run `32645968177` before filing.
