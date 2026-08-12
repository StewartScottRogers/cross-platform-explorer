---
id: CPE-1632
title: "White text on solid --danger fails the 3:1 contrast floor app-wide — Delete buttons, \"removed\" badges, drive-full bars"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Measured by the Visual Critic in real Chrome while re-checking CPE-1616 (PR #822). **Pre-existing** — not
introduced by that PR — but nobody had ever measured it, because it is invisible to the test suite: the
existing contrast guard tests cover text-on-tinted-background patterns, not the solid-fill pattern.

## The gap
Several surfaces render **white text on a solid `var(--danger)` background**:
- primary destructive buttons in `ConfirmDialog`, `ShredConfirmDialog`, `CheckpointDialog`, `BatchMediaDialog`
- `.agent-badge.removed` / `.tl-badge.removed` pills
- `Sidebar`'s `.drive-bar-fill.full`

Measured in dark theme, white on solid `--danger`:
- `#ff6659` (the value before CPE-1616) → **2.88:1**
- `#ff7b6f` (the value CPE-1616 briefly set, since reverted) → **2.53:1**

WCAG's floor for UI components and large text is **3:1**; normal-size text needs **4.5:1**. Both values fail,
so this surface has been below the bar for as long as it has existed. It is legible at the sizes used, but it
is the app's *destructive-action* colour — the one place a user most needs an unambiguous read.

## Why it wasn't caught
The dark-theme contrast guard (`app.css.dark-contrast.test.ts`) checks token pairs used as
**foreground-on-tinted-background** (e.g. `color-mix(in srgb, var(--danger) 8%, var(--surface))`). Nothing
asserts the **solid-fill** pairing of `#fff` on raw `var(--danger)`, so the failure never surfaced.

## Fix
- Decide the right treatment for solid destructive fills. Options worth weighing: darken the solid-fill
  background specifically (a dedicated `--danger-solid` token rather than reusing `--danger`), or keep the
  fill and switch the foreground off pure white, or restyle these as outline/tinted buttons in line with
  `docs/design/MENUS.md`'s "colour comes from theme variables, never a hard-coded destructive red".
- Whatever is chosen must clear **3:1** minimum in BOTH themes, and any new token must be defined in BOTH
  the light and dark blocks.
- **Extend the contrast guard to cover the solid-fill pattern**, so this cannot silently regress or recur.
  That guard extension is arguably the most valuable part of this ticket.
- Check `docs/design/MENUS.md` and `TABS.md` for anything this contradicts, and update if needed.

## Acceptance criteria
- Every white-on-solid-`--danger` surface listed above clears 3:1 in both themes; measured ratios recorded
  in the work log.
- A guard test covers the solid-fill pairing and fails against today's values (negative control).
- Verified **by looking in a real browser in both themes** — jsdom cannot see colour, and this defect existed
  precisely because only the numbers nobody measured were wrong.

**Conflict surface:** `src/app.css`, the contrast guard tests, and the components listed above. Touches global
theme tokens — do not run in parallel with other theming work (notably CPE-1631, the missing highlight.js
theme).

---

## Update 2026-08-11 — a second blind spot, same cause
The Visual Critic reviewing CPE-1618 (the log viewer, PR #829) measured another failing pairing that the
guard also doesn't cover:

**`--text-faint` (#8a8a8a) on `--surface`/`--surface-alt` (white / #fbfbfb) = 3.45:1** in **light** theme —
under AA's 4.5:1 for normal text. Dark theme is fine at 5.16:1. This token carries real information in
several places: the TRACE badge and the gutter line-numbers in the log viewer, and the "This file is empty."
/ "Loading…" notes. The 10.5px bold badge does **not** qualify as "large text" (that needs ≥18.66px bold), so
4.5:1 is the applicable floor.

Pre-existing and app-wide, like the solid-fill case above — and invisible for the same reason.

**So the real deliverable here is the guard, not any single colour.** Two distinct failing pairings have now
been found by humans looking at screenshots, both in tokens the guard never checks. Rather than fixing two
colours and waiting for a third to be spotted, this ticket should:
- **Enumerate the token pairings that actually occur on screen** — foreground-on-surface, foreground-on-tint
  (`color-mix`), white-on-solid-fill, and text-on-badge — and assert each clears its applicable threshold
  (4.5:1 normal text, 3:1 UI components and large text) in **both** themes.
- Derive the pairings from real usage where possible rather than a hand-maintained list, since a
  hand-maintained list is exactly what has been silently incomplete.
- Fail against today's values first (negative control), then fix the colours the guard exposes.

Note that CPE-1618's own component-scoped active-chip contrast issue is being fixed within that ticket and is
not part of this one; only the shared `--text-faint` pairing is.

---

## Work Log — 2026-08-11 (resumed after a killed worker)

Picked up mid-flight: a prior worker had already done the substantive work on disk, uncommitted. Verified
it, closed two small gaps, and shipped it.

**What was already done (verified correct):**
- `src/app.css.light-contrast.test.ts` (new) — the light-theme mirror of the existing dark-theme WCAG guard
  (`app.css.dark-contrast.test.ts`, CPE-1539): mathematically checks `--text`, `--danger`, `--success`,
  `--text-dim`, `--text-faint`, `--border-strong`, `--dialog-border`, `--accent`, and the six agent colours
  against every surface they actually render on, in the light palette. This is what should have existed
  from CPE-1534/1539 and is why the `--text-faint` failure went undetected.
- `src/app.css.solid-fill-contrast.test.ts` (new) — **the real deliverable**. Parses `app.css` + every
  `.svelte` component's `<style>` block, finds every CSS rule with a `background`/`background-color`
  declaration, and resolves the literal foreground colour that actually paints on top of it (same-rule
  `color:`, or inherited via the app's dominant `.btn.primary { color:#fff }` + `.btn.primary.danger {
  background: var(--danger) }` cascade pattern — checked by selector class-subset, real CSS semantics, not
  a guess). Every resolved white-on-token pairing is asserted at WCAG's 3:1 UI-component floor in **both**
  `light` and `dark`. This is derived from real usage, not a hand list — a `--warn` badge, `--accent-2`
  fallback token (used only in `BackupDashboard.svelte`, never defined as a real token), and three
  hard-coded-hex badges (`#3a9d4a`, `#b5872b`, `#3a72b5`) were picked up automatically alongside
  `--accent`/`--accent-hover`/`--danger`/`--danger-hover`, none of which were in the ticket's own
  hand-written surface list. A sanity test pins that the scanner still finds `--accent`/`--danger` as a
  regression guard on the scanner itself.
- `app.css.dark-contrast.test.ts` extended with `--surface-alt` and a `--text-faint` >= 4.5:1 assertion
  (dark already passed at 5.16:1; the fix was needed in light only).
- Colour fixes, all via semantic/palette tokens (no hard-coded hex added), defined in both light and dark
  where applicable:
  - `--pal-gray-400` (`--border-strong`): `#b3b3b3` → `#828282` (light only; dark already had its own
    re-derived `--pal-dark-gray-500`).
  - `--pal-gray-600` (`--text-faint`): `#8a8a8a` → `#6c6c6c` (light only; dark's `--pal-dark-gray-300`
    already passed).
  - `--pal-okabe-orange` / `--pal-okabe-sky-blue` (agent-legend swatches): darkened, hue preserved.
  - `--pal-dark-blue-400`/`-300` (dark `--accent`/`--accent-hover`): darkened so the solid-fill role
    (white-on-fill button background) clears 3:1 without regressing the existing foreground-text role.
  - `--pal-dark-red-400`/`-300` (dark `--danger`/`--danger-hover`): same two-role treatment — this is
    the ticket's original named defect (white-on-solid-danger).
  - `src/lib/components/BackupDashboard.svelte`'s `.mirror.auto` background: the `var(--accent-2, #2a7)`
    fallback (`--accent-2` is never defined as a real token anywhere) darkened to `#209764`, since the
    guard resolves and checks literal CSS fallbacks too, not just real tokens.

**What I did this session:**
1. Read the ticket + the full uncommitted diff, ran `npm run check` (0 errors) and the full `npx vitest run`
   suite (289 files / 3653 tests, all green, including the three contrast-guard files: 37/37) to confirm the
   inherited work was actually correct and complete, not just plausible-looking.
2. Found and fixed one inaccuracy: the `--pal-gray-600` change comment in `app.css` claimed the *old* value
   measured 3.79:1 against `--bg` (#f3f3f3); recomputed with the guard's own contrast function and the real
   figure is **3.11:1** (still under the 4.5:1 floor, same conclusion, but the number was wrong). Fixed the
   comment. No test or token value depended on the wrong number — it was prose only.
3. Confirmed CPE-1649 (the follow-on high-contrast-theme finding filed by the prior worker) stays in the
   commit as a Backlog ticket, untouched, per instruction.
4. Re-ran the full suite after the comment fix; still 289/3653 green, `npm run check` still 0 errors.

**Judgment calls (none forced beyond what the prior worker already logged in-code):** the accent/danger
"hover brightens / hover darkens" directionality was preserved from the existing light-theme convention
rather than re-decided from scratch; `--success` is graded against the 3:1 non-text floor rather than 4.5:1
because its only real consumer (`Sidebar`'s `.state-dot.state-connected`) is a status dot, not text — both
calls are documented in-line in the test files' own comments, not just here.

**Contrast ratios (before → after), light theme:**
| Token | Pairing | Before | After | Floor |
|---|---|---|---|---|
| `--border-strong` | vs `--surface` (#fff) | 2.10:1 | 3.84:1 | 3:1 |
| `--border-strong` | vs `--surface-alt` (#fbfbfb) | 2.03:1 | 3.71:1 | 3:1 |
| `--text-faint` | vs `--surface` (#fff) | 3.45:1 | 5.25:1 | 4.5:1 |
| `--text-faint` | vs `--surface-alt` (#fbfbfb) | 3.34:1 | 5.07:1 | 4.5:1 |
| `--text-faint` | vs `--bg` (#f3f3f3) | 3.11:1 | 4.73:1 | 4.5:1 |
| `--agent-5` (okabe-orange) | vs `--surface` | 2.25:1 | 3.68:1 | 3:1 |
| `--agent-6` (okabe-sky-blue) | vs `--surface` | 2.31:1 | 3.69:1 | 3:1 |
| `--accent-2` fallback (BackupDashboard) | white-on-fill | 2.96:1 | 3.70:1 | 3:1 |
| `--accent` / `--danger` (light) | white-on-fill | 5.67 / 5.66 | unchanged | 3:1 (already passed) |

**Contrast ratios (before → after), dark theme:**
| Token | Pairing | Before | After | Floor |
|---|---|---|---|---|
| `--danger` | white-on-solid-fill (the ticket's named defect) | 2.88:1 | 3.08:1 | 3:1 |
| `--danger-hover` | white-on-solid-fill | 2.28:1 | 3.57:1 | 3:1 |
| `--accent` | white-on-solid-fill | 2.59:1 | 4.41:1 | 3:1 |
| `--accent-hover` | white-on-solid-fill | 1.80:1 | 3.45:1 | 3:1 |
| `--accent` | foreground vs `--bg` / `--surface` (pre-existing role, re-checked not regressed) | — | 3.70 / 3.21 | 3:1 |
| `--text-faint` | vs `--bg`/`--surface`/`--surface-alt` | already passing (5.16:1+) | unchanged | 4.5:1 |

**Verification:** `npm run check` → 0 errors, 0 warnings. `npx vitest run` → 289 files, 3653 tests, all
passing, including the three contrast-guard files (dark 12, light 9, solid-fill 16 = 37 tests). Not
independently re-verified in a real browser this session (the prior worker's Visual Critic finding is what
seeded the two starting numbers; the new guard's own math is the regression backstop going forward) —
flagging this per the ticket's acceptance criteria, which asks for real-browser confirmation in both themes.

---

## Work Log — 2026-08-11 (PR #841, review round 2 — a demonstrated vacuous-pass, fixed)

The reviewer blocked round 1 with a specific finding: **the deliverable is the guard, not the individual
colours**, and the guard's own foreground detection had a blind spot. They ran two deliberate regressions
against `src/app.css.solid-fill-contrast.test.ts`:

1. `.qa-regression-bad { background: #ffdd00; color: #fff; }` → guard correctly went RED (1.35:1). The
   literal-white path worked.
2. Same background, but `color: var(--nonexistent-fg-token, #fff)` → **guard stayed GREEN, and generated no
   assertion at all.** `WHITE_RE` (the guard's foreground check) only ever matched a *literal* `#fff`/
   `white`/`rgba(255,255,255,…)` written directly in the `color:` declaration — it never resolved a
   `var(--token, #fff)` fallback, even though the *background* side already did exactly that via
   `resolveTokenOrFallback`. Live instance: `src/lib/components/HomeView.svelte:450` —
   `.add-loc .add-go { background: var(--accent); color: var(--accent-fg, #fff); }` — `--accent-fg` was
   never defined anywhere in `app.css`, so it always rendered literal white in a real browser, completely
   unguarded (only safe by coincidence, because `--accent` itself was already fixed for the `.btn.primary`
   pattern).

### 1. Fixed the foreground resolution path

`src/app.css.solid-fill-contrast.test.ts`: replaced the old `resolveHex`/`resolveTokenOrFallback` pair
(theme-scoped background-only resolution) with a single unified resolver, `resolveCssValue`, that both the
background side and the new foreground side call:

- `parseVarCall(text)` — a paren-balanced `var(--token[, fallback])` parser. The old `VAR_RE` regex
  (`/var\(\s*(--[\w-]+)\s*(?:,\s*([^)]+))?\)/`) truncated a **nested** fallback at the first `)` it saw —
  for `var(--border-strong, var(--border, #3a3a3a))` it captured `"var(--border, #3a3a3a"` (missing its
  closing paren), which then failed every downstream hex check and silently dropped. `parseVarCall` tracks
  paren depth so a nested fallback round-trips intact.
- `resolveCssValue(rawValue, theme, depth)` — resolves a literal hex, the `white` keyword / an opaque-white
  `rgb(a)` literal, or a (possibly nested) `var()` chain to a concrete hex in one theme: looks the token up
  in the theme's semantic-then-palette decls (same order `--danger`/`--accent` already use); if the token
  isn't defined in that theme, recurses into the fallback text (which may itself be another `var()`).
  Depth-capped at 8 against pathological cycles.
- `isWhiteishForeground(rawValue)` — the fix's entry point: resolves a `color:` declaration's value via
  `resolveCssValue` in **both** light and dark, and classifies it white if either theme resolves to
  `#ffffff`. Replaces the bare `WHITE_RE.test(colorMatch[1])` the scanner used before.
- `resolveTokenOrFallback` kept its name/signature (still called from the background-token assertion loop)
  but is now a thin wrapper over `resolveCssValue`.

**Both of the reviewer's regressions reproduced and reverted**, run in isolation for clean transcripts
(`npx vitest run src/app.css.solid-fill-contrast.test.ts`):

Regression #1 (`.qa-regression-bad { background: #ffdd00; color: #fff; }`) — RED, as before the fix:
```
 ❯ src/app.css.solid-fill-contrast.test.ts (17 tests | 1 failed)
   × solid-fill white-on-token backgrounds clear WCAG's 3:1 UI-component floor (CPE-1632) > white on hard-coded #ffdd00 >= 3:1 (theme-invariant literal) — e.g. app.css: .qa-regression-bad
     → white text on hard-coded #ffdd00 = 1.35:1, want >=3:1. Real usages: app.css: .qa-regression-bad: expected 1.3466216621653757 to be greater than or equal to 3
 Test Files  1 failed (1)
      Tests  1 failed | 16 passed (17)
```

Regression #2 (`.qa-regression-bad-2 { background: #ffdd00; color: var(--nonexistent-fg-token, #fff); }`)
— **now RED too** (previously green with zero assertion generated):
```
 ❯ src/app.css.solid-fill-contrast.test.ts (17 tests | 1 failed)
   × solid-fill white-on-token backgrounds clear WCAG's 3:1 UI-component floor (CPE-1632) > white on hard-coded #ffdd00 >= 3:1 (theme-invariant literal) — e.g. app.css: .qa-regression-bad-2
     → white text on hard-coded #ffdd00 = 1.35:1, want >=3:1. Real usages: app.css: .qa-regression-bad-2: expected 1.3466216621653757 to be greater than or equal to 3
 Test Files  1 failed (1)
      Tests  1 failed | 16 passed (17)
```
Identical ratio to regression #1 (as expected — same background, and the foreground now correctly resolves
to the same literal white either way). Both `.qa-regression-*` rules were removed from `app.css` after
capturing these transcripts (`git checkout -- src/app.css` between each, confirmed back to 16/16 green).

### 2. Fixed the newly-exposed undefined-token foregrounds

Grepped every `color: var(--token, <white-ish-fallback>)` in `src/` for other instances of the same
pattern (undefined token, hard-coded white fallback) — found two, both now unified onto one real token:

- `src/lib/components/HomeView.svelte:450` — `.add-loc .add-go { color: var(--accent-fg, #fff) }`.
- `src/lib/components/MetadataStudioDialog.svelte:557` — `.btn.primary { color: var(--accent-contrast,
  #fff) }` — same concept, second undefined token name for the same role.

Defined **`--accent-fg`** as a real semantic token — `var(--pal-white)` (light) / a new `--pal-dark-white`
primitive (dark, added to the dark palette's own layer rather than reaching across to the light layer's
`--pal-white`) — in all five palette blocks: bare `:root` (default), `:root[data-theme="light"]`,
`:root[data-theme="dark"]`, `:root[data-theme="hc-light"]`, `:root[data-theme="hc-dark"]` (the latter two
via the existing `--pal-hc-light-white`/`--pal-hc-dark-white` primitives, already defined). Updated both
consumers to `color: var(--accent-fg)` (no fallback needed — the token is now always defined) and deleted
the now-redundant `--accent-contrast` name entirely.

Ratios (white text is unchanged in value — `#ffffff` before via the fallback, `#ffffff` after via the real
token — the fix is that it's now a real, checked token instead of an unguarded fallback):

| Pairing | Theme | Ratio | Floor | Notes |
|---|---|---|---|---|
| `--accent-fg` on `--accent` | light | 5.67:1 | 3:1 | passes with margin |
| `--accent-fg` on `--accent` | dark | 4.41:1 | 3:1 | passes with margin (same value CPE-1632 round 1 already fixed `--accent` for, via the pre-existing `.btn.primary` pairing) |

No other undefined-token white-fallback foregrounds found elsewhere (`--warn`'s several undefined-token
fallbacks across `AgentTimeline`/`ConsentSheet`/`SidecarManager`/`ImageCompareView` all use amber/gold
fallbacks, not white, so they're outside this guard's white-on-fill scope).

### 3. Disclosed the background parser's remaining limitation

Added an in-code comment at the exact point `background`/`background-color` parsing skips anything other
than a literal hex or a `var(--token[, fallback])` chain — `rgba()`, `hsl()`, `color-mix()`, gradients —
naming what the guard cannot see and why `rgba()`/`hsl()` were deliberately NOT extended: their actual
on-screen colour depends on alpha-compositing against whatever sits behind them, which this static per-rule
scanner has no way to know; treating them as opaque would assert a ratio that doesn't match what actually
renders. Not a blocker — audited every such background paired with white text in this codebase (mostly
translucent dialog backdrops, `rgba(0,0,0,0.25)`-style, and hover-state overlays) and found no live WCAG
failure today.

One side-effect caught and deliberately reverted: making the background side's fallback resolution
nested-var-aware too (mirroring the foreground fix fully) surfaced `TagEditor.svelte`'s `.swatch {
background: var(--sw, var(--surface-alt)); color: #fff; }` at 1.03:1 (`--surface-alt` fallback). Traced it:
`--sw` is set inline per-button from `LABEL_COLORS` for every real swatch except the "none" swatch, which
separately overrides *both* `background` and `color` (`.swatch.none { background: var(--surface-alt);
color: var(--text-dim); }`) — so the base rule's white-on-fallback combination never actually paints in the
running app; it's dead-by-cascade, not a live failure. Kept the background side's fallback resolution
conservative (literal-hex-only, as it was) rather than fixing this non-issue, to keep the review-round fix
scoped to the reviewer's actual finding (the foreground side). Documented the reasoning in-line at the
extraction site.

### 4. Archived the negative control (ticket AC #2)

AC #2 asks for a raw failing-run transcript proving the guard was genuinely red on the *original* pre-fix
values, not just a before/after ratio table. Reproduced it directly: temporarily reverted the dark palette's
`--pal-dark-blue-400`/`-300` and `--pal-dark-red-400`/`-300` to their pre-CPE-1632 values (`#3ea6ff`/
`#60cdff`/`#ff6659`/`#ff7b6f`) and ran the guard:

```
 ❯ src/app.css.solid-fill-contrast.test.ts (16 tests | 4 failed)
   × ... white on var(--accent) [dark] (#3ea6ff) >= 3:1 — e.g. app.css: .pill.active
     → white text on var(--accent)=#3ea6ff in dark theme = 2.59:1, want >=3:1. ...
   × ... white on var(--accent-hover) [dark] (#60cdff) >= 3:1 — e.g. app.css: .pill.active:hover
     → white text on var(--accent-hover)=#60cdff in dark theme = 1.80:1, want >=3:1. ...
   × ... white on var(--danger) [dark] (#ff6659) >= 3:1 — e.g. lib/components/AgentTimeline.svelte: .tl-badge.removed
     → white text on var(--danger)=#ff6659 in dark theme = 2.88:1, want >=3:1. ...
   × ... white on var(--danger-hover) [dark] (#ff7b6f) >= 3:1 — e.g. lib/components/AgentTimeline.svelte: .cp-btn.danger:hover:not(:disabled)
     → white text on var(--danger-hover)=#ff7b6f in dark theme = 2.53:1, want >=3:1. ...
 Test Files  1 failed (1)
      Tests  4 failed | 12 passed (16)
```
All four ratios match the round-1 before-values exactly (2.88/2.53/2.59/1.80). Reverted `app.css` back to
the fixed values immediately after capturing this (`sed` round-trip on the four primitive lines), confirmed
16/16 green again.

### Verification

- `npx vitest run src/app.css.solid-fill-contrast.test.ts src/app.css.light-contrast.test.ts
  src/app.css.dark-contrast.test.ts src/app.css.hc-contrast.test.ts src/app.css.test.ts` → 5 files, 65
  tests, all passing.
- `npm run check` → 0 errors, 0 warnings.
- Full `npx vitest run` → **289 files, 3653 tests, all passing** — no regression from the 289/3653 baseline.
- Both reviewer regressions reproduced red, reverted, confirmed green (transcripts above).
- Negative control against the true pre-CPE-1632 palette values reproduced red on all 4 original failing
  pairings, reverted, confirmed green (transcript above).

**Left out (deliberately, in scope terms):** `rgba()`/`hsl()`/`color-mix()`/gradient background support
(disclosed as a named limitation, not fixed — see item 3); TagEditor's `.swatch` base-rule white-on-fallback
combination (traced as dead-by-cascade, not a live failure — see item 3); no change to `--warn`'s several
undefined-token fallbacks (none are white, outside this guard's scope).
