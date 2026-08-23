---
id: CPE-1859
title: the disk readout is right-aligned only by accident of the git chip preceding it
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`.disk` in the status bar has **no right-anchor of its own** — only `margin-left: 12px`. It sits at the
right edge purely because `.git` precedes it carrying `margin-left: auto`.

So when the git chip is absent while the disk figure is present, the free-space text renders
**left-adjacent to the item count** instead of at the right edge.

That window is short but real, and CPE-1854 created a path that exposes it. Leaving an archive back into
a git repository refetches both readouts independently; `disk_space` is fast and `forge_repo_status` is
slow on a large repository or a network share. So the sequence is: disk lands and renders in the wrong
place, then the chip lands and the disk figure **jumps** to the right edge.

Sub-second, and pre-existing in the CSS rather than introduced by CPE-1854 — but before that ticket the
chip was never cleared on entering those views, so the path did not exist.

## A second, related staleness

The sidebar's per-drive usage bars (`Sidebar.svelte:796-804`) are filled on mount and on drive-list change
only (`loadDriveUsage`, `App.svelte:1606`). They can be hours stale.

That matters more than it used to: CPE-1854's UAT justified hiding the status-bar disk figure in virtual
views partly on the grounds that free space is *already on screen permanently* in the sidebar. That
argument is sound, and it makes the sidebar the primary free-space readout — which is worth it being
fresh, or at least refreshed on a coarse timer.

## Acceptance criteria

- [ ] `.disk` anchors itself rather than relying on a sibling. `margin-left: auto` on `.disk` is the
      obvious fix; confirm it does not change the layout when both are present.
- [ ] Verify the fix in a real render, not in jsdom. This project's vitest config applies **no component
      CSS** to `getComputedStyle`, so a unit test cannot see this class of defect at all — that is exactly
      why it survived. Use the gui-smoke harness or a screenshot, and say which.
- [ ] Check the same shape for every other status-bar item that has no anchor of its own and relies on a
      neighbour's `auto` margin. Enumerate the row rather than fixing only `.disk`.
- [ ] Decide whether the sidebar drive bars should refresh on a timer or on navigation, and record the
      cost either way. If the answer is no, say what keeps them honest.

## Notes

Found by the independent UAT during CPE-1854, which flagged it as reasoned-from-markup rather than
measured: it could read `app.css:1451` and `StatusBar.svelte:135-154` but could not verify any of it in
jsdom. It also confirmed the parts that are *not* a problem — `.statusbar` is a fixed `height: 26px` so
there is no vertical jump, and the left-hand items do not shift because `.git` carries the only
`margin-left: auto`.

Related: CPE-1854 (created the exposing path), CPE-1836 (the row's layout at the 600px floor), CPE-1840
(the two count fields in the same row).

## Work Log

### 2026-08-22 — fixed, branch `cpe-1859-disk-anchor`

### The framing in the ticket is wrong, and the correction is the main finding

Both this ticket and CPE-1854's log describe the misplacement as a **sub-second race** while two
independent fetches land. It is not. `{#if git && git.is_repo}` removes the chip in **any folder that is
not a git repository**, and `{#if diskLabel}` renders the free-space figure in every ordinary folder. So
the two gates disagree permanently, not momentarily: on this machine, in `C:\Windows`, in Documents, in
Downloads — anywhere without a `.git` — the free-space text has been sitting next to the item count
since CPE-403 shipped it in July. The race CPE-1854 created is one narrow instance of a defect that was
already the **majority** case.

That was measured, not reasoned: see the red-proof table below, where the chip is simply switched off.

### The fix, and why the fix the ticket proposed is wrong

```css
.disk      { margin-left: auto; }   /* anchors ITSELF */
.git ~ .disk { margin-left: 12px; } /* …unless `.git` is actually there to anchor the cluster */
```

The ticket's suggested one-liner — `margin-left: auto` on `.disk` and nothing else — **changes the
layout when both readouts are present**, which is exactly what the acceptance criterion asked to be
confirmed. Flexbox distributes positive free space **equally among all main-axis auto margins**, so a
second auto margin stops `.git` anchoring and parks it mid-row. Measured in real Chrome at a 764px
viewport: `.git` moved from `left=501.3` to `left=293.1`, a 208px displacement, while `.disk` stayed
flush right. At 900px the same mutation moved it 637.3 → 361.1 (276px). So the naive fix trades a
defect in the no-repo case for a worse one in the repo case.

Hence the pair. Specificity does the selection, not source order: `.git ~ .disk` is 0-2-0 against
`.disk`'s 0-1-0. Confirmed in the **compiled production CSS**, not only in source — after Svelte's
scoping pass `npm run build` emits

```
.disk.svelte-bn7rcz.svelte-bn7rcz{margin-left:auto;…}
.git.svelte-bn7rcz~.disk.svelte-bn7rcz{margin-left:12px}
```

(0-3-0 vs 0-4-0, override intact) and `svelte-check` does not prune the sibling rule as unused.

### How it was verified in a real render — and why nothing else would do

**Harness, not screenshot-only, and not jsdom:** `scripts/dev-harness/statusbar-notice`, extended here.
It mounts the **real `StatusBar.svelte`** with the **real `src/app.css`** in a real browser and reports
`getBoundingClientRect` for `.item-count` / `.git` / `.disk` against the bar's right padding edge (read
from `getComputedStyle`, not hard-coded, so a padding change cannot turn a regression into a passing
number). Driven by installed Chrome, `--headless=new --virtual-time-budget=15000 --dump-dom`. Two derived
numbers carry the whole assertion: `diskRightGap` (0 ⇒ anchored right) and `diskFromItemCount`
(≈14, the bar's `gap`, ⇒ glued to the count).

This project's vitest config runs jsdom, which has **no layout engine at all** — zeros from
`getBoundingClientRect`, nothing from `getComputedStyle` for a scoped `<style>` block. No unit test
under this config can see this class of defect; that is why it survived a month and four status-bar
tickets that all touched this exact rule set.

**Measured — all four gate combinations, at innerWidth=764, content right edge = 750.0:**

| `.git` | `.disk` | before the fix | after the fix |
|---|---|---|---|
| absent | present | `.disk left=84.9 right=216.0` — **`diskRightGap=534.0px`**, `diskFromItemCount=26.0px` | `left=619.0 right=750.0` — `diskRightGap=0.0px` |
| present | present | `.git left=501.3`, `.disk right=750.0`, gap 0.0 | **identical**: `.git left=501.3`, `.disk right=750.0`, gap 0.0 |
| present | absent | `.git left=658.3 right=750.0` | identical |
| absent | absent | nothing to place | identical |

The second row is the acceptance criterion "confirm it does not change the layout when both are
present", answered by measurement: byte-identical rects.

**Also captured as screenshots** (rule removed vs. rule present, chip off, w=900): the free-space text
visibly reads `42 items   115.0 GB free of 465.7 GB` hard against the item count, versus flush right.
Kept out of the repo; the numeric dumps above are the durable record.

**Red-proof — the rule removed, observed, then restored** (`.disk` back to `margin-left: 12px`, sibling
rule deleted, i.e. the pre-ticket state):

| Mutation | Real-render result | Unit-test result |
|---|---|---|
| Pre-ticket state (12px, no sibling rule) | `diskRightGap` 0.0 → **534.0px**, `diskFromItemCount` 26.0px | `StatusBar.diskAnchor` **3 failed / 2 passed** |
| The naive fix (auto on `.disk`, no sibling rule) | `.git` `left=501.3` → **293.1** in the both-present case | **1 failed / 4 passed** — exactly the sibling-rule test |
| Restore | back to 0.0px / `left=501.3`; file md5 `f8b3c3a5…` matched the pre-mutation copy | 5/5 green |

### The row, enumerated — every child, not just `.disk`

`.statusbar` (`app.css:1451`) is `display:flex; align-items:center; gap:14px; height:26px; padding:0 14px`.
In DOM order:

| # | Child | Rendered when | Anchor of its own | Relies on a neighbour's `auto` margin? |
|---|---|---|---|---|
| 1 | `.item-count` | **always** | first in flow ⇒ left edge | no |
| 2 | `.selected-count` | `selectedCount > 0` | none needed (left group) | no |
| 3 | `.dim` "Hidden files shown" | `hiddenShown` | none needed (left group) | no |
| 4 | `.filtered-hidden` | `filteredHidden > 0` | none needed (left group) | no |
| 5 | `.unreadable` | `unreadableCount > 0` | none needed (left group) | no |
| 6 | `.notice` | `notice` non-empty | none needed (left group) | no |
| 7 | `.git` | `git && git.is_repo` | **`margin-left: auto`** | no |
| 8 | `.disk` | `diskLabel` non-empty | **was NONE — only `margin-left: 12px`** | **was YES — on `.git` (7). Fixed.** |
| 9 | `.resize-grip` | **always** | `position:absolute; right:0; bottom:0` — out of flow entirely | no |

Rows 2–6 have no anchor and do not need one, and the reason is worth stating rather than asserting:
their reference point is `.item-count`, which is **unconditional**. Removing any left-group member only
closes a gap leftward — none of them can be displaced to the wrong END of the bar. The failure mode is
specific to a member of the RIGHT-hand group whose anchor lives on a **conditional** sibling, and `.disk`
was the only one. Row 9 is out of flow and cannot be affected by any sibling.

`.git`'s own children (`.git-branch`, two `.git-ct`, `.git-dirty`, `.git-conflict`, the `.git-btn`s) are a
nested flex with `gap: 6px` and **no auto margins at all**; the conditional ones flow rightward from the
unconditional `.git-branch`. Same shape as rows 2–6, same verdict.

Pinned mechanically as well as narratively: `StatusBar.diskAnchor.test.ts` asserts the set of rules in
the block carrying `margin-left: auto` is **exactly** `[".disk", ".git"]`, so a third one cannot be added
without failing.

**Swept the three sibling status rows too, since "enumerate the row" is worthless if the same shape is
next door:**

- `.board-statusbar` (`BoardView.svelte:510`, CPE-529) — `.sb-root` sits right because `.sb-msg` carries
  `flex: 1`. `.sb-msg` renders **unconditionally** (`{error || note || ""}` — an always-present span, empty
  or not), so the pusher cannot vanish. Safe, and safe for a structural reason, not by luck.
- `.cd-statusbar` (`CardDetailDialog.svelte:213`) — `.sb-meta` carries `margin-left: auto` and is
  unconditional; the only conditional child (`.cd-grip`, `{#if !standalone}`) FOLLOWS it. Removing a
  follower cannot un-anchor its predecessor. Safe.
- `.repo-statusbar` (`RepoBrowser.svelte:454`) — no auto margins, no right-hand group; both children are
  left-aligned and the conditional one is last. Nothing to anchor. Safe.

### The sidebar drive bars — decision: refresh, on a 60s tick **and** on window focus. Not on navigation.

Implemented, not merely recorded. `App.svelte` gains `refreshDriveUsage()` wired to
`setInterval(…, 60_000)` and `window.addEventListener("focus", …)` in `onMount`, both torn down in
`onDestroy`, guarded by an `inFlight` flag.

**Why refresh at all, and why now.** Until CPE-1854 these bars were secondary: the status bar carried a
live per-folder free-space figure refetched on every navigation. CPE-1854 removed that figure in
Home/archives/smart folders/structured searches, and its UAT justified the removal partly because free
space is "already on screen permanently" here. That argument is sound and it **promotes** the sidebar to
the primary free-space readout — at which point a value probed once at launch is a false statement, not
a slightly old one. Same standard this run applied to the stale branch chip and the stale counts.

**Why not on navigation.** Navigation is this app's hottest path, and the fact is per-**drive**, not
per-folder: opening a subfolder cannot change it. A per-navigation refresh would issue one `disk_space`
per drive for a value that has not moved — the explorer's fast/small/predictable tiebreaker says no.

**Cost, stated both ways.** On the timer: one `GetDiskFreeSpaceEx`-class call per drive per minute
(2–4 calls/min on a typical machine), plus a 60s wake while the app is open. Against not doing it: the
primary free-space readout stays wrong until the app is relaunched or a drive is plugged in. The focus
half is the one that actually matters — the worst staleness is precisely after the user has been away in
another app for hours — and it costs nothing while the app is not being switched to.

**`inFlight` is not defensive padding.** The per-minute cost is not uniform: `disk_space` against a
disconnected mapped network drive can block for the OS's own timeout, which is longer than the tick.
Without the guard a dead share accumulates one probe per tick indefinitely. Note the exposure is new in
**frequency only** — mount and every drive-set change already probe the same paths the same way.

**Deliberately not done:** no `document.hidden` guard. It would trim idle work while minimised, but its
behaviour under WebView2 is not something this ticket could verify, and a wrong "hidden" reading would
silently disable the timer half. The focus listener already covers the un-minimise case. Named here
rather than left as an unexplained absence.

### Tests — and an explicit statement of which ones can see anything

`src/App.sidebarDriveUsage.test.ts` — **4 tests, genuinely behavioural.** They render App and read the
figure the sidebar is showing before and after: mount, refresh-on-focus, refresh-on-60s-tick, and the
in-flight guard. Each is a before/after pair inside one render, so an assertion cannot pass vacuously.
The drive lives at `D:\x`, outside every folder the harness can navigate to, so every `disk_space` call
naming it comes from `loadDriveUsage` and never from the status bar's own probe — which is what makes
the call counts mean what they say. Staleness is a **text** question, so jsdom can see all of this.

| Mutation | Result |
|---|---|
| drop `window.addEventListener("focus", refreshDriveUsage)` | 2 failed / 2 passed — focus test **and** the in-flight test |
| drop `setInterval(refreshDriveUsage, 60_000)` | 1 failed / 3 passed — the tick test only |
| drop `driveUsageInFlight ||` from the guard | 1 failed / 3 passed — the in-flight test only, `expected 4 to be 2` |

`src/lib/components/StatusBar.diskAnchor.test.ts` — **5 tests that pin the RULE'S PRESENCE, not its
effect,** and say so in their own header. jsdom cannot observe where `.disk` lands; these parse the
component's `<style>` block. Their narrow value is real but bounded: they are the only thing in CI that
fails if a future edit deletes either half of a two-rule mechanism whose halves each look individually
redundant. A green run here means "the declarations are still written down", never "the bar lays out
correctly". It deliberately does **not** reuse `StatusBar.notice.test.ts`'s looser `ruleBody` helper,
which allows any whitespace before the selector and would therefore match `.git ~ .disk`'s tail when
asked for `.disk` — the difference between those two selectors being this file's entire subject.

### The harness itself needed fixing to be trusted

Extended with `?git=on|off&disk=on|off` and `?notice=none`, plus the position diagnostics. Three
robustness fixes were forced by driving it non-interactively rather than by hand, and are recorded
because each produced a **plausible-looking wrong answer** rather than an error:

1. The outer shell polled the iframe for 40 × 50ms. Virtual time **fast-forwards timers**, so the whole
   budget burned in one tick before the iframe's modules (fetched over the dev server — real time, which
   virtual time does not compress) had executed. Cap raised to 2000, and the poll no longer waits for the
   iframe's `load` event to start.
2. `requestAnimationFrame` was tried in place of `setInterval` and is **worse**: the outer frame's rAF
   callbacks do not advance under the virtual-time policy at all — every run froze rather than one in
   three. Recorded so it is not re-attempted.
3. The reliable fix was to stop reading across the frame boundary: the inner document now prints its own
   diagnostics into its own DOM, so `--dump-dom` on `inner.html` captures a complete measurement with no
   polling. Its double-rAF publish also gained two idempotent backstops (`load`, `setTimeout(…,0)`),
   without which about one driven run in three snapshotted "booting…". Verified by **6 consecutive runs**
   returning `diskRightGap=0.0px`. The outer iframe shell remains the right entry point when the
   measurement depends on a controlled narrow WIDTH, which is what it exists for (CPE-1660).

Every "waiting for iframe…" snapshot along the way was a **missing** measurement, never a wrong one — but
a harness that silently returns nothing a third of the time is not evidence anybody should rely on.

### Also touched

- `src/lib/bidiEscape.guard.test.ts` — its recorded absolute App.svelte line numbers. The insert splits
  above and below the script offenders: markup offenders **+46** (31 entries, all shifted uniformly —
  verified against the test's own failure output, e.g. 6441→6487 and 7760→7806), script basename
  allowlist **+35** (2789→2824, 2804→2839). The SET is unchanged; nothing added or removed.
- `src/docs/03-explorer.md` — a new sidebar bullet stating the bars refresh about once a minute and on
  window switch, and that they are read per drive rather than per folder; plus a pointer to it from the
  existing "Free space and the git chip" bullet, since CPE-1854 made that the place a reader lands when
  the status-bar figure is absent. No new section, so `src/lib/sectionDocs.ts` is untouched.

### Gates

`npx vitest run` — **327 files, 4349 tests, all passing**. Baseline on `origin/main` was **325 files,
4340 tests**; this adds **two files and nine tests** (5 anchor + 4 sidebar). The intermediate run
recorded mid-ticket, 326/4345, is the state after the anchor file and before the sidebar file — noted
so the two sets of numbers are not read as contradicting each other. `npm run check` — **0 errors, 0 warnings**;
notably it does **not** flag `.git ~ .disk` as an unused selector. `npm run build` — clean, and the
compiled CSS was inspected directly (above) rather than assumed. gui-smoke `npm run test:unit` not run:
nothing under `gui-smoke/` was touched.

Line endings checked after every edit: all six changed files and both new files are 100% CRLF, no bare
LF, no BOM (`src/App.svelte` 7926/7926, `StatusBar.svelte` 360/360, `bidiEscape.guard.test.ts` 555/555,
`03-explorer.md` 386/386, both harness `.ts` files, both new test files). The new files were written LF
and converted with `awk`, never `sed -i`. This Work Log was appended with the Edit tool.

### Could not verify

- **Not verified in the shipped app.** No `tauri build`, no install, no GUI run. The render measured is
  the real component and the real stylesheet in real Chromium via the dev harness — the same engine
  family as WebView2, but not the shipped binary. A `margin-left: auto`/sibling-combinator difference
  between the two is not credible, but it was not observed.
- **The sidebar refresh's real-world behaviour is untested outside jsdom.** Whether a minimised WebView2
  window still fires `focus`, and whether a 60s wake is perceptible on battery, were not measured — the
  tests dispatch synthetic events and use fake timers.
- **`disk_space` against a genuinely disconnected mapped network drive was not measured.** The in-flight
  guard is reasoned from that risk and pinned by a synthetic held probe, not by a real dead share. The
  QNAP on the LAN would be the honest way to check it.
- **Narrow-width behaviour was not re-checked.** All measurements are at a comfortable width; whether the
  new anchor interacts with the shrink-priority system at the app's 600px floor belongs to CPE-1836 and
  was not exercised.
- **Light/dark was not re-checked** — this change touches no colour.

### CI, including one red that was chased rather than waved through

PR **#999**, head **fc15379**. Everything green on the second attempt; recording the first attempt
because "re-ran it and it passed" is only honest with the evidence attached.

**GUI smoke shard 3 failed on the first run** (job 97110095901). It matters here more than usual: this
change adds a 60s `setInterval` to the running app, so "my change destabilised the GUI suite" was a live
hypothesis and not one to dismiss by reflex. What the evidence actually says:

- The failing step was the **ratchet**, not a spec: `failed to parse wdio-shard-3-of-4-0-7.json as JSON:
  Unexpected end of JSON input` — a truncated reporter file for a worker that never finished writing.
- Specs 0-0…0-4 all `PASSED`; the suite log then stops mid-spec `0-5` (`open-dir.smoke.ts`) and specs
  0-6…0-9 produced no result at all. The job ran 6m55s against shard 2's 14m1s in the same run — the
  wdio process ended early rather than a case going red.
- The repo's own classifier (CPE-1728) called it: **"0 AssertionError occurrence(s), 102
  environment-signature occurrence(s)… the signature of a renderer that did not paint/settle in time
  under CI, not of a broken assertion."**
- Re-ran the failed jobs on the **same SHA**, no code change: shard 3 **pass, 7m3s**, all four shards
  pass, cross-shard verdict pass.

Same SHA, opposite outcome, and no assertion ever fired — so the red was the runner, not the diff. Worth
noting that `cpe-1858-shard-balance` was in flight on this repo at the same time, i.e. shard timing is a
known live concern rather than a surprise.

**Final: 19 checks, all pass.** Frontend type-check and test, Backend × 3 OSes, Server crates × 3,
Sidecar platform × 3, Network E2E, all four GUI smoke shards + the cross-shard verdict.
