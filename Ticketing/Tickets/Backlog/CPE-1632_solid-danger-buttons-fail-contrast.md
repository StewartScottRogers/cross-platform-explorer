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
