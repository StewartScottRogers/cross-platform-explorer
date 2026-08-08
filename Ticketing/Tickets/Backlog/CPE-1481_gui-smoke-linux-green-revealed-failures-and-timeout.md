---
id: CPE-1481
title: "gui-smoke Linux leg: get it fully green — 8 revealed environmental spec failures + 20min job timeout too short now that mouse works"
type: Bug
status: Backlog
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
