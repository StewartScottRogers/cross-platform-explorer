---
id: CPE-1821
title: three more undefined theme tokens mean their fallback hex always wins, in every theme
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1810 fixed `--warn`. The same shape survives at three more tokens, and it is worse than a tidy-up
item: because the token is defined **nowhere** in `src/app.css`, the fallback hex is not a fallback at
all — it is the only value that ever applies, in **every** theme, including dark and both
high-contrast themes.

Verified on `main` (2026-08-20): `grep -c -- '--text-muted:' src/app.css` → `0`, same for `--accent-2:`
and `--bg-dim:`. 22 call sites across 5 components:

| Token | Sites | Fallback | Files |
|-------|-------|----------|-------|
| `--text-muted` | 20 | `#9a9a9a` | `AgentTimeline.svelte` (17), `ConsultedFiles.svelte` (2), `FileList.svelte` (1) |
| `--accent-2` | 1 | `#209764` | `BackupDashboard.svelte` |
| `--bg-dim` | 1 | `#0f0f0f` | `SidecarManager.svelte` |

## Why it matters

`--bg-dim`'s fallback is `#0f0f0f` — near-black. SidecarManager renders that background in the **light**
theme too, because nothing can override it. `--text-muted`'s `#9a9a9a` is a mid grey chosen for a dark
surface; on the light theme's near-white background it is the low-contrast end of the scale and never
gets checked, because the WCAG contrast guard test only reasons about tokens that exist. The
high-contrast themes exist precisely to raise these ratios and this shape defeats them silently.

This is the CPE-1810 defect class, not a cosmetic follow-up: a theme token that is never defined is a
theme the user selected and did not get.

## Acceptance criteria

- [x] `--text-muted`, `--accent-2` and `--bg-dim` are defined in **every** theme block `src/lib/theme.ts`
      can select at runtime — bare `:root`, light, dark, hc-light, hc-dark — reusing existing `--pal-*`
      primitives where one already carries the right value, adding primitives only where none does.
- [x] All 22 call sites drop their hex fallback and read the bare token, matching CPE-1810's migration.
- [x] The WCAG contrast guard test covers the three new tokens in every theme; the derived ratios are
      recorded in the work log. `--text-muted`'s light value in particular must pass, not merely be
      carried over from the old grey.
- [x] The `src/app.css.test.ts` hex ratchet is tightened to the new true counts (CPE-1810 left it at 87
      files / 442 occurrences) and is tight, not slack.
- [x] A guard test fails if any of the three tokens is removed from any theme block, and fails if the
      `var(--token, #hex)` fallback idiom reappears for them — the shape CPE-1810's
      `app.css.warn-token.test.ts` already pins for `--warn`. Extend that guard rather than adding a
      parallel one if it generalises cleanly.
- [x] Red-proof each new assertion: state which production change makes it fail, make that change, observe
      red, revert.
- [x] `npm run check` clean and the full vitest suite green.

## Notes

Found by the CPE-1810 worker while migrating `--warn`; it correctly left them out of scope rather than
widening its diff. Follow CPE-1810's precedent for the split-role question: a token used both as a
foreground and as a solid fill under white text needs two values (`--danger`/`--danger-fill`,
`--warn`/`--warn-fill`), because one value cannot satisfy both contrast roles. Check whether `--accent-2`
is used that way before assuming a single value is enough.

## Added 2026-08-20 (Visual Critic, during CPE-1810 review) — three more, same class

Found by looking at rendered screenshots rather than by grepping, which is why CPE-1810's
`var(--token, <fallback>)` grep could not see them:

1. **`--log-warn` has no `hc-dark` override.** It therefore inherits the light-theme `#8a5a00` and
   renders at **3.28:1 on `#0d0d0d`** — the least legible chip on the panel, in the theme whose entire
   premise is legibility. CPE-1810 added `--pal-hc-dark-amber-300: #ffcc66`, which is the correct value
   for it. One line.
2. **`.tl-badge.created` (`#3a9d4a`) and `.tl-badge.renamed` (`#3a72b5`)** in `AgentTimeline.svelte` are
   the same un-themed solid-fill-with-white-text shape CPE-1810 fixed for amber, in green and blue.
   `#3a9d4a` is duplicated in `SidecarManager`'s `.status.ok`. They need `--ok`/`--info` tokens with the
   `--x` / `--x-fill` split, following the `--danger`/`--warn` precedent.
3. The methodology note is the durable finding: a token audit that greps for `var(--token, #hex)` is
   structurally blind to a bare hex that never referenced the token at all. Audit by **rendered colour
   value**, not by call shape.

## Added 2026-08-20 (CPE-1810 Reviewer, round 2) — `--bg-dim` makes a whole pane untokenizable

`SidecarManager.svelte:376` sets `.logs { background: var(--bg-dim, #0f0f0f) }`. Because `--bg-dim`
is defined nowhere, that pane's real background is the literal `#0f0f0f` **in every theme** —
including light and hc-light. Any token calibrated against `--surface`/`--bg` is therefore measured
against the wrong reference on this surface.

CPE-1810 hit this directly: tokenizing `.log-error`/`.log-warn` onto `--danger`/`--warn` produced
**3.39:1 / 3.24:1 in light and 1.92:1 / 2.45:1 in hc-light** against the real `#0f0f0f`, where the
old flat literals (`#d08b2b`, `#c9a227`) read **6.77:1 / 7.92:1**. That change was reverted with a
comment pointing here; the pairing stays on literals until `--bg-dim` is real.

So this ticket owns two things for the log pane, in order:
- [x] Define `--bg-dim` per theme so the pane has a real background.
- [x] Then tokenize `.log-error` → `--danger` and `.log-warn` → `--warn`, which fixes a genuine
      semantic inversion (the caution token currently marks errors), and verify contrast **against
      the pane's own resolved background**, not against `--surface`.

**Guard gap worth closing here too:** no existing guard checks a token against a component-local
fixed-literal background — `dark-contrast`, `solid-fill-contrast` and `warn-token` all reason about
`--surface`/`--bg` or white. That is why this regression reached review instead of CI. A guard that
resolves each rule's *actual* background chain before computing contrast would have caught it.

## Work Log — 2026-08-23 (Worker)

Defined all three tokens in every theme block (`src/app.css`), migrated all 22 call sites off their
hex fallback, and — since defining `--bg-dim` for real is what CPE-1810 round 3 explicitly deferred
to this ticket — retokenized SidecarManager's `.log-error`/`.log-warn` onto `var(--danger)`/
`var(--warn)`, fixing the leftover error/caution colour inversion at the same time. `npm run check`
is clean and the full `vitest run` suite is green (328 files / 4416 tests).

**Values chosen, per theme:**

- **`--text-muted`** (20 call sites: AgentTimeline/ConsultedFiles/FileList — all foreground text,
  border, or `color-mix` tint use, nothing solid-fill) — resolved to the exact same `--pal-*`
  primitive `--text-faint` already uses per theme, since every real call site is the same
  "de-emphasised small/uppercase UI text" role `--text-faint` already serves (TRACE badge, gutter
  numbers): light `--pal-gray-600` `#6c6c6c`, dark `--pal-dark-gray-300` `#9c9c9c`, hc-light
  `--pal-hc-light-gray-600` `#404040`, hc-dark `--pal-hc-dark-gray-600` `#bfbfbf`. Kept its own call-
  site name rather than being renamed to `--text-faint`, since every consumer already called it
  `--text-muted` before it was ever tokenized. Measured: light 4.73–5.25:1 (vs bg/surface/surface-
  alt), dark 5.16–5.93:1, hc-light 10.37:1, hc-dark 10.57–11.42:1 — all clear their theme's floor
  (AA 4.5:1 light/dark, AAA-inspired 7:1 hc) with margin. The old `#9a9a9a` fallback was picked for a
  dark surface and measured only **2.81:1 on light's white `--surface`** — confirmed broken, not
  carried over; light's replacement value is a fresh pick, not the old grey.
- **`--accent-2`** (1 call site: BackupDashboard's `.mirror.auto`, a solid green fill under inherited
  white text, no foreground-text role anywhere) — kept the exact pre-existing fallback value,
  `#209764`, as ONE theme-invariant value across all five theme blocks (new primitive
  `--pal-accent2-fill`). Judgment call: since this token's only constraint (WCAG 1.4.11's 3:1
  white-on-fill floor) depends purely on the colour's own luminance, not on which theme's
  `--surface`/`--bg` it sits near, a single value can serve every theme with zero visual change from
  today's (accidental) rendering — CPE-1632 already computed white-on-`#209764` = 3.70:1, confirmed
  automatically by `src/app.css.solid-fill-contrast.test.ts`'s dynamic scanner for light/dark, and by
  a new direct assertion in `app.css.warn-token.test.ts` for all five themes (the hc scanner only
  resolves `--pal-hc-*`-prefixed primitives by design, so it can't see this intentionally-unprefixed,
  theme-invariant one — closed that specific blind spot with the new assertion rather than adding
  fake hc-prefixed aliases).
- **`--bg-dim`** (1 call site: SidecarManager's `.logs` console pane background) — resolved to
  `var(--surface)` in every theme (reusing the exact primitive already backing `--surface`, not a new
  value). Judgment call: this both (a) fixes the bug as reported — the pane now follows the
  surrounding UI's real panel colour per theme instead of rendering a theme-invariant near-black box
  in light/hc-light — and (b) is what makes retokenizing `.log-error`/`.log-warn` onto `var(--danger)`/
  `var(--warn)` provably safe: those tokens' contrast against `--surface` is ALREADY asserted by
  `light-contrast.test.ts`/`dark-contrast.test.ts`/`hc-contrast.test.ts`, so `--bg-dim == --surface`
  means the log pane inherits those already-verified numbers instead of needing a fresh, riskier
  derivation. Measured against the pane's real (now-resolved) background: light danger 5.66:1/warn
  5.93:1, dark danger 4.60:1/warn 4.61:1, hc-light danger 10.01:1/warn 7.83:1, hc-dark danger
  8.01:1/warn 13.03:1 — every pairing clears its theme's floor. This is the exact pairing CPE-1810
  round 2 tried and round 3 reverted (that attempt measured against `--surface` while the pane's real
  background was still the undefined `--bg-dim`'s `#0f0f0f` fallback, i.e. the wrong backdrop); making
  `--bg-dim` real removes that mismatch.

**Guards:**

- Extended `src/app.css.warn-token.test.ts` (rather than adding a parallel file, per the ticket's own
  instruction) to a token-list-driven guard covering `--warn`, `--warn-fill`, `--text-muted`,
  `--accent-2`, `--bg-dim`: (a) each resolves to a concrete hex in all five theme selectors — 37
  assertions; (b) no `.svelte` file writes `var(--<guarded-token>, <fallback>)` for any of them; (c)
  the raw `#209764` literal (specific enough to be unambiguous) never reappears outside the token —
  `--text-muted`'s `#9a9a9a` and `--bg-dim`'s `#0f0f0f` were deliberately left OUT of the raw-literal
  guard since both have legitimate unrelated uses elsewhere (`DiskSpaceView.svelte`'s already-real
  `var(--text-dim, #9a9a9a)`, `Icon.svelte`'s decorative SVG stroke) that a blanket ban would
  false-flag.
- Added a new guard in the same file closing the ticket's own named gap: resolves `--bg-dim` to its
  ACTUAL per-theme hex (not assumed `--surface`) and asserts `--danger`/`--warn` against that —
  exactly the check that would have caught CPE-1810 round 2's regression before it reached review.
- Added `--text-muted` as a direct, named assertion (not just inherited via `--text-faint`) to
  `light-contrast.test.ts`, `dark-contrast.test.ts`, and `hc-contrast.test.ts`.
- Added a direct white-on-`--accent-2` assertion across all five themes in `warn-token.test.ts`,
  closing the hc-solid-fill-contrast scanner's prefix blind spot noted above.
- Tightened `app.css.test.ts`'s hard-coded-hex ratchet from 86/423 to 86/399 (22 fallback-hex
  deletions + 2 more from the log-pane retokenization; no file dropped out of the "has hex" set —
  every touched file still carries unrelated out-of-scope hex).

**Red-proof (each new/extended guard demonstrated failing, then restored to green):**
1. Deleted `--text-muted`'s declaration from the light theme block → `warn-token.test.ts`'s
   resolution check failed (`did not resolve to a hex`) and `light-contrast.test.ts` threw; restored,
   both green again.
2. Reintroduced `var(--text-muted, #9a9a9a)` at one call site → the fallback-idiom guard failed with
   the offending file named; restored, green again.
3. Pointed hc-light's `--bg-dim` at a literal that collides with `--danger` (1.00:1) → the new
   resolved-background guard failed with the exact ratio and floor; restored, green again.

**Assumptions logged:** `--text-muted` intentionally resolves to the identical value as
`--text-faint` rather than a fourth distinct grey (no distinguishing requirement found in any call
site); `--accent-2` intentionally stays theme-invariant (matches its pre-existing accidental
behaviour and its WCAG constraint is theme-independent); `--bg-dim` intentionally equals `--surface`
rather than a new "recessed panel" shade, trading the log console's old distinct near-black identity
for a provably-correct, already-verified contrast pairing — flagged here in case product/design wants
a visually distinct (but still per-theme-correct) console background in a follow-up.

**Not done / explicitly skipped:** the `gui-smoke` headless harness was not run — it requires
`tauri-driver` + `msedgedriver` on `PATH` (neither installed on this machine) and a fresh release
build in this worktree (none exists); installing those tools would modify shared machine-global
tooling other agents are currently using, which is outside this worker's remit. Relied on the
automated WCAG contrast guards instead (deterministic, stronger proof for a pure colour-token change
than a screenshot would be).
