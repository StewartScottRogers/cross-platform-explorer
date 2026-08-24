---
id: CPE-1827
title: the Trash titlebar cannot fit seven buttons and a title on one line at supported widths
type: bug
priority: Medium
status: Doing
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`.tv-titlebar` puts a title, a count/status slot, and seven buttons on one unwrapped line. Below
roughly 880 px that does not fit, and the failure is silent: `.tv-tools` overflows under
`overflow: visible` and `.tv-panel { overflow: hidden }` cuts the remainder off. Measured on `main`'s
markup (round-1 geometry in the CPE-1816 review, streaming state):

- **≤684 px** — `.tv-tools` overflows the panel by 106 px. "Delete selected permanently", "Empty
  Trash", refresh, Docs and the close **×** are all clipped or gone.
- The app's own floor is **600 × 400** (`src-tauri/src/lib.rs`, `.min_inner_size`), so this whole band
  is a size the app explicitly permits.

**The close button is the serious part.** There is no Escape handler in `TrashView` (verified), so once
**×** is clipped the only remaining way out is the ~2vw backdrop strip that `.tv-overlay`'s
`on:click|self` catches. A modal whose only exit is a sliver of backdrop is a trap.

CPE-1816 twice tried to solve a *related* symptom inside its own scope and each attempt moved the
damage: a `min-width: 0` on the title let the title (and the loading caveat) collapse to nothing, and a
`min-width: 34ch` floor pushed the toolbar off the edge at 700–880 px — a band that had previously been
fine. Both were reverted. The real cause is density, not a `min-width` value.

## Acceptance criteria

- [x] Pick and record ONE approach. The Visual Critic's recommendation is an **overflow "…" menu**,
      because it is the only option of the three that keeps the close button on the first line at every
      supported width. The alternatives considered were icon-only buttons below a breakpoint, and
      letting the bar wrap onto a second row.
- [x] At every width from 600 px up, in all three listing states (streaming, complete, degraded) and with
      and without a selection: the close **×** is present and hit-testable, and no control is silently
      clipped. — verified by CSS/layout construction + `npm run check`/unit tests; NOT verified by a
      real-browser screenshot sweep (see Work Log — the gui-smoke attempt hit an environment blocker
      unrelated to this fix).
- [x] An **Escape** handler closes the view, so keyboard users are never dependent on a visible ×. Check
      the repo's other overlays first and match whatever convention they already use.
- [ ] Verify in all 12 locales — several are materially wider than en-US, and the CPE-1816 measurements
      showed Russian is the worst case for the status slot. **NOT verified live** — only English was
      exercised; see Work Log for the reasoning on why the fix should hold regardless (`.tv-tools` no
      longer varies by locale).
- [ ] Whatever ships is pinned by the Trash gui-smoke spec from CPE-1822 (hit-test the × at a narrow
      width), not by a jsdom structural assertion — jsdom does not compute layout, which is exactly why
      three rounds of this went unguarded. **Attempted, not landed green** — see Work Log; the new spec
      is checked in but currently listed in `known-failing.json` pending CPE-1822's own investigation.

## Notes

Filed from the CPE-1816 Visual Critic's finding 5, which it raised in round 1 and re-measured in every
round after. The ≤684 px overflow is **pre-existing**, not caused by CPE-1816. Related: CPE-1822 (no
gui-smoke coverage of the Trash view at all) is a prerequisite for pinning any of this properly.

## Work Log

### 2026-08-23 — Sprint worker: overflow-menu titlebar fix + Escape handler; gui-smoke attempted, blocked by a real-Recycle-Bin environment issue (not this fix)

**Approach.** Collapsed every titlebar action except Close into ONE "…" overflow menu (Select
all/Restore selected/Empty selected/Empty Trash/Refresh/Docs), matching the Visual Critic's
recommendation. `.tv-tools` is now exactly two fixed-size controls — the overflow trigger and × —
regardless of listing state, selection, or locale, so the close button's reachability no longer depends
on how many action buttons happen to fit. `.tv-title` (icon + "Trash" + the count/status span) got
`flex: 1 1 auto; min-width: 0; flex-wrap: wrap` (replacing the old `white-space: nowrap; overflow:
hidden`, which is what silently clipped it in every prior CPE-1816 attempt) — if the title text ever
runs out of room it wraps onto its own second line inside the titlebar instead of being cut off; it
never competes with the toolbar for space since the toolbar's own width is now fixed.

The overflow menu itself uses `position: fixed` + a `clampToAnchor` action (same clamp-into-viewport
shape as `AgentMenu.svelte`/`ContextMenu.svelte`'s `.ctx`), not the global `.menu-wrap`/`.menu`
(`position: absolute`) CommandBar/MenuBar use — deliberately, because `.tv-panel` is `overflow: hidden`
and an absolutely-positioned menu would still be a normal descendant for clipping purposes: at the
600×400 floor the menu's full height (up to ~210px with every row shown) can exceed the panel's own
~368px height, so a `position: absolute` menu risked being clipped by the exact same rule this ticket
exists to fix, just one level in. `position: fixed` escapes that (no transformed/filtered ancestor
between it and the viewport), so it's clamped against the real window bounds instead.

Added the missing Escape handler (`<svelte:window on:keydown>`, dispatching `close` — same convention as
`DiskSpaceView.svelte`/`ArchiveSafetyDialog.svelte`/`RepairLinkDialog.svelte`), with two additions of
this component's own: Escape closes the overflow menu FIRST if it's open (matching MENUS.md's own
"Escape closes [the menu]" rule for every other dropdown), and is a no-op while the nested `ConfirmDialog`
is open (it already owns Escape via its own listener — without the guard, the same keypress would also
close the whole Trash view out from under the confirm).

**Widths verified, and how.** `npm run check` (svelte-check, 0 errors) plus the full `TrashView.test.ts`
(28 cases, updated so every action click now opens the overflow menu first) and
`TrashView.bidiSpoof.test.ts` pass — these confirm the MENU'S OWN logic (open/close, disabled states,
Escape, selection wiring) but jsdom does not compute real layout, so they cannot prove the geometry
claim by themselves; that was always this ticket's own point (see the ticket's last AC). The geometry
argument for "no clipping at 600px+" is by construction, not measurement: the toolbar's own width is now
fixed and small (~65px: a ~29px icon button + 8px gap + ~28px ×) regardless of state, so the ~535px+
remaining at a 600px window (minus ~28px panel padding) goes entirely to the title, which now wraps
rather than clips. I could not get a real-browser screenshot sweep to confirm this pixel-for-pixel (see
below) — flagging that plainly rather than implying it was checked.

**gui-smoke: attempted locally, blocked; the REAL CI failure has a confirmed, different root cause.**
Built the real release binary (`npm run build && npm run tauri build -- --no-bundle`, ~9 min) and wrote
`gui-smoke/specs/trash-titlebar.smoke.ts` (opens Trash from the Sidebar with one real seeded-and-deleted
entry selected, sweeps the window through 880px/700px/600×400, hit-tests `.tv-x` and the overflow menu
at each stop, snaps screenshots, then proves Escape closes the view). Locally (Windows dev machine) every
attempt hung on ordinary clicks with no WebDriver result and "Timed out receiving message from renderer"
— a real, separate issue on that one machine (`~/.cargo/bin`'s `msedgedriver` v150 vs. the machine's Edge
v151), reproduced independently the same day on the repo's own stock `open-dir.smoke.ts`. **That
diagnosis does NOT apply to the failure that actually gates this PR**: CI's `windows-latest` GUI-smoke
job (the only leg that touches `msedgedriver`) shows `skipping` on every run of this PR — it never ran.
The leg that ran and failed is `ubuntu-latest` / WebKitWebDriver, a completely different driver stack.

Pulled the real job log (`gh api repos/.../actions/jobs/97288403795/logs`, run 32676997154 shard 4) and
its screenshot artifact to find the ACTUAL cause: `element click intercepted` clicking the Sidebar's
Open-Trash row. The button's own rect at click time was `{x:20, y:644, width:193, height:30}` in the
test's 1000×700 window (`getWindowRect` confirmed in the log). Cross-referencing
`DropStackPanel.svelte`: its `.drop-stack-handle` toggle button is `position: fixed; left: 14px; bottom:
14px; z-index: 149` and, critically, is rendered **unconditionally** — only the *expanded* panel sits
behind the `{#if open}`, the handle itself is always on screen. At a 700px window height that places it
at y≈658–686, x≈14–125 — a real, measured rectangular overlap with the Open-Trash row's rect, not a
coincidence. **Root cause confirmed**: `Sidebar.svelte`'s Trash section (`order: 900`, near the bottom of
a fully-expanded sidebar) can render its rows underneath `DropStackPanel`'s always-floating handle
whenever the sidebar is tall enough to push them into that band — a pre-existing bug in two files this
ticket never touches, not a defect in the titlebar fix. Left the spec checked in (it correctly exercises
the real bug — its 4 cascading cases are exactly what a real user hitting this overlap would see) but
listed its 4 cases in `gui-smoke/known-failing.json` under CPE-1822 with the confirmed cause spelled out
(corrected from an earlier, wrong pass at this entry that cited the Windows-only driver mismatch — that
was my mistake, caught in review). This does NOT auto-clear via the driver-version fix; it needs its own
fix to `Sidebar.svelte`/`DropStackPanel.svelte` (e.g. reserving bottom padding in the sidebar's scroll
region, or giving the floating handle a narrower hit target) — flagged for the Foreman to file as its own
ticket rather than patched here under time pressure on two shared, unrelated files.

**Screenshots for the Visual Critic: none usable captured.** `snap()`/`snapFailure()` write to the SAME
filename per spec (`trash-titlebar-fail.png`), so with all 4 of this spec's cases failing in sequence,
each `afterEach` overwrote the previous one — the artifact that survived is case 4's ("Escape closes the
view"), which is a 600×400 shot of the still-unopened file listing (case 4 runs after case 3's own
`setWindowSize(600, 400)`), not a shot of the actual Trash titlebar. There is nothing usable to hand the
Visual Critic from either the local or the CI attempt. Saying this plainly rather than implying otherwise, per
the sprint DoD.

**Assumption logged:** the overflow menu's item ordering/wording reuses the exact labels the old always-
visible toolbar used (`trash.selectAll`/`deselectAll`/`restoreSelected`/`emptySelected`/`emptyAll`/
`refresh`), plus two new i18n keys (`trash.moreActions` "More actions", `trash.docs` "Docs" — translated
into all 12 locale blocks, `trash.docs` and `trash.moreActions` weren't string-reviewed by a native
speaker, machine-translated to match the existing catalog's quality bar for the other 11 non-English
locales). `HelpButton.svelte` is no longer used by `TrashView` (its dispatch is now inlined so the Docs
row can be a proper `.row` menu item rather than the chip-styled button) — did not touch `HelpButton`
itself, which is still used elsewhere.

Reviewer follow-up (2026-08-23): added `node.focus()` at the end of `clampToAnchor` (TrashView.svelte) after placement, matching `AgentMenu.svelte`/`ContextMenu.svelte`'s own `onMount` — MENUS.md requires the container take focus on open so Escape/arrow-key handling works; the first version of this file's own copy missed that one line despite citing both precedents. Also corrected the `known-failing.json` reasons for `trash-titlebar.smoke.ts`'s 4 cases: the CI failure they're exempting is a CONFIRMED `DropStackPanel.svelte`/`Sidebar.svelte` click-interception overlap (see the gui-smoke paragraph above), not the Windows-only driver-version mismatch an earlier pass at those reasons wrongly cited.

Also fixed two now-stale line-number pins this change shifted: `bidiEscape.guard.test.ts`'s
`TrashView.svelte` REGISTRY entry (recomputed via the real `findUnsafeRenderLines` scan) and
`mojibakeGuard.test.ts`'s Portuguese "NÃO" ALLOWLIST entry (5366 → 5379, from the two new i18n keys
added ahead of it in 5 locale blocks).
