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

Verified on run `32645894722` (tag `v0.57.69`, 2026-08-23): `release (ubuntu-22.04)`,
`release (macos-latest, --target universal-apple-darwin)` and `release (windows-latest)` all
failed at that step, and the dependent **`catalog`** job was therefore `skipped` — as it has been
on every release for 27 days. The agent-catalog bundle (CPE-377/308) has not been signed and
published alongside an installer in that whole window.

*(Correction, round 2: this originally cited run `32645968177`, but that run was triggered by tag
`v0.57.69-sidecar`, not `v0.57.69` — a filing error by the Foreman, spotted during the round-2
security-audit review. The real `v0.57.69` tag push is run `32645894722`, confirmed to have failed
identically on all three legs at the same step. `32645968177` also failed identically, for the same
reason, just under the wrong tag label — the root cause and every finding below are unaffected.)*

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
      `crates/updater-verify/tests/release_guard.rs` (10 integration tests + 14 lib tests, all passing)
      plus a manual dedupe-logic harness for the watchdog — see Work Log, including round 2's
      independent-security-audit red/green proof for the smuggled-platform and basename-decoy holes.
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

  **Not done / left for the next tag (Definition-of-done item 8) — superseded, see the round-2 entry
  below for the current shape of what the next tag must show.** Did not push a version tag — forbidden
  by this ticket's working rules.

- **2026-08-23 (Worker, CPE-1872 round 2 — independent security-audit findings)** — Same worktree/
  branch. PR #1008's gauntlet came back Reviewer `APPROVE`, UAT `UAT PASS`, but an independent Security
  Auditor built real minisign keypairs and eleven fixture releases against round 1's binary and found
  the verifier itself was weaker than its "OK" message claimed. Root cause from round 1 was
  independently re-confirmed by both the Reviewer and UAT and is unchanged. Four findings, addressed
  below in priority order; a fifth item (citation correction) was the Foreman's own filing error, fixed
  in the Problem section above.

  **FINDING 1 (HIGH) — a smuggled platform entry with no locally-checkable artifact passed `EXIT=0`.**
  Root cause: `lib.rs`'s `verify_update_manifest` treated a platform whose `artifact()` closure returned
  `None` as a *skip* (crypto check not run, no problem recorded), on the theory that a per-runner guard
  could legitimately verify only the platforms it built locally. That theory was wrong in practice:
  tauri-action's `upload-version-json.ts` downloads the *existing* published `latest.json` and merges
  each runner's platform into it, so the manifest that ships is the UNION of the whole release matrix,
  and no single leg's `src-tauri/target` ever contains every platform the manifest names — meaning "skip
  what's missing" was really "skip whatever this leg didn't happen to build," which an attacker-supplied
  platform entry (pointing at a URL nothing local will ever match) sails straight through. Independently
  reproduced RED on this machine before touching any code (`smuggled_extra_platform`: an honest
  `windows-x86_64` entry + a `linux-x86_64` entry signed by a different keypair, pointed at
  `https://evil.example/pwn.AppImage.tar.gz` — `EXIT=0`, `OK: manifest + 1 platform signature(s)
  verified`), matching the auditor's own finding exactly.

  **(a) vs (b), and why (a).** The ticket allowed either "a post-matrix job that downloads the draft's
  `latest.json` plus every asset it references and verifies all platforms" (a) or
  "`--expect-platforms` driven from the matrix leg" (b). Chose (a) — also the auditor's and Foreman's
  stated preference. Reasoning: (b) still only ever verifies each leg's OWN belief about which platforms
  the eventual manifest will contain (`--expect-platforms` would have to be a per-leg guess at the
  matrix's own shape), so it verifies a *model* of the release, not the release; a bug in that model (a
  leg's expected-platform list drifting from what actually ships, e.g. after a matrix change) reproduces
  the exact "checked something, but not the real thing" shape this whole finding is about. (a) verifies
  the manifest AS PUBLISHED — the actual bytes a real Tauri updater will fetch — which is the literal
  wording of the ticket's own invariant ("the signatures are checked over the bytes that actually
  ship"). It also runs once instead of three times (cheaper) and, as a side effect, closes Finding 2
  for free (see below). Did not do both, per the round-2 instructions.

  **Fix.** `crates/updater-verify/src/lib.rs`: `verify_update_manifest` now pushes a new
  `ManifestProblem::ArtifactUnavailable { platform }` (a hard failure) instead of silently skipping when
  `artifact()` returns `None` — `Ok(())` is now only reachable when *every* named platform was fetched
  and cryptographically verified. Added `pub fn manifest_platform_count(manifest_json) -> Option<usize>`
  so callers can report "verified N of M" using the manifest's own shape. `verify-release-artifacts.rs`:
  the success message is now `OK: verified {n} of {total} platform signature(s) against the configured
  pubkey.` (never an unqualified "OK"), with a belt-and-suspenders check that fails loud if `n != total`
  even though that should be unreachable now. `.github/workflows/release.yml`: removed the per-leg
  "Detect updater signing key" + "Verify updater manifest + signatures" steps from the `release` matrix
  job entirely and added a new job, `verify-published-manifest` (`needs: release`, runs once on
  `ubuntu-latest` after the whole matrix completes): downloads `latest.json` from the draft release via
  `gh release download`, extracts every platform's asset basename with `jq -r '.platforms[].url' | sed
  's#.*/##' | sort -u`, downloads each one *individually* (not `--pattern '*'`, so a manifest naming an
  asset that was somehow never uploaded is a loud `gh` failure here rather than a silently smaller
  verified set), then runs the (now strict) binary against that clean, freshly-downloaded directory.

  **FINDING 2 (MEDIUM) — basename collision in the artifact index let a decoy shadow the real build
  output.** Root cause: `verify-release-artifacts.rs` indexed artifacts as `HashMap<basename, PathBuf>`
  via `index.entry(name).or_insert_with(...)` over a `read_dir` walk of `--search src-tauri/target` — a
  build dir `swatinem/rust-cache` restores between runs, i.e. not guaranteed clean. First-wins, in
  whatever order the OS happens to visit files. Independently reproduced RED on this machine
  (`basename_decoy`: a decoy directory sorting/reading first held the bytes a signature verifies
  against; a differently-named "real build output" directory held different bytes and was never read —
  `EXIT=0`), matching the auditor's finding once the fixture's byte assignment was corrected to put the
  *signed* bytes in the decoy (an earlier attempt with the assignment reversed correctly failed instead,
  which is itself useful: it shows the crypto check was never the broken part — only which file got
  indexed was).

  **Fix.** `verify-release-artifacts.rs`: the index build now tracks any basename seen more than once
  across all `--search` dirs in a `BTreeSet`, and hard-fails before even attempting to locate the
  manifest if the set is non-empty — `verify-release-artifacts: ambiguous artifact basename(s) found
  more than once under the search dirs -- refusing to guess which one is real: <names>`. This is a
  binary-level fix (applies regardless of which directory is searched), so it also protects the new
  Finding-1 job's `release-assets` download directory, and any future/manual invocation against a dirty
  `--search` dir. The new post-matrix job additionally sidesteps this in practice — a freshly-downloaded
  directory has nothing stale to collide with — but the auditor's fixture proves the collision-detection
  fix itself works independent of that.

  **FINDING 3 — a stray committed `latest.json` at the repo root would have turned the fail-closed
  "manifest missing" case into a silent pass.** tauri-action skips writing `latest.json` at all when it
  finds no updater `.sig` artifacts ("Signature not found for the updater JSON. Skipping upload...") —
  today that's fail-closed, since round 1's `--manifest latest.json` (repo-root-relative) then finds
  nothing and fails loud. A committed stray file at that same path would survive `actions/checkout` and
  make that same lookup find something real (if stale/fake) instead of nothing. **Note:** round 2's
  Finding-1 redesign already structurally closes this specific hole for the verify step itself — the
  new `verify-published-manifest` job never reads a local `latest.json` at all, it downloads the real
  one from the release via `gh release download`. Implemented the requested hardening anyway, as
  defense in depth against any other tooling/human trusting a stray committed copy: (1) `.gitignore`
  gained `/latest.json` with a comment explaining why; (2) `release.yml`'s `release` job now runs `rm -f
  latest.json` immediately before the "Build and publish release" (tauri-action) step, on every matrix
  leg, so whatever tauri-action itself writes (or doesn't) is the only thing that can ever exist there.
  **Judgment call, logged:** did not add the "assert the file is newer than the job start" check the
  Foreman called optional ("consider") — with rm-f-before-build + gitignore, and no step in `release.yml`
  reading a local repo-root `latest.json` for verification purposes anymore, the residual risk a
  freshness check would guard against is already closed structurally.

  Demonstrated red/green for the `.gitignore` half: stashed the `.gitignore` change, wrote a dummy
  `latest.json` at the repo root — `git status --short` showed `?? latest.json` (RED: a normal,
  stageable untracked file) and `git check-ignore -v latest.json` exited 1 (not ignored). Restored the
  fix, repeated — `git status --short` showed nothing (GREEN: not stageable) and `git check-ignore -v`
  reported `.gitignore:75:/latest.json  latest.json` (ignored). Working tree confirmed clean afterward.

  **FINDING 4 (LOW) — the watchdog's `== 'failure'` gate missed `startup_failure`/`cancelled`/
  `timed_out`.** `.github/workflows/release-pipeline-watchdog.yml`: changed the job's `if:` from
  `github.event.workflow_run.conclusion == 'failure'` to `!= 'success'`, so a workflow that never
  started, was cancelled, or hit its own timeout now also files/dedupes an issue and goes red, instead
  of silently satisfying neither the old condition nor producing any signal.

  **Nit — dropped `2>&1` from the dedupe capture.** `list_output` no longer merges `gh`'s stderr into
  the captured stdout on a *successful* `gh issue list` call, which previously risked a `gh` warning
  making `existing` non-numeric and breaking the follow-up `gh issue comment` call; stderr still reaches
  the job log on its own, unredirected.

  **Dedupe-logic re-verification (all 4 changes together).** Re-ran the fake-`gh` harness from round 1
  against the updated script (`!= 'success'` + dropped `2>&1`), three scenarios, via a `PATH` built with
  POSIX-style (`/c/Users/...`) segments per the explicit lesson from #1013/#1015 — with a hard abort
  check (`resolved=$(PATH=".../fakebin:$PATH" which gh); [ "$resolved" = ".../fakebin/gh" ] || exit 1`)
  run *before* any scenario, confirming the real `gh` binary was never shadowed-out. All three logged
  `gh` calls came from the fake stub only: (1) simulated `gh issue list` failure → exit 1, zero
  `issue create`/`comment` calls (CPE-1794 regression check still passes); (2) no existing issue →
  `gh issue create` called once, exit 1 (backstop); (3) existing issue `#42` → `gh issue comment 42`
  called, no `gh issue create` (no duplicate). No real GitHub issue was touched during this round's
  testing.

  **Environment note.** The Bash tool became unresponsive partway through this round (even trivial
  `echo` calls timed out) — a raw `bash.exe` spawn measured ~8s just to start, consistent with the
  machine being under heavy concurrent load rather than a bug in this ticket's changes. Switched to
  PowerShell (directly, and via `& "...\bash.exe" <script>` for the bash-specific dedupe harness) for
  the remainder of this round's verification; all reported results are real command output, not
  inferred.

  **Verification.** `cargo test --release` in `crates/updater-verify`: 24/24 pass (14 lib + 10
  integration, including the 4 new security-audit regression tests —
  `smuggled_extra_platform_is_rejected`, `smuggled_local_name_is_rejected` (control),
  `basename_decoy_is_rejected`, plus the two round-1 tests whose expected success-message string
  updated to match the new "verified N of M" wording). `cargo clippy --all-targets -- -D warnings`
  clean. Both workflow YAMLs parse under `yaml.safe_load`; every `run:` block (including the PowerShell
  one, which is expected to fail `bash -n` and was excluded) passes `bash -n`.

  **What still can't be proven pre-tag.** Same as round 1: no version tag was pushed (forbidden). The
  next tagged release must show: the `verify-published-manifest` job actually running and printing `OK:
  verified N of N platform signature(s) against the configured pubkey.` (N = however many platforms
  that release's matrix built, i.e. 3 today) rather than any earlier failure message; `catalog` running
  (not skipped); and, if anything about that run or a later one goes red, a `release-pipeline-red`
  issue appearing within minutes via `release-pipeline-watchdog.yml`.

- **2026-08-23 (Worker, CPE-1872 round 3 — final two audit findings, attempt 3 of 3)** — Same worktree/
  branch. The re-audit at `9fcfd163` confirmed round 2's fixes correct (both original vacuous passes
  closed, all eleven original fixtures + 7 vacuous-pass-route probes now non-zero/correct, Findings 1/3/4
  closed and verified closed). Two findings remained, both in the round-2 redesign itself, both fixed
  here.

  **FINDING A (HIGH) — the gate vanished on a partial matrix failure.** Root cause:
  `verify-published-manifest` had `needs: release` and no job-level `if:`. Default GitHub Actions
  semantics for `needs` run the dependent job only when EVERY needed job's result is `success` —
  anything else (including `failure`) makes the dependent job `skipped`, not itself failed. `release` is
  a `fail-fast: false` matrix; on a partial failure the surviving legs still run tauri-action to
  completion and upload their installers + a merged `latest.json` to the draft BEFORE the overall
  `release` job result registers as `failure`. Net effect: a partial matrix failure produced a fully
  populated, entirely unverified draft, and the verify job silently never ran at all — worse than round
  1, where each surviving leg at least checked its own local artifacts. Confirmed by reading GitHub's own
  `needs`-skip semantics; no fixture needed to demonstrate a YAML conditional's documented behavior.

  Fix: `.github/workflows/release.yml` — added `if: ${{ !cancelled() }}` at the `verify-published-
  manifest` job level (only an actual workflow cancellation skips it now; `release` succeeding, failing,
  or partially failing all cause this job to run and attempt to verify whatever the matrix left behind —
  which will legitimately fail loud if the draft is incomplete/unverifiable, turning "no gate" into "red
  gate"). Also documented, per the Foreman's note, why this job deliberately does NOT declare its own
  `permissions:` block (it needs the inherited workflow-level `contents: write` — a draft release 404s on
  the unauthenticated `GET /releases/tags/{tag}` lookup, and `gh` falls back to a listing call that needs
  push access; a later "narrow this job's permissions" edit would silently break draft access).

  Downstream half (explicitly called out as ours to fix too): `.claude/commands/run.md` step 1b published
  a draft after checking only that it had installer assets — no check that verification passed. Added a
  new **1b-ii** between the existing asset check (1b) and publish (1c): looks up the `release.yml` run for
  the tag, reads the `verify-published-manifest` job's `conclusion` via `gh run view --json jobs`, and
  requires `success` or `skipped` (the legitimate no-signing-key case) before allowing 1c to run at all —
  `failure`/`cancelled`/a missing job throws and stops the publish.

  **FINDING B (MEDIUM) — the round-2 fix closed the local-collision half of the url problem, not the
  binding half.** Root cause: both the download step (`jq -r '.platforms[].url' | sed 's#.*/##'`) and the
  verifier (`basename(url)` → local index lookup) reduce every platform `url` to its LAST path segment —
  the host and every path segment before the basename (repo, release tag) are compared against nothing.
  A manifest can carry a real, correctly-signed artifact under a `url` pointing at a foreign host, or at
  the right repo but the wrong tag, and this pipeline verifies it clean: the bytes are genuine (`gh
  release download` only ever pulls from THIS repo's THIS tag, regardless of what the manifest's `url`
  claims), but the manifest that ships tells every real updater client to fetch from wherever that url
  actually points. Reproduced exactly per the auditor's fixtures:
  - RED: `n1_foreign_host_same_basename` (url = `https://evil.example/pwn/<basename>`) — `EXIT=0`,
    `OK: verified 1 of 1 platform signature(s)...`.
  - RED: `n2_wrong_tag_same_basename` (url = right repo, tag `v0.0.1` instead of the real one) —
    `EXIT=0`, same false-pass message.

  **(binary vs. shell-grep) reasoning, logged as instructed:** implemented the binding check as a new
  `--expect-url-prefix <prefix>` flag on `verify-release-artifacts` (lib.rs gained `pub fn
  platforms_with_url_outside_prefix(manifest_json, expected_prefix) -> Vec<(platform, url)>`), enforced
  in the SAME binary that does the crypto check, rather than a `grep`/prefix-check inline in the download
  step's bash. Reasoning: (1) it lives at the exact site the ticket's own invariant names — "the
  signatures are checked over the bytes that actually ship" — extended to "and the url that ships with
  them points where we say it does"; (2) it is covered by this crate's own test suite (5 new lib unit
  tests + 5 new integration tests) the same way every other manifest invariant already is, rather than
  being an untested shell one-liner a future edit to the download step could silently drop; (3) it keeps
  the download step's job (fetch what the manifest already names, individually, failing loud on a
  missing asset) separate from the verify step's job (decide whether what was fetched — and what it
  claims — can be trusted), which is the same separation of concerns the round-2 redesign already used
  between "download" and "verify".

  Fix: `crates/updater-verify/src/lib.rs` — new `platforms_with_url_outside_prefix`.
  `crates/updater-verify/src/bin/verify-release-artifacts.rs` — new `--expect-url-prefix` CLI flag,
  checked immediately after the manifest is read (before the crypto pass), failing loud and listing every
  offending platform + its url if any `url` doesn't start with the given prefix. `release.yml`'s "Verify
  the published manifest + signatures" step now always passes `--expect-url-prefix
  "https://github.com/${REPO}/releases/download/${TAG}/"` (`REPO`/`TAG` from `github.repository` /
  `github.ref_name`, already in scope). GREEN re-run of both fixtures with the flag now set: `EXIT=1`,
  `manifest platform url(s) do not start with the expected prefix '...' -- refusing to trust a manifest
  that could point real updater clients at unexpected infrastructure even though the artifact
  bytes/signatures may check out: windows-x86_64 -> https://evil.example/...` (and the wrong-tag
  equivalent). Control test (`matching_prefix_url_still_passes_with_url_prefix_check_enforced`) confirms
  a genuine matching-prefix url still passes with the check turned on — the fix doesn't just fail
  everything.

  **Watchdog note (not a finding, logged as asked).** `.github/workflows/release-pipeline-watchdog.yml`:
  added a comment acknowledging that the round-2 `!= 'success'` widening also fires on a deliberately
  cancelled release (a human cancelling a run they already know is broken) — accepted noise, not a bug;
  the alternative (narrowing back to catch fewer `cancelled` cases) risks missing a REAL
  runner-died-mid-job cancellation nobody triggered on purpose, which is exactly the blind spot Finding 4
  closed.

  **Verification.** `cargo test --release` in `crates/updater-verify`: **34/34 pass** (19 lib + 15
  integration — 5 new lib tests for `platforms_with_url_outside_prefix`, 5 new integration tests
  reproducing `n1`/`n2` RED-then-GREEN plus a matching-prefix control). `cargo clippy --all-targets --
  -D warnings` clean. `.github/workflows/release.yml` and `release-pipeline-watchdog.yml` both parse
  under `yaml.safe_load`; every non-PowerShell `run:` block (11 in `release.yml`, 1 in the watchdog)
  passes `bash -n`. Dedupe-logic fake-`gh` harness was NOT re-run this round (no dedupe-logic changes in
  round 3) — round 2's three-scenario proof stands unchanged.

  **Environment note.** The Bash tool remained intermittently slow/unresponsive through this round too
  (short timeouts still hit occasionally; longer ones and PowerShell were reliable). Used PowerShell +
  `Read`/`Edit` tools for all file changes and verification, consistent with round 2. No real `gh`
  command reached the repo this round — the dedupe harness from round 2 was not re-invoked since nothing
  in round 3 touched its logic.

  **What still can't be proven pre-tag.** Unchanged: no version tag pushed. The next tagged release must
  additionally show `verify-published-manifest` running (not skipped) even in the face of any partial
  matrix trouble, and — if a future release ever legitimately needs a foreign/relocated download host —
  that `--expect-url-prefix` would need a deliberate, reviewed change here rather than silently accepting
  it; that is by design, not a gap.
