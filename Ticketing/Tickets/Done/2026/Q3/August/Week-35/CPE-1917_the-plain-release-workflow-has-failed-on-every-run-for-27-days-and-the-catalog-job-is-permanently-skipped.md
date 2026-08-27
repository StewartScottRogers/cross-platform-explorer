---
id: CPE-1917
title: the plain Release workflow has failed on every run for 27 days — `verify-release-artifacts` cannot find `latest.json`, so the catalog job is permanently skipped
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

Every run of the plain `Release` workflow (`.github/workflows/release.yml`) since **2026-08-04** has
failed, on **all three platforms**, at the step *"Verify updater manifest + signatures (CPE-1058)"*.
Because that step fails, the dependent `catalog` job is **skipped on every run** — it has not executed
once in 27 days.

This was noticed during the 2026-08-23 batched run, recorded as background context inside several
other tickets, and **never filed on its own**. Filing it now so it stops being folklore.

## Measured evidence

Run `32645968177` (`release: bump to 0.57.69`, 2026-08-23), job `release (windows-latest)`, raw log
fetched via `gh api repos/:owner/:repo/actions/jobs/97210161653/logs` (2,191 lines, complete — the
reporter view truncates, per CPE-1868, so this was fetched raw and the tail confirms a real finish):

    Running `target\release\verify-release-artifacts.exe --conf ../../src-tauri/tauri.conf.json --search ../../src-tauri/target`
    verify-release-artifacts: no latest.json found under ../../src-tauri/target (searched: 1)
    error: process didn't exit successfully: ... exit code 1
    ##[error]Process completed with exit code 1.

Job outcomes on that run:

| job | conclusion |
|-----|------------|
| `release (ubuntu-22.04)` | failure |
| `release (macos-latest, --target universal-apple-darwin)` | failure |
| `release (windows-latest)` | failure |
| `catalog` | **skipped** |

The three most recent `release.yml` runs (2026-08-20 and 2026-08-23 ×2) are all `failure`.

## What this does and does not affect

- It does **not** affect the **sidecar** build (`Release (sidecar-enabled)`), which is the channel
  users actually install ([[always-install-sidecar-build]]). That is why nobody has been hurt yet, and
  why it has gone unnoticed for a month.
- It **does** mean the plain channel's updater manifest has been unverified for 27 days, and the
  `catalog` job — the agent-catalog publish step (CPE-308) — has not run from this workflow at all.
- Related: **CPE-1874** (six shipped releases never signature-checked) and **CPE-1908** (the
  channel-purity guard only covers the plain manifest — the mirror-image gap).

## Acceptance criteria

- [x] Establish *why* `latest.json` is absent under `src-tauri/target` in this workflow — is the
      verifier's `--search` root wrong for the plain build's layout, is the manifest emitted somewhere
      else (or under a different name) by the `tauri-action` version in use, or is the plain build
      genuinely not producing an updater manifest any more? Answer with evidence from a real run's
      artifact listing, not by reading the workflow.
- [x] Fix the actual cause. If the search root is wrong, fix the root; if the manifest is genuinely
      not being produced, that is a bigger finding and must be stated plainly rather than papered over
      by widening the search.
- [x] **Do not make the verify step non-fatal.** A verification step that can be skipped is the
      failure mode CPE-1058 was filed to close, and CPE-1901 is already open on a one-token kill
      switch in the neighbouring pin check. Silencing this is a regression, not a fix.
- [x] Pin the fix with a test or guard that goes red if `latest.json` moves again — the recurring
      defect in this repo is guards that prove nothing.
- [ ] Confirm the `catalog` job actually runs to completion once the gate passes, and say what it
      published. A green `release` with a still-skipped `catalog` is a half fix.
      **NOT CLOSED — needs a real tagged release, which is the user's call, not an agent's. Its
      permanent-skip cause is fixed and asserted; see the Work Log's AC5 section.**
- [x] Add a backstop that makes a **month-long silent workflow failure** visible: nothing surfaced
      this for 27 days, which is the more expensive half of the bug.

## Notes

Filed 2026-08-26 by the sprint Foreman during pre-flight, from the previous run's checkpoint note
("open item 1") plus a fresh raw-log confirmation. The checkpoint explicitly flagged that this had
never been ticketed.

## Foreman note added 2026-08-27 00:00

PR **#1039** (CPE-1908, sidecar channel-purity guard) touches the very binary that fails here —
`crates/updater-verify/src/bin/verify-release-artifacts.rs` — adding an `--expect-channel` flag and a
post-matrix gate job. It does **not** claim to fix the missing-`latest.json` failure. Whoever picks
this ticket up should rebase onto that work rather than against it, and should specifically check
whether the new post-matrix gate shape (which fetches the *published* manifest rather than searching
`src-tauri/target`) is in fact the correct shape for the plain channel's in-matrix step too — that
would make this a "use the gate we already have" fix rather than a search-root patch.

## Work Log — 2026-08-27

### Current state first: the failing invocation no longer exists on `main`

The Foreman's note was right that the landscape moved, but it named the wrong PR. **PR #1039
(CPE-1908) is not what changed this area, and there is no `--expect-channel` flag anywhere in the
repo** (`grep -rn "expect-channel"` → no hits; CPE-1908's actual commit is `c75c99c9`, "extend the
channel-purity guard to the sidecar release channel"). What reworked this area is **PR #1008
(CPE-1872), merged `f97aef8a` on 2026-08-23 20:27** — three hours after the last failing run.

CPE-1872 is, in substance, **this same bug**: its ticket file is literally named
`...-workflow-has-failed-on-every-platform-for-27-days.md`. CPE-1917 was filed 2026-08-26 from a stale
checkpoint note, three days after the fix landed. So:

- The step named in this ticket — *"Verify updater manifest + signatures (CPE-1058)"* inside the
  `release` matrix — **is gone from `main`**. `verify-release-artifacts` is now invoked exactly once,
  from the post-matrix `verify-published-manifest` job, over assets downloaded from the draft release
  (`--manifest release-assets/latest.json --search release-assets --expect-url-prefix …`).
- `catalog`'s permanent skip was separately fixed by **CPE-1893** (`3d15b555`), which gave it
  `if: ${{ !cancelled() }}` and added `catalog-freshness.yml`.
- **No plain-channel tag has been pushed since.** `gh run list --workflow=release.yml` shows the most
  recent run as `32645894722` (2026-08-23 14:35). So the fix was correct-by-inspection but had never
  been exercised — which is exactly the gap AC5 names.

### AC1 — why `latest.json` was absent, from real artifacts

Not from reading the workflow. From the release itself:

    gh release view v0.57.69 --json assets
      → 14 assets, including **latest.json (7,206 bytes)**; draft: true; created 2026-08-23T14:35:22Z

    gh api .../actions/runs/32645894722/jobs
      → release (ubuntu-22.04)   failure  @ "Verify updater manifest + signatures (CPE-1058)"
        release (windows-latest) failure  @ same step
        release (macos-latest…)  failure  @ same step
        catalog                  skipped

The manifest **exists and is complete**: 11 platform entries (`linux-x86_64`, `-appimage`, `-deb`,
`-rpm`, `windows-x86_64`, `-msi`, `-nsis`, `darwin-aarch64`/`-x86_64` + `-app`), each naming a
plain-channel asset that is on the release, each carrying a signature.

**So the third and scariest possibility in AC1 — "the plain build is genuinely not producing an
updater manifest any more" — is ruled out by the artifact listing, not by argument.** It was producing
a good one the whole time. The verifier was pointed at a directory it has never been written to:
`tauri-action`'s `upload-version-json.ts` writes `resolve(process.cwd(), 'latest.json')` — the job's
repo root — and uploads it directly. `--search ../../src-tauri/target` (resolved from
`working-directory: crates/updater-verify`) could not have found it on any platform, ever.

Reproduced both directions locally, offline, against the **real published bytes** — the manifest plus
all six referenced assets downloaded from the v0.57.69 draft:

- **Old invocation, RED, byte-identical to the logged failure:**

      $ verify-release-artifacts --conf ../../src-tauri/tauri.conf.json --search ../../src-tauri/target
      verify-release-artifacts: no latest.json found under ../../src-tauri/target (searched: 1)
      EXIT=1

- **Current invocation, GREEN, against the very release that was failing:**

      $ verify-release-artifacts --conf src-tauri/tauri.conf.json \
          --manifest release-assets/latest.json --search release-assets \
          --expect-url-prefix "https://github.com/StewartScottRogers/cross-platform-explorer/releases/download/v0.57.69/"
      channel    : plain (product name: 'Cross-Platform Explorer')
      url prefix : … (enforced)
      OK: verified 11 of 11 platform signature(s) …
      EXIT=0

That second run is the strongest evidence available without pushing a tag: the CPE-1873 pubkey +
endpoints pin, the CPE-1903 platform-config scan, the CPE-1872 url-prefix binding and the CPE-1894
channel check all ran for real, over the real manifest, and all passed. `v0.57.69`'s draft would have
gone green under today's workflow.

### AC2 — the cause is fixed, and deliberately not "re-fixed"

Nothing in `release.yml` needed changing; re-patching a correct workflow to have something to show
would have been the wrong move. **The verify step was not made non-fatal, no kill switch was added,
and `--skip-pin-check` is now asserted to be absent from the workflow (AC3).**

### AC4 — the pin, and why the existing tests were not one

CPE-1872 added tests to `release_guard.rs` whose helper is documented as running the binary "exactly
the way `release.yml` invokes it post-CPE-1872". **That stopped being true within the same PR**: round
2 moved the check into `verify-published-manifest` with completely different arguments, and the
hard-coded `--manifest latest.json --search src-tauri/target` was never updated. So the guard for "the
workflow points at the right place" had quietly stopped being about the workflow — it is green no
matter what `release.yml` says. That is CPE-1929's defect, sitting inside the fix for this very bug.

Two new pins, plus a correction:

1. **`crates/updater-verify/tests/release_workflow_wiring.rs`** (new, 5 tests) — reads the invariant's
   two halves out of `release.yml` from **two different places** and executes them against each other:
   the *download* step's `--dir` decides where the fixture manifest + artifact are scaffolded; the
   *verify* step's argv is extracted (continuations joined, `${REPO}`/`${TAG}` resolved) and handed to
   the real binary. Not circular, and genuinely falsifiable — measured:

   | mutation to `release.yml` | result |
   |---|---|
   | `--search release-assets` → `--search src-tauri/target` (the original bug) | **2 tests RED** |
   | download `--dir release-assets` → `--dir assets` | **2 tests RED** |
   | drop `--expect-url-prefix` | **3 tests RED** |
   | unmutated | 5 pass |

   Also asserts statically: exactly **one** `verify-release-artifacts` invocation in the file (so the
   deleted per-leg check cannot creep back), `--conf` is the plain channel's real config, and the
   workflow never passes `--skip-pin-check`. Plus RED companions proving the green test discriminates
   (a manifest under `src-tauri/target` is not found; a tampered artifact still fails).

2. **`src/lib/releaseVerifyWiringGuard.test.ts`** (new, 16 tests, `parseYaml` house style) — the
   structural facts a running binary cannot see: the gate lives in `verify-published-manifest` and
   **not** in the `release` matrix; the verify step has no `working-directory` (the original root
   cause was relative paths resolved from `crates/updater-verify`); download and verify sit in the
   **same job** and share one secret gate (split them across jobs and the verifier searches an empty
   workspace — "no latest.json found", verbatim, again); `needs: release` + `if: ${{ !cancelled() }}`
   on both `verify-published-manifest` and `catalog`.

3. `release_guard.rs`'s three stale "exactly as release.yml does" claims corrected to say what those
   tests actually prove, with a pointer to the new file.

### AC6 — the backstop, and the hole in the existing one

CPE-1872 added `release-pipeline-watchdog.yml` and CPE-1893 added `catalog-freshness.yml`, so the
month-of-silence half is largely addressed already. What was **not** addressed is that the watchdog can
itself go dark, silently, in two ways — both of which reproduce this ticket one level up:

- **It selects its subjects by workflow *display name*** (`workflows: ["Release", "Release
  (sidecar-enabled)"]`). A `workflow_run` trigger naming a workflow that does not exist matches nothing
  and fails **silently**; rename `release.yml`'s `name:` and the only alarm on the release pipeline
  stops firing with nothing anywhere saying so. Now guarded: the test resolves those strings against
  the real `name:` fields **read from the workflow files**, in both directions (every release workflow
  is covered; every watched name resolves to a real workflow). Red-proofed by renaming `name: Release`
  → `name: Release Plain`: **2 tests RED**, with the reason spelled out in the failure message.
- **It only fires on a run that happened.** If `release.yml`'s tag filter stops matching plain tags
  there is no run, no red X and no issue — quieter than the 27-day outage. CPE-1894's
  `tags: ["v*", "!v*-sidecar"]` was three characters of YAML with no test on it; now pinned, including
  that the negation stays *inside* `tags:` (GitHub rejects `tags` + `tags-ignore` for one event, and a
  rejected config means the workflow does not run at all) and that `release-sidecar.yml` still has no
  `push` trigger.

`RELEASING.md` gained a *"You are not the alarm — the watchdog is"* section under **Check build / CI
status**: `gh run list` is near-useless for a workflow that fires only on tags, and
`gh issue list --label release-pipeline-red --state open` is the real health check.

### AC5 — NOT closed. Stated plainly rather than papered over.

**The `catalog` job has still not run, and this ticket cannot make it run.** Confirming it "runs to
completion and saying what it published" requires pushing a real version tag and publishing, which is
out of this ticket's remit — release plumbing is being fixed here, not a release cut. What can be said:

- Its permanent skip is fixed at the cause (CPE-1893's `if: ${{ !cancelled() }}`, asserted here).
- The gate that was blocking the run has been proven green against the real v0.57.69 artifacts.
- If it silently stops publishing for any *other* reason, `catalog-freshness.yml` now files an issue
  within 14 days instead of nobody noticing for 31.

**Follow-up for whoever cuts the next plain release:** watch `release.yml` end to end and record
`catalog`'s conclusion and the asset names it uploads (`catalog-index.json` + the signed bundle). That
is the one remaining acceptance criterion and it needs a human's decision to tag.

### Deliberately not touched

CPE-1874 (six shipped releases never signature-checked), CPE-1901 (`--skip-pin-check` kill switch —
this ticket only asserts the *workflow* never passes it, which is not the same as removing the flag),
CPE-1923 (three hostile manifests pass at exit 0), CPE-1918 (runbook `gh --jq` PowerShell quoting — the
RELEASING.md section added here deliberately carries no `--jq` snippet).

### Verification

- `cargo test -p cpe-updater-verify` — **69 tests, all pass** (43 lib + 18 release_guard + 5 new wiring
  + 2 pinned_pubkey + 1 platform_config).
- `cargo clippy --all-targets -- -D warnings` on `crates/updater-verify` — clean.
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` on the new guard plus neighbours (`catalogPublishFreshnessGuard`,
  `releaseHangHardening`, `sectionDocs`, `epicsQueueLayout`) — 66 tests, all pass.
- No Rust dependency changed, so no `Cargo.lock` regeneration was needed across the nine lockfiles.
- No signing key touched; `tauri.conf.json` unmodified; nothing published, no workflow dispatched, no
  release edited.
