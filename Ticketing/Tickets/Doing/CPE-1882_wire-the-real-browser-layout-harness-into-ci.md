---
id: CPE-1882
title: wire the real-browser layout harness into CI, so a clipping regression goes red instead of needing a human
type: task
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Why this is the highest-leverage QA ticket open

For this entire batched run the Visual Critic has been **blind**: nothing local could answer a
*layout* question, so every visual ticket shipped on jsdom assertions plus a human's judgement.

The cause was misdiagnosed twice. First as "the GUI test drivers are not installed" — they are, in
`~/.cargo/bin`. Then as "`msedgedriver` 150 against Edge 151 hangs sessions" — true, and still worth
fixing, but **not the blocker**.

The actual answer was found by the worker on CPE-1833/CPE-1836 while doing something else:
**plain installed `chrome.exe --headless=new`**, driving a local page. No WebDriver. No install. No
machine-global change. Living proof in `scripts/dev-harness/statusbar-notice/`, which now reports:

- element **rects** at a chosen viewport width
- an **`overlapPairs`** list — which elements actually collide
- a **paint probe** (`elementFromPoint`) answering "does element A paint on top of element B"

That is the missing capability, and it is not screenshots. jsdom can assert that a CSS property
appears in the source; it can **never** tell you whether the resulting pixels overlap. That is exactly
why "the git block bleeds into the disk label" (CPE-1836) was a real, visible bug that no test could
catch.

## The gap this ticket closes

**None of that thoroughness reaches CI.** PR #1019's reviewer grepped for it: the harness is invoked
by nothing in `gui-smoke` or `vitest`. The only CI guard for CPE-1836 is three regex assertions
against the `<style>` source text. That reviewer enumerated precisely what those miss:

- a second `.git{}` rule added later in the file (or with `!important`) overriding the first — the
  helper uses `.match()`, not `.matchAll()`, so it only inspects the first occurrence
- any layout regression not touching those four specific property/selector pairs, e.g. a new pinned
  child added to `.git` without `flex: 0 0 auto`

So the current guard is a **narrow tripwire for one fix**, not a layout guarantee — and the run's own
experience says the next clipping bug will be in a different component anyway.

## What to do

1. **Generalise the harness.** Something a ticket can point at a component plus a list of widths and
   get back rects, overlap pairs and paint probes. `scripts/dev-harness/statusbar-notice/` is the
   working prototype — read it first; do not start over.
2. **Wire it into CI** as its own job. It needs no WebDriver, so it should be far more reliable than
   the existing `gui-smoke` legs, which is most of the point.
3. **Red-proof it** against the two bugs already on record: reintroduce CPE-1836's missing
   `overflow: hidden` and confirm the job goes red naming the overlapping pair; do the same for
   CPE-1827's Trash titlebar at the 600px floor.
4. **Cover the standing rule, not just the two bugs.** The repo's pill/tick-tack convention says a row
   of pills must wrap and grow while each pill keeps its text on one line. That rule has no automated
   enforcement anywhere. A generic "no element in this row overlaps another, and no text overflows its
   own background" check would enforce it everywhere at once.

## Fixes a mis-referenced acceptance criterion

CPE-1836's own AC says its fix must be *"pinned by the browser-level coverage from CPE-1822"*.
CPE-1822 is entirely about `gui-smoke` coverage for the **Trash view** and has nothing to do with the
status bar — the reference is wrong and could not have been satisfied by anyone touching
`StatusBar.svelte`. Found by PR #1019's reviewer, which read CPE-1822 rather than assuming. **This
ticket is the correctly-scoped replacement for that bullet.**

## Relationship to the driver mismatch

Fixing `msedgedriver` against the installed Edge (recorded in
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`) is still worth doing for **full-app flows** — real
navigation, real Tauri commands, real trash operations. But it is no longer on the critical path for
**layout** claims, and this route is cheaper, faster and has fewer moving parts. Do this first.

## Acceptance criteria

- [x] A CI job that measures real layout for at least two components at multiple widths. —
      `layout-guard` in `.github/workflows/gui-smoke.yml`, `statusbar-notice` (5 widths) +
      `trash-titlebar` (7 widths, new harness page).
- [x] Reintroducing CPE-1836's bug makes it red, naming the overlap — demonstrated locally (see Work
      Log): `CLIP-BREACH .git: .git .git-btn:not(.resolve) overhangs by 16.1px AND paints there ... —
      not clipped`. CI's own run of the same job is pending — I cannot watch CI (CPE-1880); the
      Foreman owns that verdict.
- [x] Reintroducing CPE-1827's bug makes it red — demonstrated locally (see Work Log):
      `TEXT-OVERFLOW .tv-title scrollWidth=91 clientWidth=0 overflow-x=visible — text paints past its
      own background`, at the app's own 600px floor.
- [x] A ticket author can add a component and a width list without touching harness internals. —
      `cases.mjs` is the one file touched; harness page (index.html+main.ts) is the same per-component
      work every existing harness already requires. Backend-talking components use the shared,
      pluggable mock (`shared-mocks/invoke.ts`'s `registerRawInvoke`) — no bespoke mock file needed.
- [x] `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` updated: this closes the layout half of the
      GUI-verification debt, and the row says so (row #3, half-closed — pixel-baseline blessing is
      still open, separate mechanism).

## Work Log

- **2026-08-23 18:45 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Three separate agents converged on this today from different directions: the CPE-1833/1836 worker
  built the harness, PR #1019's reviewer proved it never reaches CI and found the mis-referenced AC,
  and the CPE-1827 worker independently lost hours to the driver mismatch this route sidesteps.

- **2026-08-23 (CPE-1884 worker)** — A third concrete red-proof case for item 3 (alongside CPE-1836's
  status bar and CPE-1827's Trash titlebar): CPE-1884 (the Drop Stack handle floating over the
  Sidebar's bottom rows) is the same class of bug — `.drop-stack-handle` (`position: fixed`) painting
  over Sidebar.svelte content — and I built a standalone version of exactly this harness while fixing
  it, since this ticket hadn't landed yet: `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs`
  (`npm run harness:sidebar-drop-stack-overlap`). Same approach this ticket already specifies — plain
  `chrome.exe --headless=new` + raw CDP (`Runtime.evaluate`/`Emulation.setDeviceMetricsOverride`), no
  WebDriver, no npm deps — but spins up its own `vite` dev server and drives the REAL app rather than a
  purpose-built stand-in page, and asserts a structural containment invariant
  (`.navigation-pane`'s own rendered box never extends into `.drop-stack-handle`'s y-range) rather than
  per-pixel overlap pairs. Red-proofed twice by deliberately reverting the CPE-1884 fix — worth reading
  before generalising: v1 of my probe (checking the handle's own corners) never failed, because
  `elementFromPoint` at an element's own rect trivially returns that element (it wins the paint order
  there by definition — checking the WRONG side's corners can never observe this class of bug); v2
  (checking every row's own click-center) produced false positives for rows simply scrolled outside
  the container's own clip, unrelated to the actual defect. Not wired into CI — left for this ticket.
  See CPE-1884's Work Log for the full writeup (repro screenshots, before/after evidence, the fix
  itself) and `gui-smoke/known-failing.json`'s four `trash-titlebar.smoke.ts` entries (tag `CPE-1822`)
  it could not itself clear (same msedgedriver/WebKitGTK gap this ticket exists to route around).

- **2026-08-26 (Worker)** — Picked up. Plan: generalise `statusbar-notice`'s prototype into
  `scripts/dev-harness/layout-guard/` (a CDP-driving engine with four composable check kinds —
  `siblingOverlap`, `clipProbe`, `textOverflow`, `selfPaint` — reusing `sidebar-drop-stack-overlap`'s
  CDP-over-`chrome.exe --headless=new` shape rather than the outer/iframe `--dump-dom` shape, since
  `Emulation.setDeviceMetricsOverride` sets the real CSS viewport directly and needs no iframe trick), a
  `cases.mjs` manifest (the one file a future ticket touches to add a case), and ONE shared dev server
  (`vite.harness.layout-guard.config.ts`) with a generic, pluggable backend mock
  (`shared-mocks/invoke.ts`'s `registerRawInvoke`) so a new case never needs a bespoke mock file either.
  Two cases: `statusbar-notice` (reuses the existing harness page, red-proofs CPE-1836) and a new
  `trash-titlebar` (new harness page mounting the real `TrashView.svelte`, red-proofs CPE-1827). Wiring
  into `.github/workflows/gui-smoke.yml` as a new job, unconditional on every push/PR (measured cost:
  under a minute end to end — cheap enough that path-filtering isn't worth the CPE-1893-shaped risk of a
  silently-skipped check).

  First local run against the real (fixed) code caught the engine's own bugs, not real regressions —
  worth recording since they shaped the final design: (1) `scrollWidth > clientWidth` alone is NOT "text
  overflows its own background" — `.git-branch`/`.disk`/etc. correctly ellipsis-truncate
  (`overflow: hidden; text-overflow: ellipsis`), which legitimately makes scrollWidth exceed
  clientWidth while painting nothing outside the box; the `textOverflow` check now also requires
  `getComputedStyle(el).overflowX === "visible"` before flagging. (2) `.resize-grip`
  (`position: absolute; right: 0; bottom: 0`) is BY DESIGN allowed to sit over the tail of trailing flow
  content in the corner — added as `siblingOverlap`'s `exclude` option, matching the original CPE-1836
  prototype's own judgment call. Both false positives are recorded in engine.mjs's own comments so a
  future case doesn't rediscover them.

  **Red-proofed both AC-mandated bugs, locally, against the real components:**
  - CPE-1836: removed `.git { overflow: hidden }` in `StatusBar.svelte` → `layout-guard` went red at
    600px: `CLIP-BREACH .git: .git .git-btn:not(.resolve) overhangs by 16.1px AND paints there (probe
    (547.2,13.5) hit .git-btn) — not clipped`. Restored the line → clean at all 12 case/width
    combinations again (confirmed `git diff` shows zero change to `StatusBar.svelte`).
  - CPE-1827: reintroduced the pre-fix shape in `TrashView.svelte` (dropped `.tv-title`'s
    `flex-wrap: wrap` back to the old pinned width, and added dummy buttons to `.tv-tools` to restore
    the old toolbar density) → `layout-guard` went red at the app's own 600px/640px floor: `TEXT-OVERFLOW
    .tv-title scrollWidth=91 clientWidth=0 overflow-x=visible — text paints past its own background`.
    Reverted both → clean again (confirmed `git diff` shows zero change to `TrashView.svelte`).
    **[Reviewer correction, see below]: this originally said "5 dummy buttons" — wrong, corrected after
    an independent reviewer could not reproduce with literally 5. See the 2026-08-26 UAT-round-2 entry
    for the accurate repro recipe.**

  Both real components ship unchanged — only the harness itself (engine.mjs/cases.mjs) is a permanent
  diff. Next: wire the `layout-guard` job into `.github/workflows/gui-smoke.yml`, run `npm run
  check`/`npx vitest run`, update `MANUAL-TEST-BURNDOWN.md`, open the PR.

- **2026-08-26 (Worker, wrap-up)** — CI job added (`layout-guard`, unconditional + blocking, every
  push/PR; `.github/workflows/gui-smoke.yml`). Cost measured locally: ~35s dev-server cold start + ~1-2s
  per width thereafter, ~1 minute end to end for the 12 case/width combinations shipped today — cheap
  enough that it is NOT path-filtered (deliberate: at this cost, a silently-skipped run reads
  indistinguishably from a passed one in the checks list, exactly CPE-1893's shape, and isn't worth the
  risk for a minute of runner time). `MANUAL-TEST-BURNDOWN.md` row #3 updated (half-closed: layout
  geometry automated, pixel-baseline blessing still open, row stays 🔧 not ✅).

  Guardrails: `npm run check` → 0 errors, 0 warnings. `npx vitest run` → 3 pre-existing failures
  (`lockfileLockedGuard.test.ts`'s `release-sidecar.yml` `--locked` check, `msrvSync.test.ts` x2 — no
  `msrv:` job in `ci.yml`), confirmed via `git stash` to already fail identically on this branch's base
  commit before any of this ticket's changes — unrelated to this ticket, not touched (`ci.yml` and
  `release-sidecar.yml` are both outside this ticket's scope and the sibling-agent conflict note).
  Rebased cleanly onto `origin/main` (`40bb6193`, no conflicts), all three checks re-run clean
  post-rebase.

  **Status:** PR ready to open. Real components (`StatusBar.svelte`, `TrashView.svelte`) ship with
  ZERO diff — every red-proof edit was reverted, confirmed via `git status`/`git diff`. Staying in
  `Doing/` until the PR merges (CI verdict owned by the Foreman per CPE-1880 — I cannot watch it).

- **2026-08-26 (UAT attempt 2 — BLOCKING fail, worker response)** — PR #1035 UAT: the `layout-guard`
  job crashed on **every real CI run**, before measuring a single pixel — `ReferenceError: WebSocket is
  not defined` (job 98371907013). Root cause: `engine.mjs`'s CDP client calls the global `WebSocket`
  constructor directly, which is only a stable Node built-in from v22; the job was pinned to
  `node-version: 20` (copy-pasted from this workflow's other jobs, all of which stay on 20 — this ONE
  job does not, deliberately). Every local red-proof in the original pass ran on local Node ≥22, which
  never exercises what CI actually runs — the gap the UAT named exactly: "the harness works" vs. "the
  harness is wired into CI". Fixed: pinned `layout-guard`'s own `Setup Node` step to 22 (see that step's
  own comment for why 22, not the `ws` package). `engine.mjs`'s header now states the minimum Node
  version explicitly. `cases.mjs` gained the UAT's own decision table (its independent case-building
  test found `siblingOverlap` doesn't fire for a missing `flex-wrap` — pushes the row past the viewport
  or shoves the next block down, doesn't make chips overlap). `siblingOverlap`'s failure message now
  computes a human "overlap by NxM px" figure (verified with a real self-inflicted overlap:
  `.git × .dim overlap by 26.0px × 16.0px`, then reverted, zero diff). `MANUAL-TEST-BURNDOWN.md`
  corrected to not claim CI enforcement before a real CI run confirms it, with a same-shift correction
  entry recording the root cause.

- **2026-08-26 (independent reviewer, same attempt) — 4 more findings, folded in**:
  1. **Real local flakiness, cause identified**: 1/9 local runs measured a class (`.tv-sync-badge`) that
     exists in no worktree's committed code — the reviewer's OWN in-progress fixture from a DIFFERENT
     worktree's UAT test, running concurrently. Root cause: `cdpPortBase` (engine.mjs) defaulted to a
     fixed `9600` and the vite dev server (run.mjs) to a fixed `4331` + `strictPort: true` — two
     concurrent `run.mjs` processes (different worktrees, same codebase, the NORMAL condition on this
     dev machine) could pick the same dev-server port; the OLD code's readiness check
     (`waitForHttp`) only confirmed "something answered HTTP 200", which a FOREIGN worktree's own
     already-running server on that same port satisfies just as well as this run's own — a genuine race,
     not a theory. Fixed with two independent layers: (a) both the CDP port base and the dev-server port
     now derive from `process.pid` (unique per concurrently-running process), making an actual collision
     between two SEPARATE runs astronomically unlikely instead of routine; (b) `run.mjs`'s
     `waitForViteBoundHere` now requires THIS process's own vite child to announce, on its own stdout,
     that it bound this exact port (not merely "something answered HTTP") — caught + fixed a real bug
     while building this: vite colours its "Local:" banner with ANSI escapes and inserts one literally
     between "localhost:" and the port digits, so a plain string match on "localhost:<port>" never
     matched the raw bytes; strips ANSI codes first now. (c) `checkOneWidthHeight`'s ready-poll now also
     asserts `location.href` matches the exact URL this run navigated to, before trusting `readySelector`
     at all, as a second independent layer inside the browser itself.
  2. **`MISSING` selector must fail the run — self-tested, already did.** Built a real self-test (a case
     pointed at a selector that cannot exist), ran the harness, confirmed `process.exitCode` was already
     1 with the missing selector named in the output, then reverted (`git diff` on `cases.mjs`: zero net
     change from the self-test). Could not reproduce the specific "reported MISSING, still exited 0"
     symptom directly — most likely explained by finding 1 (their run may have been measuring a
     different worktree's page entirely, which can produce confusing combined results). Fixed the
     SEPARATE, real bug the same finding named: the shared mock alias (`vite.harness.layout-guard.
     config.ts`) only matched `../invoke`/`../bindings.gen` (written by a component under
     `src/lib/components/*.svelte`), not `./invoke`/`./bindings.gen` (written by a plain service module
     living in `src/lib/*.ts`, e.g. `src/lib/tags.ts` — exactly what `TagEditor.svelte`'s seed path goes
     through). Broadened the alias regex to match both depths.
  3. **Unbounded local disk growth — real, 1.4 GB reclaimed.** `checkOneWidthHeight` created a fresh
     Chrome profile dir per width and never deleted it, only `chrome.kill()`. Confirmed before the fix:
     a full 12-width run left new dirs behind. Fixed: waits for the process to actually exit (bounded at
     3s, not just for `kill()` to have been called) then deletes the dir, best-effort (a still-held
     Windows file lock right after exit must never fail the actual layout check). Confirmed after the
     fix: a full 12-width run left the leftover-dir count UNCHANGED (147 before, 147 after) — zero new
     leaks. Deleted the pre-fix accumulated debt (1.4 GB, gitignored scratch, not tracked).
  4. **Work Log wording fixed** — see the corrected note on the original CPE-1827 red-proof entry above.
     Re-verified the ACCURATE repro: `flex-wrap: wrap` removed AND 8 wider-text buttons added to
     `.tv-tools` (not 5) reproduces the failure at every width 600-1200px this time (`TEXT-OVERFLOW` +
     `UNREACHABLE .tv-x` both fire once the toolbar is wide enough to push Close off-panel entirely, a
     stronger/more convincing repro than the original one-width finding). Reverted, zero diff confirmed.

  **Non-blocking, noted per the reviewer's ask, not built under the attempt cap**: nothing today asserts
  the OTHER half of the pill convention — that a pill's own text stays on one line and does not shrink.
  A pill that wraps internally instead of ellipsising is invisible to `textOverflow` unless it ALSO
  overlaps or truly overflows. A fifth check kind (`whiteSpace`-based: flag any watched element whose
  `getComputedStyle().whiteSpace` isn't `nowrap` when the case declares it as pill-shaped) would close
  this. Flagged for the reviewer/Foreman to file as its own ticket rather than adding a fifth check kind
  here under the attempt cap.

  Guardrails re-run after all of the above: `npm run check` → 0 errors, 0 warnings.
  `npm run harness:layout-guard` → clean at all 12 case/width combinations, twice in a row (once to
  confirm the port/cleanup fixes, once after the CPE-1827 repro re-check's revert). Rebased onto the
  Foreman's merge of `main` (which carried the `--locked` fix for `release-sidecar.yml` — one of the 3
  vitest failures flagged as pre-existing in the previous pass is now gone on `main` itself). `npx vitest
  run` then surfaced two more CRLF-checked-out `.mjs` files unrelated to this ticket
  (`scripts/organize-done.mjs`, `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs` — neither
  touched by CPE-1882) via `sprintStallControls.test.ts`'s CPE-1880 checkout guard; fixed both with the
  test's own suggested remedy (`rm <file> && git checkout -- <file>`, zero-diff re-materialisation from
  the already-LF index — confirmed via `git status`). Final `npx vitest run`: **2 failures, both
  `msrvSync.test.ts`** (no `msrv:` job in `ci.yml` — CPE-1855's territory, unrelated to and out of scope
  for this ticket), 4581/4583 passing.

- **2026-08-26 (CI round 3 — real CI run, job cancelled at its own 10-minute cap)** — Pushed, watched
  `layout-guard` (job `98379701649`) via `gh api .../jobs/<id>` reads (not `gh run watch`). Its own step
  timeline showed `npm ci` finishing in 4s (cache hit) and `Resolve + confirm Chrome` in 2s — the setup
  was fine. `Run the layout guard` itself ran for **10m14s** before the job's own `timeout-minutes: 10`
  cancelled it — a real finding, not a fluke: the local "~1 minute" cost claim never exercised a real CI
  runner, and this design does not survive contact with one.

  **Root cause**: `runAllCases` (engine.mjs) launched a FRESH Chrome process + fresh profile dir for
  EVERY width — 12 full process-spawn-plus-CDP-handshake cycles for the two shipped cases. Cheap on a
  dev workstation; evidently not cheap on a shared/throttled GitHub-hosted `ubuntu-latest` runner.

  **Fix**: refactored to launch ONE Chrome instance for the WHOLE sweep, reused across every case×width
  via `Page.navigate` + `Emulation.setDeviceMetricsOverride` per width (same CDP calls as before, just
  against a long-lived connection instead of a fresh one each time) — removes 11 of 12 process launches
  for today's two cases. This does not reintroduce the "fresh profile avoids a stale cached app.css"
  concern the original per-width design cited (that concern is about a profile REUSED ACROSS SEPARATE
  DAYS of local dev-loop iteration — see `sidebar-drop-stack-overlap/check.mjs`'s own comment — not
  about one browser process living for the few seconds one run's own sweep takes; vite always serves
  current content regardless of the browser's own cache, and every case still gets a genuine fresh
  `Page.navigate`).

  Re-verified locally after the refactor: `npm run harness:layout-guard` clean at all 12 combinations
  (~28s wall-clock, `date +%s` before/after — comparable to the pre-refactor local timing, since local
  Chrome launches were never the bottleneck; the fix targets CI's launch cost specifically), zero
  leftover profile dirs (cleanup still fires correctly on the single reused instance), and the CPE-1836
  red-proof still reproduces + reverts cleanly on the new single-instance design (proving no state leaks
  between cases sharing one Chrome tab). `npm run check`: 0 errors, 0 warnings.

  Also bumped `layout-guard`'s own `timeout-minutes` 10 → 15 (headroom over the now-expected wall-clock,
  not a claim it needs anywhere near that long) and corrected the cost comments in `run.mjs`'s header and
  the job's own YAML comment to record this finding rather than repeat the unverified "~1 minute" claim.
  **CI verdict on the refactored design is still unconfirmed** — pushing this fix now; the Foreman owns
  watching the next run to completion, per CPE-1880.
