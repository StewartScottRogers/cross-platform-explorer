---
id: CPE-1908
title: the channel-purity guard runs only on the plain manifest — the sidecar channel, which users actually install, has none
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

CPE-1894 fixed the trigger that let plain and sidecar installers land in one release, and added
`platforms_with_mismatched_channel` — a real guard that reads the produced manifest's own asset URLs
and names every platform served from the wrong channel. Verified working against the live contaminated
manifest by both its reviewer and its UAT.

**It only ever runs against the plain channel.**

`verify-release-artifacts.rs` derives the expected channel from whatever `--conf` points at, and
`release.yml` always passes `src-tauri/tauri.conf.json` — the plain config. `release-sidecar.yml` never
invokes that binary at all; it runs a different check entirely. So the channel that **actually reaches
users** has zero channel-purity assertion on its manifest, and only the plain channel — whose accidental
contamination is the *less* consequential direction — is guarded.

Both independent legs flagged this separately, without seeing each other's report. Neither blocked
CPE-1894 on it, correctly: that ticket promised the trigger fix plus a checkable guard, and delivered
both. This is the follow-through.

## Why it matters, traced concretely

CPE-1894's UAT established the mechanism rather than assuming it:

- `plugins.updater.endpoints` is a single URL in the base config, and `tauri.sidecar.conf.json`
  overrides only `productName`, `identifier` and `bundle.createUpdaterArtifacts` — **not** the endpoint
  and **not** the pubkey. So a sidecar-build app checks the same endpoint as a plain one.
- Both workflows sign with the same `TAURI_SIGNING_PRIVATE_KEY`, and the pubkey baked into both builds
  is identical.
- Therefore a sidecar app on an affected platform fetches the manifest, resolves its platform key to a
  **plain** asset, and the updater's signature check **passes** — it is a genuine signature from the
  right key, for the wrong product. Nothing in that check distinguishes channel, and the version string
  is identical either way, so there is no visible signal to the user.

What could not be established without running an installer, and is worth resolving: whether the plain
installer overwrites the sidecar install in place (losing the AI Console) or lands as a second app,
since the two channels carry different `identifier`s. Both outcomes are bad; they differ in how bad.

## Acceptance criteria

- [x] Run the channel-purity check against the **sidecar** manifest too, with the sidecar config as its
      expected-channel source. Wire it into `release-sidecar.yml` the way CPE-1894 wired the plain one.
- [x] Make it **preventive** rather than detective if you can. On the plain channel the check is
      `needs: release`, so installers are already uploaded to the draft by the time it runs; on the
      sidecar channel there is a `needs:`-gated pattern available (CPE-1873's `verify-updater-pin`
      demonstrates it) that skips build/sign/publish outright on failure. Prefer that shape.
      **Investigated and NOT fully achievable for this specific check** — see Work Log: the
      channel-purity check needs the manifest's own published asset URLs, which don't exist until every
      matrix leg has uploaded, so it structurally can't run before the build the way the static pin
      check does. Made it the same shape as the plain channel's own equivalent gate instead (a
      `needs:`+`if: ${{ !cancelled() }}` post-matrix job that RED-gates the draft before a human
      publishes it) — the closest available analogue, applied symmetrically to both channels.
- [x] Red-proof it: construct a mixed sidecar manifest, confirm the guard names the mismatched platforms
      and fails the job, and confirm a uniform sidecar manifest passes.
- [x] Decide what happens to a manual `gh release edit --draft=false`. The plain channel's only
      protection today is `run.md`'s check of the job's conclusion — a hand-run publish bypasses it
      entirely. Say whether that is acceptable and why, or close it.
      **Decision (round 2, corrected): wired `/run` to cover it, rather than filing a follow-up.**
      Round 1 of this ticket claimed "there's no `/run`-style automated sidecar publish flow to wire
      it into" — that was a factual error a Reviewer caught: `run.md` step 1a always installs the
      *latest* release regardless of channel, and this project's shipping strategy is sidecar-only, so
      most tags `/run` reaches ARE `-sidecar` tags. `/run` *is* the de facto publish path for this
      channel; it only failed safe by accident (a hard-coded `--workflow=release.yml` lookup that
      throws on a sidecar tag instead of silently passing). Fixed: `release-sidecar.yml` now sets
      `run-name: "Release (sidecar) ${{ inputs.tag }}"` (workflow_dispatch runs have no tag-bearing
      `headBranch` to match on, unlike a tag-triggered run, so the tag has to be surfaced some other
      way), and `run.md` step 1b-ii branches on the `-sidecar` tag suffix, resolving the run via
      `displayTitle` and checking `verify-published-manifest-sidecar` instead of
      `verify-published-manifest`. RELEASING.md's manual `gh` check (added round 1) stays, for
      publishing outside `/run` entirely. Residual gap, same as the plain channel's always-had one:
      nothing on GitHub can force a human to run either check before a fully manual
      `gh release edit --draft=false` that bypasses both `/run` and RELEASING.md's own instructions —
      accepted for the same reason the plain channel's identical gap is (a CI job can make the failure
      loud, not physically unbypassable; the publish step is deliberate and manual either way). This is
      narrower than round 1's claim, now that the common case (`/run`) is actually covered.

## Notes

Filed 2026-08-26 from CPE-1894's independent reviewer **and** its UAT, which raised it separately.

Related: **CPE-1894** (the trigger fix and the guard), **CPE-1909** (the case-sensitive negation gap
found in the same review), **CPE-1872** / **CPE-1874** (the plain channel's verification history),
**CPE-1873** (the `needs:`-gated preventive pattern worth copying).

See [[always-install-sidecar-build]] for why the sidecar channel is the one that matters here: the
standing rule is that installs and updates always use it, so an unguarded sidecar manifest is the guard
gap that can actually reach a user.

## Work Log

- **2026-08-26 USMST** — Implemented:
  1. `crates/updater-verify`: added `--expect-channel <plain|sidecar>` to `verify-release-artifacts`
     (plus `impl FromStr for Channel` backing it, with unit tests). Needed because
     `release-sidecar.yml`'s `--conf` must stay the BASE `tauri.conf.json` (it's the only file with
     `pubkey`/`version`/the CPE-1873 pin — `tauri.sidecar.conf.json` is a partial overlay with none of
     those, confirmed by reading it), so the expected channel can no longer be derived from `--conf`'s
     own `productName` (always "Cross-Platform Explorer", plain) the way `release.yml`'s invocation
     does — it has to be declared explicitly. `release.yml`'s existing invocation now also passes
     `--expect-channel plain` explicitly (behaviourally a no-op — it already derived Plain from
     `--conf`'s productName — but makes both channels' invocations symmetric and machine-checkable).
  2. `release-sidecar.yml`: new job `verify-published-manifest-sidecar`, `needs: [create-release,
     release-sidecar]`, `if: ${{ !cancelled() }}` (same CPE-1872-round-3 reasoning as release.yml's
     `verify-published-manifest` — `release-sidecar` is `fail-fast: false`, so a bare `needs:` would
     silently SKIP this on a partial matrix failure, the exact class of bug CPE-1872/1893 already
     fixed elsewhere in this repo). Downloads the manifest by the `inputs.tag` workflow_dispatch input
     (this workflow has no `github.ref_name` — it's dispatch-only), runs `verify-release-artifacts`
     with `--conf src-tauri/tauri.conf.json --expect-channel sidecar --expect-url-prefix ...`.
  3. **Preventive-vs-detective (AC #2), investigated and resolved as "as preventive as this specific
     check can be":** `verify-updater-pin`'s block-before-build shape works because it checks STATIC
     config that exists before a single byte is built. The channel-purity check asserts on the
     manifest's own published asset URLs — those don't exist until every one of the 3 matrix legs has
     uploaded to the draft, so there is no earlier point in the pipeline where "is this manifest
     channel-pure" even has an answer. Made this job the SAME shape as the plain channel's own
     equivalent (`needs:`+`if: !cancelled()`, a post-matrix RED gate on the draft before a human
     publishes) rather than inventing a different, weaker pattern — documented the reasoning inline in
     both the ticket and the workflow comment so it isn't re-litigated as "why didn't this copy
     verify-updater-pin exactly" later.
  4. Red-then-green proof, two layers:
     - `crates/updater-verify/tests/release_guard.rs`: 4 new end-to-end tests driving the real binary —
       `a_plain_asset_in_a_manifest_expected_sidecar_is_rejected_by_name` (RED: a plain-channel asset
       smuggled into an otherwise-sidecar manifest, checked with `--conf` carrying the ordinary PLAIN
       productName + `--expect-channel sidecar` exactly as the real job invokes it, fails and names the
       offending platform without falsely flagging the honest one), a mirror-direction control proving
       the flag — not the productName — decides the expectation, and
       `a_uniform_sidecar_manifest_passes_with_expect_channel_sidecar` (GREEN: a 3-platform, fully
       sidecar-named manifest passes clean under the exact same `--conf`+flag combination). Full crate
       suite: 45 lib unit tests + 21 release_guard end-to-end tests + 2 pin-guard + 1 platform-config,
       all green. `cargo clippy --all-targets -- -D warnings` clean (crate has no feature flags, so
       there is only one mode to run — same as CPE-1894's own note).
     - `src/lib/channelPurityCoverage.test.ts` (new, addresses the "make coverage impossible to
       silently lose again" ask): reads `Channel`'s variant names straight out of
       `crates/updater-verify/src/lib.rs` (no hand-duplicated list) as the canonical channel set, and
       asserts every variant has a real `--expect-channel` invocation in `release.yml`/
       `release-sidecar.yml`, via `parseYaml` structural parsing (not raw-text regex, per this repo's
       established convention after `catalogPublishFreshnessGuard.test.ts`'s review round). Proved the
       ratchet actually catches a regression: temporarily rewrote `--expect-channel sidecar` to a
       disabled spelling in `release-sidecar.yml`, ran the test — 2 failures, exactly naming
       `["sidecar"]` as missing — then reverted and confirmed green again. This is what makes "add a
       third channel without guarding it" (or "silently drop the sidecar wiring") fail CI going
       forward instead of shipping unnoticed the way this ticket's own defect did.
  5. **AC #4 (manual `gh release edit --draft=false`), decided:** added the equivalent manual
     verification step to `RELEASING.md`'s sidecar publish section (`run.md` has no automated sidecar
     publish flow to extend — sidecar releases are dispatched and published by hand per RELEASING.md's
     existing documented flow, unlike the plain channel's `/run`). Same residual limitation the plain
     channel already accepts, stated explicitly in both RELEASING.md and the workflow comment: no
     GitHub mechanism can force a human to check a job's conclusion before running a manual `gh`
     command; CI can only make the failure loud (a red run in the Actions tab) rather than physically
     unbypassable. Not treated as a new gap needing its own follow-up ticket — it's the same trade the
     plain channel already made, now applied symmetrically.
  6. `npm run check` clean. Did not touch `.github/workflows/ci.yml` (a sibling worker's file per the
     dispatch note) or any signing-key material.
- **2026-08-27 USMST** — Round 2, independent Security Auditor + Reviewer passes on PR #1039. Both
  confirmed the round-1 fix itself is correct and load-bearing (ran the real invocation against the
  live contaminated `v0.57.69-sidecar` manifest, got exit 1 naming exactly the five known-bad platform
  keys; confirmed the job can't pass vacuously in six traced failure modes), but found the round-1
  coverage RATCHET (`channelPurityCoverage.test.ts`) was itself under-guarded in five distinct ways,
  and that the AC #4 decision rested on a factual error. All fixed on the same branch:
  1. **H2/H3 (job wiring can be silently disabled):** the round-1 detector matched only `step.run`
     TEXT — hard-disabling the whole job (`if: ${{ false }}`), DELETING its `if:` line outright
     (restoring the exact bare-`needs:` silent-skip shape CPE-1872/CPE-1893 exist to prevent), or
     neutering the step's own `if:` all still passed 5/5. Fixed with `isActuallyWired()`: every
     coverage assertion now checks the job's `if:` is EXACTLY `${{ !cancelled() }}`, `needs:` names
     the real build job, and the step's own `if:` is the real secret gate.
  2. **H1 (flag matched inside a shell comment):** commenting out `--expect-channel` (a realistic
     "unblock a red release" edit) still counted as coverage, and the binary then falls back to a
     productName-derived `plain` expectation — an all-plain manifest under a `-sidecar` tag would
     pass. Fixed by extracting `stripShellComment`/`logicalLines` out of `releaseHangHardening.test.ts`
     into `src/lib/preview/shellScriptLines.ts` (CPE-1849's already-reviewed comment/continuation
     handling) and using it here instead of a second hand-rolled stripper.
  3. **H4 + extension (undercounted variants):** the enum-variant parser only matched a bare
     `Ident,`, so `Beta(String),` OR `Beta = 3,` both vanished from the canonical list silently.
     Fixed with a depth-aware top-level-comma splitter (`splitTopLevelVariantSegments`) that extracts
     every variant's identifier regardless of payload/discriminant, asserting the parsed segment
     count equals the enum body's real non-comment/non-attribute line count.
  4. **The "false RED" trap:** reading the Rust IDENTIFIER's spelling meant a pure, harmless rename
     (`Channel::Sidecar` → `Channel::SidecarBuild`; `FromStr` still accepts `"sidecar"`, nothing
     breaks) made the ratchet go red and recommend `--expect-channel sidecarbuild` — a value the
     binary actually REJECTS, which would have broken a real release. Fixed at the root in BOTH
     languages: `crates/updater-verify/src/lib.rs` gained `Channel::ALL` + an `exhaustiveness_guard`
     match (no wildcard arm — a new variant fails to COMPILE until handled) +
     `channel_display_fromstr_round_trip_covers_every_variant` (proves Display's output always parses
     back via FromStr, for every variant, regardless of identifier spelling); the TS test now reads
     `Display`'s string LITERALS via `readCanonicalChannelTokens()`, not the Rust identifiers, so a
     pure rename is a non-event there too.
  5. **R2:** added `timeout-minutes` (10 for the download step, 8 for the verify step) to
     `verify-published-manifest-sidecar` — this new job had none of the CPE-1824 hang hardening the
     rest of `release-sidecar.yml` carries. `release.yml`'s identical sibling gap on
     `verify-published-manifest` is pre-existing and deliberately left untouched (outside this
     ticket's diff surface, not something this PR introduced).
  6. **§4, the AC #4 decision was factually wrong, now corrected:** round 1 claimed "no `/run`-style
     automated sidecar publish flow to wire it into". Wrong — `run.md` step 1a always installs the
     *latest* release regardless of channel, and this project ships sidecar-only, so `/run` *is* the
     de facto sidecar publish path; it only failed safe by accident (a hard-coded `release.yml`
     lookup that throws on a `-sidecar` tag instead of silently passing). Fixed by WIRING it rather
     than filing a follow-up: `release-sidecar.yml` now sets
     `run-name: "Release (sidecar) ${{ inputs.tag }}"` (a `workflow_dispatch` run has no tag-bearing
     `headBranch` to match on otherwise — unlike `release.yml`'s tag-triggered runs), and `run.md`
     step 1b-ii branches on the `-sidecar` tag suffix, resolving the run via `displayTitle` and
     checking `verify-published-manifest-sidecar`. Corrected the false premise in `RELEASING.md` and
     this ticket's own round-1 Work Log entry (left uncorrected in place above, superseded here) — a
     manual `gh release edit --draft=false` that bypasses `/run` AND `RELEASING.md`'s own instructions
     entirely is still unguarded, same residual limitation the plain channel's gate always had.
  - **Red-then-green, every finding, each demonstrated interactively and reverted via `git checkout`
    before landing the real fix (fix committed first, then probed, per instruction):**
    - Job `if: ${{ false }}` → RED (`no ACTUALLY-WIRED ... guards: ["sidecar"]`) → reverted → GREEN.
    - Job `if:` line deleted entirely → RED (same message) → reverted → GREEN.
    - Step `if: steps.sig.outputs.has == 'true'` neutered to `if: false` → RED → reverted → GREEN.
    - `--expect-channel sidecar` commented out (reviewer's exact scenario, a `# TODO: re-enable...`
      line plus the trailing `\` removed) → RED → reverted → GREEN.
    - `needs: [create-release, release-sidecar]` → `[create-release]` (build job dropped) → RED →
      reverted → GREEN.
    - `Beta(String),` added to `Channel` with no `Display` arm → RED (loud "no Display arm" failure,
      whole suite errors rather than silently passing). With a matching `Display` arm added too → RED
      at the union-coverage check instead, correctly naming `"beta"` as unguarded — this ALSO caught a
      real bug in my own regex (it didn't tolerate a tuple-variant binding pattern between the
      identifier and `=>`, so a legitimately-written arm still read as missing); fixed the regex,
      re-ran, got the correct RED. Reverted → GREEN.
    - `Beta = 3,` (discriminant variant) → same RED/RED/GREEN sequence as `Beta(String)`.
    - `Channel::Sidecar` → `Channel::SidecarBuild` (pure rename, real Rust identifiers only, no string
      literals touched) → Rust: `cargo test` 70/70 still green (round-trip test proves `Display`/
      `FromStr` stay consistent) → TS: **stayed GREEN** (6/6) — the fix working as intended. Also ran
      the actual round-1 (pre-fix) test file (extracted via `git show c75c99c9:...` into a throwaway
      `src/lib/__round1_scratch_*.test.ts`, deleted after) against this same rename: it went RED
      exactly as the Reviewer predicted (`expected [ 'sidecarbuild', 'plain' ] to include 'sidecar'`),
      concretely proving the trap existed before this fix and is closed after it.
  - Full validation after all fixes: `cargo test --locked` in `crates/updater-verify` 70/70 green
    (46 lib + 21 release_guard + 2 pin-guard + 1 platform-config), `cargo clippy --all-targets -- -D
    warnings` clean, `npm run check` clean, full `vitest run` 4609/4611 green (the 2 failures are a
    pre-existing `ci.yml` `msrv:`-job gap on `main` itself — confirmed via
    `git diff --stat origin/main -- .github/workflows/ci.yml` showing zero diff, untouched by this
    branch, a sibling worker's file per the original dispatch note).
