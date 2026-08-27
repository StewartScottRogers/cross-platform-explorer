---
id: CPE-1893
title: the signed agent catalog has not published since 2026-07-25, because `catalog` needs a job that always fails
type: bug
priority: High
status: Doing
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

`release.yml`'s `catalog` job is declared `needs: release` with **no `if:` condition**. `release` has
failed on every tagged run since 2026-07-26, so `catalog` has been reported `skipped` on all ~15 of
them. Net effect, unnoticed for a month: **the signed agent-catalog bundle has not been published
since 2026-07-25.**

This is a distinct failure from the one CPE-1874 covers. CPE-1874 is about the *verification step*
being dead. This is about a *publishing* job that has silently produced nothing for 31 days because
it was chained behind that dead step. Nobody flagged it because `skipped` reads as benign in the
run summary — it is the same shape as a path-filtered job that had nothing to do.

The catalog pipeline itself is built and activated (CPE-308), and the signing key is configured, so
this is not a "never finished" — it is a working feature that stopped shipping and said nothing.

Found 2026-08-26 by CPE-1873's independent Security Auditor while establishing whether that PR's
green depended on the broken release job. It did not; this turned up alongside.

## Acceptance criteria

- [x] Determine what consumers do when the catalog goes stale for a month — does the app fall back,
      pin, or silently serve nothing? Record the answer; it decides whether this is High or Critical.
- [x] Decouple `catalog` from `release`'s success where that is correct, or make the skip **loud**.
      A publishing job that produces nothing must not report as `skipped` indistinguishably from a
      job that had nothing to do.
- [x] Add a freshness check that fails when the published catalog is older than a chosen threshold,
      so the next month-long gap surfaces on its own rather than waiting for an auditor.
- [x] Red-proof it: arrange the failing-`release` condition, observe the new signal fire, restore.
- [x] Do not fix `release.yml`'s underlying verify failure here — that is CPE-1872 / CPE-1874.

## Notes

Evidence: last successful `release.yml` run was `v0.57.33` on 2026-07-25. Every run from
`v0.57.35-sidecar` (2026-07-26) through `v0.57.69` / `v0.57.69-sidecar` (2026-08-23) failed on all
three legs at `Verify updater manifest + signatures (CPE-1058)`. Confirmed on runs `30219127836`,
`31133248284`, `32645894722`, `32645968177`.

Note the corrected window: the outage is **31 days from 2026-07-26**, not the 27-days-from-2026-08-04
figure carried in earlier tickets and in the prior run's checkpoint. Correct that where it appears.

Related: **CPE-1874** (the releases that shipped without their signatures verified), **CPE-1872**
(the redesigned `verify-published-manifest` job — note it merged 2026-08-24, *after* the newest tag,
so it has **never executed** and is entirely unexercised in production), **CPE-308** (the catalog
auto-update pipeline this job publishes for).

## Work Log

- **2026-08-26 USMST** — Picked up by a Worker. Rebased onto current `main` (already up to date — no
  drift found; CPE-1894's disjoint-tag-trigger fix is already in `release.yml`). Plan:
  1. Answer the consumer question first (it decides severity) by reading the actual fetch/apply/UI
     code, not guessing.
  2. Decouple `catalog` from `release`'s job-level pass/fail via `if: ${{ !cancelled() }}` — the same
     expression CPE-1872 already established for the sibling `verify-published-manifest` job, for the
     identical reason (fail-fast:false means surviving matrix legs still populate the release even
     when the job's own status reads failure).
  3. Add an independent, schedule-driven freshness check as a new workflow (`catalog-freshness.yml`),
     because the job-level fix alone only closes ONE way this can recur — it says nothing about a
     genuinely-succeeding-but-empty `catalog` job (e.g. a revoked signing secret), and nothing at all
     runs if no release is tagged for weeks.
  4. Red-proof both without touching real release CI (per Foreman instruction: no tag pushes, no
     `workflow_dispatch` of a release workflow, no CI polling) — structural guard tests over the
     parsed YAML (this repo's established convention, `src/lib/*.test.ts` + `preview/yaml.ts`) plus
     actually EXECUTING the freshness arithmetic locally against fabricated timestamps.

- **2026-08-26 USMST** — **Consumer question, answered from the real code** (not speculation):
  Traced the full path: `src-tauri/src/lib.rs::do_fetch_catalog` (default `catalog_url()` =
  `https://github.com/<repo>/releases/latest/download/`) → `sidecar/host/src/catalog.rs::apply_bundle_with`
  (index-signature check, content-hash binding, anti-rollback) → the AI Console UI's `refreshCatalog()`
  in `sidecar/ai-console/src/launcher.html` (~line 1799).

  **The trust/integrity engine itself is fine and stays fine** — a bad/missing/tampered index touches
  nothing (`index_ok == false` ⇒ last-known-good, `sidecar/host/src/catalog.rs` doc comment + its own
  test `a_bad_index_signature_touches_nothing_last_known_good`), and a same-or-older `version` is
  rejected as `EntryVerdict::Rollback`, never silently re-applied. So this was never a security hole:
  no attacker can exploit the gap, nothing gets corrupted, nothing downgrades. **It is purely an
  availability/freshness gap** — which is why "High" (not "Critical") stands, per the ticket's own
  test.

  **But it is a genuinely silent one, confirmed at the UI layer, not just inferred:**
  `refreshCatalog()` in `launcher.html` collapses two very different states into one reassuring
  message each:
  - `indexOk === true && applied === 0` (a fetch that "succeeds" but changes nothing — exactly what a
    month-old-but-still-served catalog looks like, since anti-rollback rejects the identical
    already-installed version every time) → **"Agents are already up to date."** — indistinguishable
    from genuinely being current.
  - `indexOk === false` (a hard fetch failure — offline, OR the URL is genuinely dead) →
    **"No agent update available (offline, or none published yet)."** — the message ITSELF conflates
    "you're offline" with "the pipeline is broken," by design, in the UI copy.
  Neither branch surfaces the `error` string `do_fetch_catalog` actually returns
  (`fetch_catalog_response` in `lib.rs` puts it in the JSON body; `handle_catalog_refresh` in
  `console.rs` never forwards it to its own HTTP response — only `indexOk`/`applied`/`agents`). So
  even someone staring at the AI Console during a broken month sees nothing distinguishing "healthy"
  from "dead." On top of that, catalog auto-update is opt-in (`autoUpdateCatalog`, CPE-378, off by
  default per `renderCatalogControls()`), so most users never even trigger a manual check.

  **Confirmed live, right now (2026-08-26), which apply**: `GET
  https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/catalog-index.json`
  → **HTTP 404** (checked via `curl`, and independently via `gh api repos/.../releases/latest`, which
  shows the current `/releases/latest/` is `v0.57.69-sidecar` — a sidecar-channel release; only
  `release.yml`, the PLAIN channel, ever runs the `catalog` job, so a sidecar-latest release has no
  catalog asset at all). So the CURRENT real-world failure mode is not "serves a month-old snapshot"
  but a hard fetch failure — a related but DISTINCT blind spot from this ticket's own (it's the
  multi-channel `/releases/latest/` ambiguity CPE-1894/1908/1909 already own, not something to fix
  here). Both failure shapes — stale-but-served, and outright-404 — get the same silent treatment
  from the UI described above, so the freshness check built below treats a 404 as its own confirmed
  alarm (not folded into "stale," not shrugged off as "inconclusive" either).

  **Verdict recorded**: consumers get **no visible signal at all** when the catalog goes stale or
  disappears — the app tells the user "you're up to date" / "nothing published" either way, whether
  that's true or the pipeline has been dead for a month. Severity stays **High**: real, invisible,
  now-confirmed availability gap; not Critical, because trust/integrity enforcement never weakens and
  nothing an attacker controls gets through it.

- **2026-08-26 USMST** — **Implemented**:
  1. `.github/workflows/release.yml`'s `catalog` job: added `if: ${{ !cancelled() }}` alongside its
     existing `needs: release`, plus a comment block explaining the CPE-1893 history and pointing at
     `verify-published-manifest`'s identical, already-reviewed pattern (CPE-1872 finding A) as the
     precedent. The job's own per-step secret gate (`steps.k.outputs.has == 'true'`, guarding
     Rust/dbus install + build/sign + upload) is untouched — it still skips gracefully when
     `CPE_CATALOG_SIGNING_KEY` genuinely isn't configured, which is a different, correct kind of
     "nothing to do" than the job-level bug this ticket is about. Net effect: a partial `release`
     matrix failure (some legs succeed, `fail-fast: false`, a release object exists) now lets
     `catalog` run and publish as before CPE-1893's regression window; a TOTAL `release` failure (no
     release object at all) now makes `catalog` run and FAIL LOUDLY at `gh release upload` (nothing to
     upload to) instead of vanishing as `skipped` — turning the one genuine dependency into a loud
     failure, per the ticket's own required shape, rather than leaving it silent.
  2. New workflow `.github/workflows/catalog-freshness.yml` — a scheduled (daily, 08:37 UTC),
     independent freshness backstop, deliberately NOT reactive to `workflow_run` (unlike
     `release-pipeline-watchdog.yml`, CPE-1872) because its subject — "is the artifact a real client
     fetches right now too old" — keeps drifting even when no release workflow runs at all; only a
     wall-clock schedule catches that. Checks the EXACT default URL `do_fetch_catalog` requests
     (`.../releases/latest/download/catalog-index.json`), 3-way verdict mirroring
     `ffmpeg-pin-freshness.yml`'s established shape (CPE-1763): HTTP 200 → evaluate age; HTTP 404 →
     its own confirmed alarm (no catalog live at all — the exact case confirmed live above); anything
     else → inconclusive, fails loud, never silently swallowed as a pass and never treated as
     confirmed staleness either. On a confirmed problem, files/dedupes a GitHub issue (`catalog-stale`
     label) the same deduped way `release-pipeline-watchdog.yml`/`ffmpeg-pin-freshness.yml` do
     (CPE-1794's lesson: a failed `gh issue list` lookup must never read as "no existing issue"), and
     the run itself stays red as the backstop for anyone not watching Issues.
     **Threshold: 7 days**, chosen and recorded deliberately (not a guess) — this repo cuts tagged
     releases roughly daily (CPE-1894's Work Log), and `catalog` now runs on every one, so a healthy
     catalog is almost always <48h old; 7 days absorbs a quiet stretch (weekend/lull) without a false
     alarm while still giving ~3 weeks of runway before silently reaching this ticket's own 31-day
     gap. **Cadence: daily** (not weekly like the ffmpeg canary) — this check is one cheap GET + a
     timestamp compare (no multi-asset HEAD sweep), so there's no cost reason to wait a week, and
     daily means a breach is caught within ~24h of crossing the threshold.
  3. `.github/workflows/scripts/catalog-freshness-check.sh` — the one copy of the age/staleness
     arithmetic (`catalog_age_days`, `is_catalog_stale`), sourced by the workflow AND directly runnable
     standalone, mirroring `ffmpeg-anchor-check.sh`'s (CPE-1796) established shape. Uses
     `catalog-index.json` entries' own `version` field as the publish timestamp — `catalog-sign`
     (`sidecar/host/src/bin/catalog_sign.rs`) already stamps it with `date +%s` at sign time for
     CPE-372's anti-rollback counter, so no second timestamp field needed anywhere.
  4. Structural guard test `src/lib/catalogPublishFreshnessGuard.test.ts` (uses the in-repo bounded
     YAML parser, `preview/yaml.ts` / CPE-1617 — the established convention `releaseHangHardening.test.ts`
     and siblings use, chosen specifically because a regex-over-raw-text guard can be satisfied by an
     unrelated neighbouring comment): asserts `catalog`'s `if:` equals `verify-published-manifest`'s
     `if:` (not just "is present" — so the two sites can't silently diverge), that its per-step secret
     gating is untouched, and that `catalog-freshness.yml` has a real `schedule.cron`, a
     `workflow_dispatch` test-hook input, `issues: write` permission, a non-zero threshold, and sources
     the shared script rather than reimplementing the math inline.

- **2026-08-26 USMST** — **Red-proofed, per the Foreman's no-CI-polling / no-release-workflow-dispatch
  instruction — entirely locally, no GitHub Actions run triggered:**
  - The freshness arithmetic was executed directly (not just asserted to exist) against a fixed
    reference epoch, via both a standalone `bash` invocation during this work AND a committed vitest
    suite that shells out to the same script (`spawnSync`, probed + gracefully skipped if `bash` isn't
    on PATH — same shape `releaseVersionBump.test.ts` uses for pwsh/powershell):
    - published 1 hour ago, threshold 7d → `fresh — age 0d`, exit 0.
    - published exactly 7 days ago (the boundary) → `fresh — age 7d <= threshold 7d`, exit 0 —
      confirms the strictly-greater-than comparison doesn't false-positive on the exact threshold.
    - published 8 days ago → `STALE — age 8d > threshold 7d`, exit 1.
    - published 40 days ago (this ticket's own real scale) → `STALE — age 40d > threshold 7d`, exit 1.
    All four match the intended verdict; the healthy case (0d, 7d) stays green, the broken case (8d,
    40d) goes red — satisfying "confirm the new signal fires" AND "confirm neither fires on a healthy
    run" without needing a live run.
  - The `catalog` job's `if: ${{ !cancelled() }}` decoupling is a declarative GitHub Actions primitive
    (not custom logic to unit-test) — verified by structural equality against `verify-published-manifest`'s
    already-reviewed, identical expression (CPE-1872 finding A) in the guard test above, rather than by
    triggering a real failing `release` run (which would require pushing a tag / dispatching a release
    workflow — off-limits per this ticket's own guardrails).
  - Confirmed the freshness check's chosen URL is genuinely live and currently exercises its 404 path
    for real: `curl` against
    `https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/catalog-index.json`
    returns HTTP 404 right now (see the consumer-question entry above) — so on its very first
    scheduled run post-merge, this check is expected to fire for real, not just in a fabricated test.
  - `npm run check`: 0 errors, 0 warnings. `npx vitest run`: 334 files / 4596 tests pass (incl. the 15
    new tests here); the ONE failure (`src/lib/msrvSync.test.ts`, 2 tests) is the pre-existing,
    known-unrelated CPE-1902 CRLF issue called out by the Foreman, untouched by this change. No Rust
    files touched by this ticket, so no clippy run needed.

  Opening the PR.

- **2026-08-26 USMST — UAT round 1 correction (attempt 2 of 3).** Foreman/UAT independently verified
  all five acceptance criteria and confirmed the consumer investigation end to end (verbatim UI
  strings, the exact `console.rs:1342` line that drops the `error` field, the live 404, all four
  freshness-script cases) — none of that was touched. One factual error in the threshold's stated
  justification was caught and had to be fixed: the workflow's comment claimed "this repo cuts
  tagged releases roughly daily (see CPE-1894's Work Log)" as grounds for a 7-day threshold. That
  statistic is real but belongs to the **sidecar** channel — CPE-1894's Work Log explicitly carries
  that qualifier and I dropped it when I wrote the comment. `catalog` lives in `release.yml`, whose
  trigger (`["v*", "!v*-sidecar"]`, CPE-1894) fires **only** on plain tags; the sidecar channel's
  pace has no bearing on how often `catalog` actually gets a chance to run. The Foreman read the
  plain-channel tags directly (`v0.57.32`/`v0.57.33` 2026-07-24, `v0.57.37` 2026-07-26, `v0.57.69`
  2026-08-23 — a 28-day gap after `v0.57.37`) and I independently reproduced the same dates via
  `git log -1 --format=%aI <tag>` before touching anything, rather than taking either the UAT or my
  own prior work on faith.
  **Fix applied**: widened `DEFAULT_THRESHOLD_DAYS` 7 → **14**, and rewrote the threshold comment in
  `catalog-freshness.yml` to derive the number from the plain channel's own observed 28-day gap,
  named explicitly so the next person can re-derive it. Caught and corrected my own first draft of
  that rewrite before committing: an early version claimed 14 days "clears the 28-day gap with room
  to spare," which is backwards — 14 < 28, so a real gap that size would still trigger red partway
  through (at day 15). The final comment says so plainly: neither 7 nor 14 is false-alarm-free
  against a 28-day gap (7 red for ~20 of 28 days, "roughly three weeks"; 14 red for ~13 of 28 days,
  roughly half as much exposure, not zero) — 14 is presented as the better available compromise
  given nothing today enforces a faster plain-channel cadence, not as a number that eliminates false
  alarms. Re-verified the freshness script's boundary behavior at the new threshold directly (bash,
  fixed epochs): published 13 days ago → fresh, exit 0; published 15 days ago → STALE, exit 1.
  `npx vitest run src/lib/catalogPublishFreshnessGuard.test.ts` — 15/15 (the test only asserts the
  threshold is a positive integer, so no test code needed to change for the new value).
  `npm run check` — 0 errors. Confirmed `main` had not moved (no rebase needed). Pushing the fix.
