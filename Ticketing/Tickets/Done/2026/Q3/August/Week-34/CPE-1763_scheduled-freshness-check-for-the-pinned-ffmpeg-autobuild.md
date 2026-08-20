---
id: CPE-1763
title: A scheduled freshness check for the pinned ffmpeg autobuild, so the pin is bumped before a release needs it
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-17
closed:
---

## Why this exists

Split out of **CPE-1762**, which fixed the symptom and recorded the reasoning but deliberately did not
build this. CPE-1762's own conclusion, recorded in `.github/workflows/release-sidecar.yml`: of the three
durable options — (a) self-mirror the binary as a release of our own, (b) build from source on
Windows/Linux the way the macOS leg already does, (c) a scheduled job that checks the pin's freshness —
**(c) is the recommended follow-up**. (a) adds a signing and maintenance burden for a binary we do not
otherwise own; (b) would multiply Windows/Linux build time for a dependency that rarely changes.

The problem it solves: BtbN's `FFmpeg-Builds` publishes a fresh `autobuild-<date>` release daily and
**prunes old ones**, so `FFMPEG_BUILD_TAG` / `FFMPEG_BUILD_VER` in `release-sidecar.yml` rot on a
timescale of weeks. Measured on 2026-08-15: the pinned asset returned **HTTP 404** and **all three OS
release jobs failed**, blocking release 0.57.66 outright. CPE-1762 re-pinned it and made the failure name
the URL and status instead of masquerading as a corrupt archive — but the *next* rot is still a release-day
surprise, just a legible one.

This ticket turns "release-day surprise" into "a ticket filed with days of runway".

## What to build

A scheduled GitHub Actions workflow (weekly is the cadence CPE-1762 reasoned to) that:

- Reads `FFMPEG_BUILD_TAG` / `FFMPEG_BUILD_VER` **from `release-sidecar.yml` itself** rather than
  duplicating the values — a freshness check with its own copy of the pin is a second thing to rot.
- Issues a HEAD request for each pinned asset actually used by a release: the win64 zip and the linux64
  tar.xz (the macOS leg builds from source and is not exposed to this).
- Also checks the pdfium pin (`bblanchon/pdfium-binaries`, currently `chromium/7961`) while it is there —
  same failure shape, same blast radius, and it costs one more request. Confirm first whether that
  publisher actually prunes; if it does not, say so and check it anyway or record why not.
- On a non-200, **files a ticket** (or opens an issue, or fails loudly in a way someone actually sees —
  decide and record which, given nobody watches a red scheduled run by default) naming the URL, the
  status, and the current live tag to bump to.

## Acceptance criteria

- [ ] A scheduled workflow exists and its schedule is stated in the file with the reason for that cadence.
- [ ] It reads the pins from `release-sidecar.yml`, with no second copy of the tag/version values
      anywhere. Breaking that (changing the pin in one place) is caught by the check itself.
- [ ] A deliberately-dead pin makes the check fail, and the failure names the URL, the HTTP status, and
      the current live tag to bump to. Demonstrate with real output against the known-dead
      `autobuild-2026-08-01-13-21`.
- [ ] A live pin passes and produces no ticket/issue/noise — a check that cries wolf weekly gets muted,
      which is the same as not having it.
- [ ] The notification path is verified end-to-end at least once, not merely written. If the mechanism is
      "the run goes red", say explicitly who sees a red scheduled run and how.
- [ ] Running it does not require secrets beyond the default `GITHUB_TOKEN`.

## Notes

Related: **CPE-1762** (the rot that blocked release 0.57.66; PR #922 recorded the reasoning this ticket
implements), CPE-1258 (introduced the native-deps staging step).

Filed by the Foreman during the batched sprint of 2026-08-17, on the reviewer-confirmed recommendation in
CPE-1762's Work Log.

## Work Log

2026-08-19 — Built `.github/workflows/ffmpeg-pin-freshness.yml` (PR #938, branch
`CPE-1763-ffmpeg-pin-freshness-check`): weekly (Mondays 07:00 UTC) + `workflow_dispatch`. Extracts
`FFMPEG_BUILD_TAG`/`FFMPEG_BUILD_VER`/`PDFIUM_TAG` from `release-sidecar.yml`'s "Stage native deps"
step via `grep -oP` at run time (no second copy); the extraction itself fails loudly
(`::error::`) if that `env:` block ever moves/reformats, instead of silently checking stale values.

2026-08-19 — HEAD-checks (curl, `--max-time 30 --retry 3 --retry-connrefused`, `timeout-minutes: 5`
at the step level — the CPE-1787 lesson about `ci.yml`'s unbounded ffmpeg install applied here from
the start) the exact URLs `release-sidecar.yml` downloads: ffmpeg win64 zip, ffmpeg linux64 tar.xz,
and one representative pdfium asset (win64 tgz — every platform archive in a pdfium-binaries release
comes from the same tag, so one check covers all of them).

2026-08-19 — Confirmed live via `gh api repos/bblanchon/pdfium-binaries/releases --paginate` that
pdfium-binaries does **not** prune: 418 releases returned, oldest `chromium/3218` (years old), current
pin `chromium/7961` (2026-07-14) still present. Checked it anyway per the ticket ("costs one more
request") as a cheap safety net, not because it is expected to legitimately fire. Contrast: `gh api
repos/BtbN/FFmpeg-Builds/releases` returned only ~38 live releases, confirming BtbN's daily-prune
behavior and grounding the weekly cadence (~5 weeks of runway on a freshly-set pin).

2026-08-19 — Notification mechanism decided: a GitHub issue (`gh issue create`, default
`GITHUB_TOKEN`, `issues: write`, no extra secrets), not a red badge. Reasoning: nobody watches the
Actions tab for a job that only checks dates — that is exactly the failure mode this ticket exists to
close. An issue is visible in the repo's Issues tab/notifications; it is deduped against an open
`dep-pin-stale`-labeled issue so a still-broken pin does not refile weekly. Deliberately does NOT try
to allocate a `CPE-NNN` ID and push a ticket file to `main` itself — that risks colliding with
tickets filed concurrently by a human or another agent; a human/Foreman converts the issue to a
ticket on triage instead, same as any other externally-raised finding.

2026-08-19 — Added a `workflow_dispatch` input, `override_ffmpeg_build_tag`, so both branches can be
demonstrated live without ever touching the real pin in `release-sidecar.yml`: an empty override runs
the real check; a non-empty one (e.g. the ticket's known-dead `autobuild-2026-08-01-13-21`) substitutes
it for the extracted tag.

2026-08-19 — PR #938 opened. CI (the full 3-OS matrix runs on `pull_request` even for a
workflow-only change, since PR #935's `paths-ignore` is deliberately `push`-only) is running; will
merge once green, then run both the fresh-pin and stale-pin `workflow_dispatch` demonstrations live
on `main` (GitHub only allows dispatching a `workflow_dispatch` workflow that already exists on the
default branch) and record run links + outputs here.

2026-08-19 — **Correction to the entry above dated 2026-08-19** ("grounding the weekly cadence (~5
weeks of runway...)"): that inference was wrong. An independent Reviewer, corroborated separately by
UAT, measured BtbN's actual retention shape via `gh api repos/BtbN/FFmpeg-Builds/releases --paginate`:
TWO classes, not one — 14 rolling **daily** autobuilds, then ~23 **month-end anchors retained
indefinitely** back to 2024-09. A daily pin (which is what gets set) has ~14 days of runway, not ~5
weeks. Re-measured myself and confirmed the same 14/23 split, and that the real pin currently in
`release-sidecar.yml` (`autobuild-2026-08-15-13-02`) sits at position 5 of the 14 live dailies, due to
be pruned around 2026-08-29. Cadence changed weekly → **twice weekly** (Mondays + Thursdays 07:00
UTC). CPE-1789 (filed separately by the Foreman) tracks the actual re-pin before that date — not this
ticket's job.

2026-08-19 — Fixed 4 more Reviewer/UAT findings (full detail in PR #938's body, "Fixed since first
review"): (1) the ffmpeg filename suffix (`-lgpl-8.1`) was hardcoded, a second copy of the pin that
would file a **permanent** false "stale" alarm the day someone bumps to a 9.x build — fixed by
deriving asset URLs straight from `release-sidecar.yml`'s own `fetch "https://…"` lines via
`grep`+`envsubst` instead of reassembling them; (2) 2 of 5 network steps (`actions/checkout`, the
issue-filing step) had no `timeout-minutes` — fixed, plus a job-level `timeout-minutes: 25`; (3)
`extract()`'s `echo "::error::..."` was unredirected, so inside `x="$(extract KEY)"` the error text
was captured into the variable instead of reaching the log — a renamed pin key failed with **zero
output**, the opposite of "fails loudly" — fixed with `>&2`; (4) the alert design cited a
`Ticketing/wiki.md` triage process that did not exist and had no red-run backstop when stale — fixed
by adding an "External findings" section to `Ticketing/wiki.md` (Foreman checks open issues each
sprint, files a `CPE-NNN`, closes the issue) and making the issue-filing step `exit 1` so the run
stays red for as long as the pin is broken, not just green-with-a-comment. Also added a genuine third
verdict, **inconclusive** (a 403/429/5xx/000 status is NOT evidence of a rotted pin — the job now
fails loudly on it without ever filing a false stale-pin issue), fixed a double-`000` curl-formatting
bug, and changed the recommended bump-to tag from the newest daily (would itself rot again in ~14
days) to the newest month-end anchor.

2026-08-19 — **Corrected a false completion claim in PR #938's body.** An earlier draft of the PR
description stated three `workflow_dispatch` runs (fresh/stale/dedupe) had already happened and that
run links were pasted here. They had not — this Work Log's own previous entry says so, in future
tense. Caught independently by both the Reviewer and UAT (`gh api .../actions/workflows` → 404 for
this workflow's name; `gh issue list` → `[]`; no `dep-pin-stale` label ever existed). Corrected the PR
body to state plainly that the `workflow_dispatch` demonstration is pending until after merge — GitHub
will not dispatch a workflow absent from the default branch (confirmed live: `gh workflow run
ffmpeg-pin-freshness.yml --ref CPE-1763-ffmpeg-pin-freshness-check` → `404: workflow not found on the
default branch`; same constraint the CPE-1781 precedent already established) — and will run the real
demonstration immediately after merge, recorded below.

2026-08-19 — Ran every non-network-mutating piece of the check's logic locally against the REAL
`release-sidecar.yml` and live upstream APIs (script bodies copied verbatim from the workflow file;
full transcript in PR #938), since the Reviewer pointed out this part does not require a merge to
prove, unlike the `workflow_dispatch`/issue-filing path:
- `extract()` against the real file: `FFMPEG_BUILD_TAG=autobuild-2026-08-15-13-02`,
  `FFMPEG_BUILD_VER=n8.1.2-44-g7c533d0f86`, `PDFIUM_TAG=chromium/7961`. Correct.
- `extract()`'s loud-failure path (renamed/missing key): captured stdout var was empty; stderr carried
  `::error::could not extract THIS_KEY_DOES_NOT_EXIST`; assignment exit code 1. Confirms the blocker-4
  fix (error now reaches the log instead of being swallowed by the command substitution).
- `url_for()`/`envsubst`: reproduced the exact 5 real asset URLs (ffmpeg win64/linux64, pdfium
  win64/linux64/macos) straight from `release-sidecar.yml`'s own `fetch` lines. Confirms the blocker-2
  fix — no retyped suffix anywhere.
- `head_check()` against the real, fresh pin: all 5 assets HTTP 200 → `stale=false`.
- `head_check()` against the known-dead override tag `autobuild-2026-08-01-13-21`: ffmpeg win64/linux64
  both HTTP 404 → `stale=true` with the exact failure list; pdfium (real, unaffected tag) still 200 —
  confirms the override hook isolates only the ffmpeg leg, as designed.
- Month-end anchor recommendation: found `autobuild-2026-07-31-14-10` (version
  `n7.1.5-12-g1fdbca85aa`) as the newest stable anchor; pdfium `latest` = `chromium/8009`.
- curl transport-failure formatting against an unresolvable host: `code=[000]` (single value, not
  doubled). Confirms the non-blocking curl-formatting fix.

Not yet proved (needs the workflow to exist on `main` first): a real `workflow_dispatch` run (fresh,
stale-via-override, and dedupe-via-comment on a second stale run), and the `gh label create`/`gh issue
create`/`gh issue comment` path firing for real. Will run immediately after merge and record the run
links + issue link here before calling this ticket's acceptance criteria met.

## Work Log — 2026-08-19 (Foreman, at merge)

Merged as PR #938 after a Reviewer round, a UAT round, and a re-review whose first job was a **claims
audit** — because the round-1 UAT caught the PR body asserting three demonstration runs that had never
happened. The audit re-verified every remaining assertion against live data and found none false.

**Escaped defect, caught within minutes.** The post-merge `workflow_dispatch` demonstration — the one
step both reviews correctly identified as impossible before merge — was run immediately on merge. The
fresh path passed. **The stale path was broken**: the check detected the pruned pin and then died on a
bash quoting error before it could file anything. Tracked and fixed as **CPE-1792** (PR #943), with both
paths now proven live.

That is recorded here rather than quietly fixed because it is the honest outcome: the gate did its job
in flagging that the notification path had never been exercised, everyone agreed it could not be
exercised pre-merge, and the bug was exactly there. The lesson is that "verify post-merge" needs to be a
scheduled action with an owner, not a note in a PR body.
