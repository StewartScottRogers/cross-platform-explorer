---
id: CPE-1594
title: "gui-smoke: make it produce signal — upload screenshot artifacts, ratchet the Linux leg to blocking, stop the Windows leg poisoning every run"
type: Task
status: Backlog
priority: High
component: CI/QA-infra
epic: CPE-810
tags: [ready]
created: 2026-08-10
closed:
---

## Why

The crew has a standing instruction to ignore `gui-smoke` because it's "flaky". It isn't flaky. It is
**emitting nothing at all**, and that single fact is what keeps Manual-Test-Burndown rows #1 (GUI
end-to-end), #2 (build→deploy→run smoke), #3 (visual/theme regression) and #4 (cross-OS GUI) stuck on
🔧 while MVD climbs every shift.

Measured 2026-08-10 against the Actions API (`repos/:owner/:repo/actions/workflows/gui-smoke.yml/runs`,
800 runs, 2026-08-03 → 2026-08-10):

- **796 `cancelled` · 4 `failure` · 0 `success`.** The most recent **300 runs (2.5 days) contain not one
  terminal verdict.**
- **Windows leg: 0 of 40 specs have ever run an assertion in CI.** Raw log of run `31409461248` job
  `93523746819`: every WebDriver session dies with `session not created: DevToolsActivePort file doesn't
  exist` (the CPE-1048 WebView2 startup crash, still unfixed despite the `--disable-gpu --no-sandbox
  --disable-dev-shm-usage` env already in the workflow). Each spec burns ~3 min (1 attempt + 3×60s
  retries); `timeout-minutes: 45` kills the job after ~7 specs, having logged 39 `DevToolsActivePort`
  errors. **The timeout-kill is what stamps the whole RUN `cancelled`** — the dead Windows leg is the
  reason the working Linux leg reads as a flake. It also burns a full 45-min `windows-latest` runner
  (release `tauri build` + two `cargo install`s) on **every push and every PR** for zero information.
- **Linux leg: works, and is being discarded.** Same run, job `93523746768`:
  `Spec Files: 33 passed, 7 failed, 40 total (100% completed) in 00:27:48`, job wall-clock ~39 min.
  82.5% green with a stable, readable failure set.
- **No screenshots ever leave CI.** `gui-smoke.yml` contains **no `actions/upload-artifact` step**. The
  75 `snap()` calls write to `gui-smoke/.screenshots/` — gitignored, on an ephemeral runner, discarded.
  **The CPE-1148 Visual Critic loop has no CI substrate at all**; it has only ever worked off a local
  `tauri build` run by hand. `gui-smoke/baselines/` still holds only 2 synthetic demo PNGs.

Consequence: the 5 manual surfaces logged into the burndown this shift (font specimen + glyph grid in
both themes, user-command Toolbar crowding, preview action bars, JSON tree, Trash view) have no path to
automatic verification, and `network.smoke.ts` has been failing on *"expected the permanent Network
section header to render"* — a possible real regression in CPE-1516's shipped surface — with nobody
allowed to look. The Linux tail has grown **3 → 7** specs since CPE-1507 catalogued it.

This ticket does **not** fix the 7 failing specs and does **not** fix CPE-1048. It makes the harness
*emit a verdict and a screenshot gallery*, and ratchets it so the tail can never grow again.

## What to build

### 1. Export the screenshots (unlocks the Visual Critic in CI)

`.github/workflows/gui-smoke.yml`, Linux leg — add after the suite step:

```yaml
      - name: Upload gui-smoke screenshots
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: gui-smoke-screenshots-ubuntu
          path: gui-smoke/.screenshots/**
          retention-days: 14
          if-no-files-found: error
```

`if: always()` matters — a failing run's `<name>-fail.png` shots (CPE-1149) are the ones the Visual
Critic most wants. Add the same step to the Windows leg (guarded the same way) so a manual dispatch of
that leg is useful the day CPE-1048 is solved. Document the retrieval command in `gui-smoke/README.md`:
`gh run download <run-id> -n gui-smoke-screenshots-ubuntu -D <dir>` — that one line is what lets the
sprint's Visual Critic judge a PR without a Foreman building locally.

### 2. Ratchet the Linux leg so it can go blocking at 33/40

Do **not** wait for all 40 specs to pass. Pin the current state and forbid regression.

- **`gui-smoke/known-failing.json`** (new, committed). The 7 specs failing on `main` today, each with a
  reason and an owning ticket:

  ```json
  {
    "$comment": "CPE-1594 ratchet. A spec listed here is ALLOWED to fail; anything else failing reds the job. Removing an entry is one-way: once a spec passes, the ratchet FAILS until it is deleted from this list.",
    "specs": {
      "samples.smoke.ts":         { "reason": "preview pane never settles for ~15 sample kinds (crypto/*, .eml, .ics, .vcf, .json) on WebKitGTK after 20s", "ticket": "CPE-1507" },
      "saved-search.smoke.ts":    { "reason": "Saved Searches sidebar header never renders on Linux; .fav-title getText() returns empty", "ticket": "CPE-1507" },
      "network.smoke.ts":         { "reason": "permanent Network section header never renders — POSSIBLE REAL REGRESSION in CPE-1516, triage first", "ticket": "CPE-1595" },
      "archive-browse.smoke.ts":  { "reason": "element click intercepted", "ticket": "CPE-1595" },
      "archive-password.smoke.ts":{ "reason": "element not interactable", "ticket": "CPE-1595" },
      "shred-dialog.smoke.ts":    { "reason": "'.ctx button.row' still not clickable after 10s", "ticket": "CPE-1595" },
      "transfer-panel.smoke.ts":  { "reason": "seeded CPE-1226 transfer row never appears", "ticket": "CPE-1595" }
    }
  }
  ```

  (File the CPE-1595 triage ticket as part of this work, or point those four at CPE-1507 if the Foreman
  prefers one tail ticket — the exact ID matters less than every entry having an owner.)

- **JSON results from wdio.** Add `@wdio/json-reporter` to `gui-smoke/package.json` devDependencies and
  register it in `wdio.conf.ts` alongside the existing `spec` reporter, writing to `gui-smoke/.results/`.
  Do not parse the human `spec` reporter text — it is not a contract.

- **`gui-smoke/lib/ratchet.ts`** (new, pure) + **`gui-smoke/lib/ratchet.test.ts`** (new — must live in
  `lib/` because `npm run test:unit` is `tsx --test lib/**/*.test.ts`). Mirrors the existing
  `compare.ts` + `compare.test.ts` split. Pure function `evaluate({ results, knownFailing, expectedSpecCount })`
  returning `{ ok, newFailures[], fixedButStillListed[], incomplete }`. It fails when:
  1. a spec failed that is **not** in `known-failing.json` → `NEW GUI REGRESSION`;
  2. a spec **in** the list **passed** → `RATCHET: <spec> now passes — delete it from known-failing.json`
     (the one-way ratchet: a surface may leave the failing column exactly once, per the QA charter);
  3. the run is **incomplete** — total specs seen < `expectedSpecCount` (derived by globbing
     `specs/*.smoke.ts`, not hard-coded) → `SUITE DID NOT COMPLETE`, so a timeout or crash can never
     masquerade as green. This clause is the specific guard against the failure mode this whole ticket
     exists to fix.

- **`gui-smoke/scripts/run-ratchet.ts`** (new, I/O wrapper, mirrors `scripts/bless-demo-baselines.ts`) +
  `"ratchet": "tsx scripts/run-ratchet.ts"` in `package.json`.

- Workflow Linux leg: **remove `continue-on-error: true`**, and split the run step so the ratchet is the
  gate:

  ```yaml
      - name: Run GUI smoke suite (xvfb-run)
        working-directory: gui-smoke
        run: xvfb-run --auto-servernum npm test || true

      - name: Ratchet — no new GUI regressions
        working-directory: gui-smoke
        run: npm run ratchet
  ```

  The suite's own exit code is deliberately swallowed; the ratchet owns the verdict.

### 3. Stop the Windows leg poisoning every run

It has never run one assertion and it costs 45 min of `windows-latest` per push *and* per PR. Take it off
the hot path without deleting the diagnostic:

- Move the `gui-smoke` (windows) job behind `if: github.event_name != 'push' && github.event_name != 'pull_request'`,
  and add `workflow_dispatch:` plus a nightly `schedule:` (`cron: '0 7 * * *'`) to the workflow triggers.
- Drop its `timeout-minutes` 45 → **15**: if `DevToolsActivePort` is still broken, it should die in
  minutes, not burn an hour.
- Keep `continue-on-error: true` on it (it is a diagnostic, not a gate) and keep the CPE-1048 comment
  block + the research Library pointer intact — this is a deferral, not a decision that Windows GUI
  coverage doesn't matter. Add one line saying the real fix is a self-hosted / interactive-session
  Windows runner and that the nightly run is the canary for when the hosted image starts working.

### 4. Paperwork

- `gui-smoke/README.md`: new section "Reading a CI run" — the artifact download command, how the ratchet
  works, and the exact procedure to retire a `known-failing.json` entry.
- `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`: flip row #4 (cross-OS GUI) to ✅ **for the Linux
  leg** naming `GUI smoke (ubuntu-latest)` + the ratchet step as the pinning job (macOS residual stays);
  update rows #1/#2 to name the same pinning job and note the Windows residual; update row #3 to record
  that screenshots now reach CI.

## Acceptance

1. A PR that deliberately breaks a currently-**passing** spec (e.g. rename a `data-testid` used by
   `open-dir.smoke.ts`) makes the Linux leg go **RED**. Demonstrate this in the PR description with a
   throwaway commit or a local ratchet run against a doctored results file.
2. On unmodified `main`, the Linux leg is **GREEN** (33 pass, 7 known-failing) — a real, readable verdict
   instead of `cancelled`.
3. A truncated/timed-out run is **RED**, not green (clause 3 above) — covered by a `ratchet.test.ts` case.
4. Every run, pass or fail, uploads `gui-smoke-screenshots-ubuntu` containing **≥60 PNGs**, downloadable
   with `gh run download`. Paste the artifact listing in the PR.
5. Deleting a spec from `known-failing.json` while it still fails → RED; leaving one in after it starts
   passing → RED. Both covered by `ratchet.test.ts`.
6. `gui-smoke`: `npm run typecheck` and `npm run test:unit` green, including the new ratchet tests.
7. No `windows-latest` `gui-smoke` job runs on push or PR; `workflow_dispatch` still starts one.
8. The burndown row edits in §4 are in the same PR.

## Follow-ups (explicitly NOT this ticket)

- **Triage the 7 known-failing Linux specs** — `network.smoke.ts` FIRST, it may be a live regression in
  the shipped Network sidebar (CPE-1516). CPE-1507 already owns `samples` + `saved-search`.
- **Dark-theme visual pass** — `gui-smoke` has **zero** dark-theme coverage; no spec anywhere flips
  `data-theme`, so every visual surface is verified light-only even though CPE-1492/1493 shipped a real
  dark theme. Needs a `theme.ts` helper + a light/dark `snap()` pair on the key surfaces.
- **Specs for the 5 surfaces logged as manual debt on 2026-08-10**: `font-preview.smoke.ts`,
  a toolbar-crowding spec, `preview-actions.smoke.ts` (covers the JSON tree too), `trash.smoke.ts`.
- **Bless real baselines** (burndown row #3) — now finally possible, because §1 gets the PNGs off the
  runner.
- **Shard the suite across matrix jobs** (the workflow's own comment already recommends this; CPE-1266)
  so the Linux leg's 28 min drops and adding specs stays cheap.
- **CPE-1048 proper** — self-hosted / interactive-session Windows runner for a real WebView2 session.

## Notes

Cross-reference correction: the crew's checkpoints and history cite **CPE-1181** as the "gui-smoke is
flaky, ignore it" ticket. CPE-1181 is *"navigate into non-zip archives"* — an unrelated, closed feature
ticket. The actual non-blocking decision is **CPE-1048** (Option E). Worth fixing the citation wherever
it appears so the next crew reads the right rationale.

QA-Architect owned. Epic CPE-810.

## Work Log

2026-08-10 — Implemented all three changes (screenshot upload artifact on both legs, `known-failing.json` +
`gui-smoke/lib/ratchet.ts` + `scripts/run-ratchet.ts` gating the Linux leg, Windows leg moved off push/PR onto
`workflow_dispatch`/nightly `schedule`). Filed CPE-1595 for the known-failing triage follow-up.
2026-08-10 — Foreman-relayed triage on `network.smoke.ts` (one of the original 7): confirmed a stale test
selector, not a CPE-1516 product regression — `$("=Network")` maps to WebDriver's "link text" strategy (`<a>`
only), but `Sidebar.svelte:862` renders the header as a plain `<span class="label fav-title">`. Fixed the
selector in this PR (matches `saved-search.smoke.ts`'s `$$(".fav-title")` + text-filter convention).
2026-08-10 — Reviewer flagged the `network.smoke.ts` fix as unverified against a live Linux run before it could
be trusted to leave `known-failing.json`; pushed a hedge commit keeping it listed (7 entries) until confirmed.
Correct call — see next entry.
2026-08-10 — PR #801's own first live `gui-smoke-linux` CI run (31446269217, job 93641134303) came back RED, but
for two reasons, not one:
  1. **Confirms the hedge was right**: `Spec Files: 33 passed, 7 failed, 40 total (100% completed) in 00:28:04`
     — exactly the 7-entry baseline. `network.smoke.ts`'s corrected selector still times out live
     (`expected the permanent Network section header to render`) — the same class of `.fav-title getText()`
     issue `saved-search.smoke.ts` is already known-failing for. Filed the working theory (shared root cause)
     in CPE-1595 and left the entry listed with an updated reason.
  2. **Two real workflow bugs, found via the raw job log** (`gh api .../jobs/93641134303/logs`): (a) the
     screenshot upload matched **zero files** — `##[error]No files were found with the provided path:
     gui-smoke/.screenshots/**` — root-caused to `actions/upload-artifact@v4`'s documented default of excluding
     any path under a dot-prefixed folder ("hidden files"), confirmed via the action's own docs; needs
     `include-hidden-files: true`. (b) that upload step's `if-no-files-found: error` **aborted the job before
     the Ratchet step ever ran** — no ratchet output anywhere in the log — because GitHub Actions steps default
     to running only if every prior step succeeded. Fixed both: added `include-hidden-files: true` to both
     legs' upload steps, downgraded Linux's `if-no-files-found` to `warn`, and reordered the Linux leg so
     `Ratchet` runs immediately after the suite (which always "succeeds" via `|| true`) and the screenshot
     upload runs last — so the gate can never again be silently skipped by an unrelated step's failure.
