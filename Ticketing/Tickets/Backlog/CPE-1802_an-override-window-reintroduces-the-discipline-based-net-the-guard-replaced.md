---
id: CPE-1802
title: an ffmpeg pin override window reintroduces the discipline-based net the guard just replaced
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1796 added a mechanical guard so the ffmpeg pin must be a **month-end anchor** — upstream retains
those indefinitely, whereas rolling dailies are pruned after about 14 days and eventually 404 the
release build. It also lowered the freshness check's cadence from twice-weekly to weekly, reasoning
that the guard now blocks accidental dailies so the check no longer needs to catch them early.

That reasoning holds **except during a deliberate override window.** When someone sets
`FFMPEG_BUILD_TAG_OVERRIDE_REASON` and pins a daily on purpose, the pin genuinely is a daily again with
the original ~14-day runway — and the mitigation on offer is "remember to dispatch the freshness check
manually".

So for exactly the case this ticket family exists to make mechanical, the safety net becomes a
discipline someone has to remember, at the one moment they are already doing something unusual and
under time pressure.

## What to do

- Make the override **arm its own safety net**. The cleanest shape suggested during review: trigger
  `ffmpeg-pin-freshness.yml` automatically when `FFMPEG_BUILD_TAG_OVERRIDE_REASON` transitions to
  non-empty — a `repository_dispatch`, or a step in the guard job itself that fires the check when it
  takes the override path.
- Consider also raising the cadence *only while an override is active*, rather than globally. A
  conditional schedule is awkward in GitHub Actions, so weigh that against simply running the check on
  every push while the override is set — during an override window that is a small number of runs and
  the exposure is real.
- Whatever the shape, the point is that **the person who takes the override should not also have to
  remember the consequence.** If it stays manual, say so explicitly next to the override variable so
  the obligation is visible where the decision is made, rather than in a workflow comment they will
  never open.

## Notes

Filed by the Foreman from PR #956's review, 2026-08-20, which flagged it as a judgment call rather than
a bug and explicitly out of scope for that PR. The guard itself was verified not to fire on any
legitimate pin — leap years, month lengths, malformed and missing values all handled — so this is about
the one path that deliberately steps around it.

Related: **CPE-1796** (the guard and the cadence change), **CPE-1789** (the repin that prompted it),
**CPE-1763** (the freshness check), **CPE-1795** (its recommendation bug).

## Work Log

**2026-08-20** — Implemented the shape the ticket recommended: the override arms its own net,
automatically, rather than staying a discipline someone has to remember.

- `.github/workflows/ci.yml` (`ffmpeg-pin-guard` job): the "Assert FFMPEG_BUILD_TAG is a month-end
  anchor" step now has `id: guard` and writes `override_active=true` to `$GITHUB_OUTPUT` on — and
  only on — the branch that accepts a valid `FFMPEG_BUILD_TAG_OVERRIDE_REASON` (never on the
  anchor-is-fine branch). A new step, "Fire the freshness check now that the override is live",
  gated on `steps.guard.outputs.override_active == 'true' && github.event_name == 'push'`, runs
  `gh workflow run ffmpeg-pin-freshness.yml --ref "${{ github.ref_name }}"`. Push-only (not PR):
  the override isn't live until it merges to `main`, and a fork PR's `GITHUB_TOKEN` couldn't
  dispatch anyway. Went with "fire on every push while the override is set" over a conditional
  schedule, per the ticket's own steer — GitHub Actions can't express the latter cleanly, and the
  former is a small number of extra runs during a short window. Added `permissions: contents: read,
  actions: write` at the job level (declaring `permissions:` at all zeroes every unlisted scope, so
  `contents: read` had to be restated for `actions/checkout`).
- `.github/workflows/ffmpeg-pin-freshness.yml`: updated the cadence comment that used to say "run
  this workflow manually (workflow_dispatch) during that window" — that instruction is now false,
  so left uncorrected it would have become exactly the stale-but-confident-comment class this repo
  has been bitten by before (CPE-1796's own second item). It now describes the automatic dispatch.
- `.github/workflows/release-sidecar.yml`: extended the comment next to
  `FFMPEG_BUILD_TAG_OVERRIDE_REASON` to say the override also arms the freshness check
  automatically, so the obligation is visible at the point of decision (per the ticket's "if it
  stays manual, say so explicitly" — it doesn't stay manual, but the same principle applies to
  documenting what actually happens there).
- Added `src/lib/ffmpegOverrideAutoDispatch.test.ts` (5 tests, all new) — no automated guard test
  covered this workflow before. Asserts: (1) the guard job has `actions: write`; (2)
  `override_active=true` is written on the override-accepted branch and NOT on the anchor-OK
  branch; (3) the dispatch step is gated on both `override_active` and `github.event_name ==
  'push'` and calls `gh workflow run ffmpeg-pin-freshness.yml`; (4) release-sidecar.yml's override
  comment documents the automatic dispatch; (5) ffmpeg-pin-freshness.yml's cadence comment no
  longer tells a human to dispatch it by hand. Every test was proven red by temporarily reverting
  the specific piece of code/comment it covers, observing the failure, then reverting back — see PR
  description for the per-test red-proof transcript.

**Gates run:**
- `python3 -c "import yaml; yaml.safe_load(open(...))"` on all three touched workflow files —
  parsed clean, before and after every temporary red-proof edit.
- `npx vitest run src/lib/ffmpegOverrideAutoDispatch.test.ts` — 5/5 passed.
- `npx vitest run` (full frontend suite) — 317 files / 4187 tests passed.
- `npm run check` — 0 errors, 0 warnings.
- No Rust files touched, so `cargo clippy` was not run for this ticket.

Not independently verified: an actual live `gh workflow run` dispatch (would require pushing a real
override to `main` and burning a real Actions run against upstream BtbN/pdfium — out of scope for a
worktree PR). The `gh workflow run ffmpeg-pin-freshness.yml --ref ...` command and permission grant
were checked by reading `gh`'s documented behavior and this repo's existing use of `actions: write`
+ `github.token` elsewhere, not by an end-to-end run.

**2026-08-20 (revision 2, PR #963 Reviewer round)** — The independent Reviewer returned CHANGES
REQUESTED: the mechanism above was verified correct across six scenarios, but 3 of the original 5
test assertions did not fail for the regression they claimed to catch (proven by making the
realistic one-line change and watching all 5 stay green). Fixed all three, plus two non-blocking
comment-accuracy nits:

- **Blocker 1** — `expect(guardJob).toMatch(/actions:\s*write/)` matched the explanatory comment
  next to the key (`\s*` matches zero chars), not the key itself; deleting only the real
  `actions: write` line stayed green. Rewrote the assertion to go through `parseYaml`
  (`src/lib/preview/yaml.ts`, the in-repo bounded-subset YAML parser already used for the app's own
  YAML preview pane — reused rather than adding a `yaml`/`js-yaml` dependency, which this repo does
  not have) and read `guardJob.permissions.actions`/`.contents` from the parsed structure.
- **Blocker 2** — nothing asserted the guard step carries `id: guard`; deleting that line makes
  `steps.guard.outputs.override_active` resolve to empty, so the dispatch step's `if:` is
  permanently false and the net silently never arms again. Added an assertion (also via `parseYaml`)
  that the named assertion step's `id` is `"guard"`.
- **Blocker 3** — nothing asserted `ffmpeg-pin-freshness.yml` still declares `workflow_dispatch:`.
  `gh workflow run` hard-fails without it, and the dispatch step runs under `set -euo pipefail`, so
  that would turn the guard job red on every override-active push. Added an assertion that
  `ffmpeg-pin-freshness.yml`'s parsed `on:` has a `workflow_dispatch` key — also the cheapest
  available proxy for the live dispatch itself, which remains unverified end-to-end (see above).
- **Nit 4** — `ci.yml`'s dispatch step interpolated `${{ github.ref_name }}` directly into the
  `run:` script (the canonical Actions script-injection shape). Not exploitable today (the push
  trigger is `branches: [main]` and the step is push-gated), but nothing pinned that trigger, so
  widening `push.branches` later could reopen it. Moved it through `env: REF_NAME:` +
  `--ref "$REF_NAME"` instead.
- **Nit 5** — three comments said the dispatch fires on "every push to `main`", which overstates it:
  `ci.yml`'s push trigger carries a `paths-ignore` (CPE-1781) for `Ticketing/Tickets/**` /
  `Ticketing/Sprints/**`, so a ticket-bookkeeping-only push never reaches this job at all. Reworded
  all three (`ci.yml`, `ffmpeg-pin-freshness.yml`, `release-sidecar.yml`) to say "every push to
  `main` that reaches that job" and note the exclusion is harmless (a bookkeeping-only commit can't
  be the one that sets/clears `FFMPEG_BUILD_TAG_OVERRIDE_REASON` anyway).

Per the Reviewer's read, tests 2 ("marks override_active…") and 3 ("dispatches…gated…") were already
load-bearing and needed no fix; kept as-is. Tests 4/5 (comment-prose pinning) were confirmed non-
mechanism and kept as documentation-AC coverage only, now updated to match the reworded comments.

**Red-proofs for every fixed/new assertion** — the exact minimal one-line deletion the Reviewer
specified, observed red, then reverted:
- Blocker 1: deleted `.github/workflows/ci.yml:1321` (`      actions: write`), leaving the comment —
  `grants the guard job actions:write...` failed: `expected undefined to be 'write'`. Reverted.
- Blocker 2: deleted `.github/workflows/ci.yml:1326` (`        id: guard`) — `pins id: guard on the
  assertion step...` failed: `expected undefined to be 'guard'`. Reverted.
- Blocker 3: deleted the `workflow_dispatch:` block from `.github/workflows/ffmpeg-pin-freshness.yml`
  (lines 88–96, the `on:` mapping's `workflow_dispatch` key and its `inputs:`) — `the target workflow
  still declares workflow_dispatch...` failed: `expected false to be true`. Reverted.

**Gates re-run after the fix:**
- `python3 -c "import yaml; yaml.safe_load(open(...))"` on all three touched workflow files — clean,
  before and after every red-proof edit.
- `npx vitest run src/lib/ffmpegOverrideAutoDispatch.test.ts` — 7/7 passed (2 new tests added: the
  `id: guard` check and the `workflow_dispatch` presence check).
- `npx vitest run` (full frontend suite) — 317 files / 4189 tests passed.
- `npm run check` — 0 errors, 0 warnings.
- `git diff --numstat package.json package-lock.json` — empty; no dependency was added (reused the
  in-repo `parseYaml`).
- No Rust files touched, so `cargo clippy` was not run.
