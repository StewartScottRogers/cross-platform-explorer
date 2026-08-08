---
id: CPE-1481
title: "gui-smoke Linux leg: get it fully green — 8 revealed environmental spec failures + 20min job timeout too short now that mouse works"
type: Bug
status: Doing
priority: High
component: CI/QA-infra
tags: [ready]
epic: CPE-810
qa-architecture: true
parent: CPE-1479
created: 2026-08-08
---
## Context — the follow-up to CPE-1479 (mouse-CDP fix, MERGED aed89022)
CPE-1479 fixed the root-cause harness breakage: `mouse.ts` was CDP-only and threw on WebKitWebDriver (Linux), so
every mouse spec failed instantly and the suite timed out. The W3C-Actions fallback **works** — confirmed on PR
#722's ubuntu run: **0 CDP-mouse errors**, `performActions` pointer sequences execute, and **9 specs PASS** that
couldn't run before. But the leg is still not green, for two reasons now UNMASKED by the fix (previously hidden
because specs died on the first click):

### 1. ~8 revealed spec failures (mostly seeded-state / environmental, Linux CI)
From PR #722 ubuntu log (run 31269532835, job 93133057356) — the PRIMARY failing assertions are content-presence,
not mouse actions:
- `context-menu.smoke.ts:46` — `expected a row for the seeded empty folder "CPE-1154-empty-folder"` (row absent).
- `drive-menu.smoke.ts:191` — `Home landing should show at least one drive tile (.qa-card with a drive-root path)`
  → null (no drive tile renders on the Linux CI Home landing).
- `home-item-menu` (CPE-1162) — `Home Folders tab should list the folder opened via --open=<tmpDir>` → null.
- `link-badge` (CPE-1208) — `expected the broken symlink row's badge to gain the .broken class` (symlink seeding?).
- `archive-browse` / `archive-password` (CPE-1182/1183) — `expected the item context menu on the marker row`.
- `macro-in-menu` (CPE-1191), `macro-param-prompt` — bound-macro submenu / param prompt.

Most of these fail on **missing seeded content** (`expected null/undefined to not equal null/undefined`), i.e. the
row/tile/folder isn't present on the Linux runner at all — a data-seeding / Home-landing-render / symlink-creation
environmental gap, NOT the mouse fallback. TRIAGE each: (a) genuinely environmental (Linux drive-tile enumeration,
symlink perms, `--open` seeding) → fix the seed/harness or gate the spec; (b) a real right-click-doesn't-open-menu
where the Actions `contextmenu` doesn't fire like CDP did → fix in `mouse.ts` (e.g. add a pointerMove+pause before
down, or dispatch a synthetic `contextmenu`); (c) a real app bug. Don't assume — read each failure.

### 2. 20-min job timeout too short now
`gui-smoke.yml` sets `timeout-minutes: 20` on both legs. Now that specs actually RUN (instead of failing instantly),
the ubuntu run only reached ~17 of ~39 specs before `The operation was canceled` at 20:15. Options: raise
`timeout-minutes` (simplest first step — needed just to SEE the full pass/fail set), add a per-test timeout so one
slow spec can't eat the budget, and/or shard the suite across matrix jobs (revisit CPE-1266's concurrency work).

## Suggested order (mind the ~20min CI feedback loop — don't blind-iterate)
1. **First**, raise `timeout-minutes` (e.g. 30–35) so a full run completes and reveals the TRUE failing set — you
   can't triage 8 failures when the run is cut off at 17 specs.
2. Triage the revealed failures locally where possible (read the specs' seed/setup; some are Windows-reproducible),
   classify env-vs-real, fix or appropriately gate/skip the genuinely-environmental Linux-only ones with a logged
   reason.
3. Confirm the leg goes green (or only-known-gated remain). Then flip the QA burndown row and name the pinning job.

## Acceptance
- gui-smoke (ubuntu) completes within its timeout and passes (or the only red is explicitly-gated env specs with a
  filed reason). Windows leg remains separately tracked under CPE-1048 (WebView2 DevToolsActivePort).

## Notes
Filed from the CPE-1479 workshift. This is the "restore the Visual Critic/UAT substrate to GREEN" work; CPE-1479
was the necessary first half (mouse). Epic CPE-810. Coordinate with the concurrent workshifts_* process.

## Work Log — Round 1 (2026-08-08, QA-infra Worker)

**Status: left in Doing.** This round is a high-confidence read-based triage + batch of harness fixes, not a
confirmed-green run — gui-smoke CI (ubuntu, tauri-driver + xvfb) is the only place that can actually verify these,
and several of the 8 are documented hypotheses rather than proven fixes. Expect a follow-up round once the
Foreman's CI loop reports back.

### 1. Timeout changes
- `.github/workflows/gui-smoke.yml`: `timeout-minutes: 20` → `35` on **both** the `gui-smoke` (windows) and
  `gui-smoke-linux` (ubuntu) jobs. Simplest lever to let a full ~39-spec run complete instead of being cut off at
  ~17, per the ticket's suggested order.
- `gui-smoke/wdio.conf.ts` `mochaOpts.timeout`: already `90_000` (90s/test) before this round — checked, not
  duplicated. That's generous per-test headroom well under the new 35-min job cap, so one hung spec still fails
  fast instead of eating the whole job. No change needed.

### 2. Per-spec triage (the 8 revealed failures)

| Spec (failing assertion) | Likely cause | Action |
|---|---|---|
| `context-menu.smoke.ts:46` — empty-folder row absent | The spec's `before` hook never waited for the initial `--open=<tmpDir>` navigation to render (only confirmed the state file exists) — every sibling spec that scans `.row`s first gates on `[aria-current="page"]`; this one didn't. A still-loading ~27-entry root listing reads as "row absent" instead of "not rendered yet". | **Fixed**: added the same crumb-ready gate, and turned the one-shot row scan into a `browser.waitUntil` poll (15s) instead of a single synchronous pass. |
| `drive-menu.smoke.ts:191` — no drive tile on Home | Backend `list_drives_impl` (src-tauri/src/lib.rs) is unconditional on non-Windows — it always pushes exactly one `{name:"File System", path:"/"}` entry, and there's a Rust unit test (`list_drives_returns_at_least_one_root`) pinning that. So the tile can't be genuinely *absent*; `goHome()` only waits for the outer `.qa-grid`/`.home` container to exist, not for the `{#each cards}` child tiles to have painted. | **Fixed (best-effort)**: wrapped the tile lookup in a `browser.waitUntil` poll (10s) instead of one synchronous read right after the container appears. **Uncertain** — if this doesn't clear it, the next round needs an actual Linux CI DOM dump (Home's rendered HTML) to see what's really there. |
| `home-item-menu.smoke.ts:132` — Folders tab row absent (CPE-1162) | Same class as drive-menu: `recordRecentFolder(tmpDir)` runs synchronously in `onMount` well before any spec runs, so the MRU should already contain it; `pointOfFirstRow()` was read once, synchronously, right after a fixed 150ms pause following the pill click. | **Fixed (best-effort)**: poll (10s) instead of one-shot read. **Uncertain** — if the true cause is MRU eviction/dedup or a path-normalization mismatch rather than a render race, this won't clear it; flagging for the next round to add direct logging of `recentFolders` state if it recurs. |
| `link-badge.smoke.ts:107` — broken-link badge never gains `.broken` | **Root-caused with evidence already in this codebase**: `transfer-panel.smoke.ts`'s own `danger-badge` test independently diagnosed the *identical* wait on a copy of this same assertion and documented that neither `LinkBadge.svelte`'s IntersectionObserver nor a CDP/Actions-driven hover reliably reaches its `on:mouseenter={load}` through this harness's shim — only a direct `dispatchEvent(new MouseEvent("mouseenter"))` deterministically kicks the lazy `linkStatus` fetch. `link-badge.smoke.ts` itself never had that stimulus, plus its target row can sit below the fold among ~27 root fixtures. Backend `link_status` (crates/server/src/links.rs) is correct and unit-tested — not an app bug. | **Fixed**: added `scrollIntoView` + the same `dispatchEvent("mouseenter")` kick already proven in transfer-panel.smoke.ts, to both the intact- and broken-link tests (the intact-link test's old assertion was also checking the pre-fetch default state, which passes vacuously — now it forces + waits for the real result too). |
| `archive-browse.smoke.ts` / `archive-password.smoke.ts:116` — item context menu doesn't open on a row | Neither spec's `pointOfRow` scrolled the target row into view before computing its `getBoundingClientRect()` point — the one thing every *other* row/point lookup in this suite that got the CPE-1253 fix already does. With ~27 root fixtures, and archive-password right-clicking two different rows across two tests (archive row, then marker row), a stale scroll offset from the first click is a plausible reason the second right-click's computed point misses the real element (CDP/W3C-Actions hit-tests the exact viewport point, not "wherever a human would expect the row"). | **Fixed (best-effort)**: added the same scroll-then-rect pattern to both specs' `pointOfRow`. **Uncertain** — this is the least-confident fix of the batch; if it doesn't clear it, the next round should check whether WebKitWebDriver's Actions API reliably synthesizes a native `contextmenu` from a `pointerDown/pointerUp(button:2)` pair at all (a known category of WebKitGTK WebDriver gap) — `mouse.ts`'s `rightClick` fallback may need its own fix, not just the harness's row-finding. |
| `macro-in-menu.smoke.ts` / `macro-param-prompt.smoke.ts` — Run-macro submenu / param prompt don't appear | **Root-caused**: both specs' `pointOfRowNamed`/`pointByText` used WebdriverIO's own `getLocation()`/`getSize()` ("get element rect") instead of `getBoundingClientRect()` inside `browser.execute()` — the one primitive `rightClick`'s CDP/W3C-Actions viewport-space coordinates are documented (mouse.ts) to match, and the pattern every *other* row/point lookup in this suite uses. They also never scrolled the target into view. Either gap alone can produce a point the driver's real hit-test doesn't land on — no `contextmenu` fires, `.ctx` never appears. | **Fixed**: converted both helpers in both specs to scroll-then-`getBoundingClientRect` via `element.execute`, matching the rest of the suite. This is the single highest-confidence fix in this round — it's the only genuinely inconsistent code path found (every sibling spec already uses the correct primitive). |

### 3. Verification done this round
- `cd gui-smoke && npm run typecheck` — **passes clean**.
- `cd gui-smoke && npm run test:unit` — **21/21 pass**.
- gui-smoke's own suite (`npm test`) requires tauri-driver + a native driver + (on Linux) xvfb — cannot run
  locally on this Windows box; CI is the only verification surface, as the ticket anticipated.

### 4. Honest expectation
`macro-in-menu`/`macro-param-prompt` and `link-badge` are backed by strong, specific evidence (a proven-working
fix already documented elsewhere in this repo, or a clear inconsistency against the suite's own established
pattern) — reasonably confident those clear on the next run. `context-menu` is a real, clear gap (missing
readiness gate) — confident. `drive-menu`, `home-item-menu`, and `archive-browse`/`archive-password` are
best-effort robustness fixes against a plausible-but-unconfirmed race/scroll theory — plan for at least one more
round after seeing the actual CI log, especially for `archive-browse`/`archive-password` where a WebKitWebDriver
Actions-API right-click limitation is a real possibility that would need a `mouse.ts` fix, not a spec fix. No app
`src/` bugs found — `list_drives_impl` and `link_status` were both read and are correct + already unit-tested.

## Work Log — Round 2 (2026-08-08, QA-infra Worker)

**Status: still in Doing (CI-only verification).** Round 1's per-spec fixes moved the ubuntu leg 9 → 15
passing. The **6 remaining failures are ONE root cause in the shared harness**, exactly as round 1's
lowest-confidence note predicted — confirmed from PR #724's ubuntu run (job 93143361534).

### Root cause (proven from the CI log, not hypothesised)
All 6 still-red specs are the ones that open a menu via an Actions `rightClick` and assert `.ctx`:
`context-menu` [0-5], `drive-menu` [0-9], `home-item-menu` [0-11], `macro-in-menu` [0-14],
`macro-param-prompt` [0-15], `archive-password` [0-1]. Every failure message is "expected the … context
menu (.ctx) to open". Cross-check: `new-link` [0-20] imports `rightClick` but never calls it → PASSED;
`archive-browse` [0-0] doesn't right-click → PASSED; the `element not interactable`/`…/click` warning was
`batch-media` [0-2], which **PASSED** (recovered WARN, not a failure); the `move target out of bounds when
running "actions"` line was WebdriverIO's OWN `scrollIntoView` (Actions wheel) WARN falling back to JS
scroll — non-fatal, not our `pointerMove`.

**The bug:** WebKitWebDriver's W3C Actions API delivers a real `pointerdown`/`mousedown` +
`pointerup`/`mouseup` for `button:2`, but — unlike CDP on Chromium — it does **not** synthesise the DOM
`contextmenu` event a secondary-button press produces. So the app's `on:contextmenu` handlers never ran
and no `.ctx` opened. (WebKitGTK WebDriver gap: the platform context-menu signal that dispatches
`contextmenu` isn't fired from synthetic WebDriver input.) The CDP fast-path (Windows/Edge) was never
affected — it emits a genuine native `contextmenu`.

### The fix — `gui-smoke/lib/mouse.ts` (`rightClick`, Actions fallback ONLY)
After the real `button:2` move→down→up, if no menu is open, dispatch a **hit-tested** synthetic
`contextmenu` at the same viewport pixel: `document.elementFromPoint(x,y)` resolves the topmost element
(real occlusion/z-order — NOT the CPE-1154 anti-pattern of firing at a hand-picked node), then a
bubbling/cancelable `MouseEvent("contextmenu", {button:2, buttons:2, clientX, clientY, …})` fires there.
The app handlers read exactly `clientX/clientY` and open the menu; most `stopPropagation`, and CPE-1160's
50 ms open-guard absorbs the rest, so the event never reaches the window dismisser → no open-then-close.
The `if (!contextMenuOpen())` guard means a future WebKit that DOES emit native `contextmenu` won't
double-fire. CDP path untouched; non-focus-stealing guarantee preserved (nothing new grabs input, and the
fallback runs only where CDP is absent).

### Timeout
Bumped `gui-smoke.yml` `timeout-minutes: 35 → 45` on **both** legs. Round-1's 35 still cut the ubuntu leg
off: build ~10 min, then a SEQUENTIAL (`maxInstances: 1`) run reached only ~21 of 39 specs — the
media-heavy back half (thumbnails / similar-images / metadata / snapshot-diff) is slow. The mouse fix
reclaims only ~1 min (the 6 failures stop burning their 10 s `waitForExist`), so 45 is the pragmatic
margin. **Real long-term fix: shard the suite across matrix jobs (revisit CPE-1266)** — not done this
round to keep the change focused on the harness fix.

### Verification
- `cd gui-smoke && npx tsc --noEmit` — clean.
- `cd gui-smoke && npm run test:unit` — 21/21 pass.
- gui-smoke itself needs tauri-driver + WebKitWebDriver + xvfb → CI (ubuntu) is the only surface; Foreman
  runs it. Confidence the 6 clear next run: **high** — the fix targets the exact, log-proven cause shared
  by all 6, and the synthetic event carries precisely what the handlers read. Residual risk: (a) if a
  target pixel is occluded/off-screen at right-click time `elementFromPoint` resolves wrong/no element —
  but round 1 already added `scrollIntoView` to those specs; (b) `home-item-menu`/`drive-menu`/`macro-*`
  also need the row/tile to actually be seeded/painted, a separate axis the synthetic event can't fix if
  content is genuinely absent — round 1's polling fixes cover the render-race version of that.
