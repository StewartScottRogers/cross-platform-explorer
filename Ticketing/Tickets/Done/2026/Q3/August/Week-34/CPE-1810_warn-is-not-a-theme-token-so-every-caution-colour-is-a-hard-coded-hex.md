---
id: CPE-1810
title: "--warn is not a theme token, so every caution colour in the app is a hard-coded hex"
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-20
---

## Problem

`--warn` is referenced as a CSS custom property in a number of components — but **it is not defined
anywhere in `src/`.** Every one of those call sites is really `var(--warn, <hex>)`, so the fallback
*always* wins and the "caution" colour is a hard-coded hex in disguise.

The codebase already knows this and says so in two places:

- `src/app.css.solid-fill-contrast.test.ts:237-241` — resolves a literal fallback "when the token itself
  isn't defined anywhere in that theme (e.g. `--warn` — components that lean on a CSS custom-property
  fallback instead of a real theme token)".
- `src/lib/components/AgentTimeline.svelte:2102-2105` — calls it **"this file's *older* `var(--warn, <hex>)`
  fallback idiom"**, contrasts it with "real, always-defined semantic vars", and says to use those,
  "never a hard-coded hex".

So the deprecation is documented; the migration was never done.

## Why it matters

A fixed hex renders identically in light and dark. This app ships a real dark theme (CPE-1492/1493) with a
WCAG contrast guard, and the amber values in use (`#b5872b`, `#b8860b`) are precisely the case that guard
exists for. The result is that a **warning is least legible in the theme where it most needs to be seen** —
and the contrast guard cannot catch it, because the value never passes through a token it inspects.

There is also a slow ratchet effect: each of these sites is a hard-coded hex occurrence, and the repo's
`BASELINE_TOTAL_HEX_OCCURRENCES` guard only ratchets down. Every new caution-coloured element copied from
an existing one pushes against that guard rather than with it.

## What to do

- **Define `--warn` as a real semantic token in *both* the light and dark palette blocks**, with values
  chosen to pass the WCAG contrast guard in each. Show the contrast numbers; do not eyeball them.
- Then migrate the existing `var(--warn, <hex>)` call sites and **delete the now-dead fallbacks**. A
  half-migration is worse than none — it leaves readers unable to tell which sites are live.
- Ratchet `BASELINE_TOTAL_HEX_OCCURRENCES` **down** by the number of literals removed, so the guard records
  the improvement rather than merely tolerating it.
- Check whether the same shape exists for other undefined tokens; `--warn` was found by accident, so
  assume it is not alone. Grep for `var(--` with a fallback and cross-check each name against the palette
  blocks.

## Notes

Filed by the Foreman during the batched sprint, 2026-08-20. Found when CPE-1803's fix reached for the
"caution" idiom, hit the hard-coded-hex ratchet, and bumped the baseline to get past it — the guard was
right and the idiom was wrong. That PR took the correct narrow route (real tokens, box treatment instead of
hue, baseline restored); this ticket is the general fix it declined to take on as scope creep.

Related: **CPE-1803**, **CPE-1492/1493** (the dark theme and its contrast guard).

## Work Log

**2026-08-20** — Implemented on branch `cpe-1810-warn-theme-token`.

- Defined `--warn` (general foreground/border/tinted-background token) and a sibling `--warn-fill`
  (solid-fill-background-with-white-text role, ConsentSheet's `.badge` — the same two-role tension
  CPE-1632/CPE-1649 already found for `--danger`/`--danger-fill`) in **all four** live theme blocks —
  light, dark, hc-light, hc-dark (the ticket asked for light+dark; hc-light/hc-dark were added too
  because they're wired live via `theme.ts`, and deleting the fallback without a value there would
  have made the caution colour silently disappear in high-contrast mode instead of just being wrong).
  Contrast numbers (WCAG 2.1, computed not eyeballed):
  - light: `--warn`/`--warn-fill` = `#8a5a00` (reused `--pal-amber-700`) — 5.93:1 vs `--surface`
    (white); no role tension since `--surface` IS literal white there.
  - dark: new `--pal-dark-amber-500: #c38800` — fgSurf 4.61:1 (>=4.5 AA body text), fgBg 5.31:1,
    white-on-fill 3.07:1 (>=3 WCAG 1.4.11 UI floor). One value serves both roles (positive but
    narrow luminance window, ~[0.283, 0.30]) — no split needed, unlike hc-dark below.
  - hc-light: new `--pal-hc-light-amber-900: #734900` — 7.83:1 vs white (clears the AAA-inspired
    7:1 bar); trivial, same reasoning as light.
  - hc-dark: negative window (7:1 fgSurf needs relative luminance >=0.3282; white-on-fill 3:1 needs
    <=0.30 — same non-overlapping gap CPE-1649 found for `--danger`), so split into
    `--pal-hc-dark-amber-300: #ffcc66` (13.03:1 vs surface, text-role only) and
    `--pal-hc-dark-amber-fill: #bd8300` (white-on-fill 3.28:1, fill-role only).
- Migrated every real `var(--warn, <hex>)` / `var(--warn, <hex>)`-as-background call site
  (AgentTimeline.svelte x10, ConsentSheet.svelte x2, ExplorerPane.svelte x1, ImageCompareView.svelte
  x3, SidecarManager.svelte x8 = 24 total) onto the real tokens and deleted the fallbacks — 24 hex
  literals removed, not the 22 I first hand-counted (AgentTimeline had 10 call sites, not 8; missed
  two on first pass). Updated the two doc comments that referenced the old undefined-token idiom by
  name (AgentTimeline's `.hd-unclean-note` comment, TrashView's `.tv-degraded-note` comment) so they
  no longer claim `--warn` is undefined.
- Ratcheted `src/app.css.test.ts`'s `BASELINE_FILES_WITH_HEX` 90->87 and
  `BASELINE_TOTAL_HEX_OCCURRENCES` 466->442 (actual pre-fix count was 88/466 — the files-with-hex
  baseline already had 2 free of slack before this ticket). ImageCompareView.svelte dropped out of
  the "has hex" set entirely — its only hex literals were `--warn` fallbacks.
- Added `src/app.css.warn-token.test.ts`: (a) asserts `--warn`/`--warn-fill` resolve to a concrete
  hex in all 5 theme blocks (bare `:root` default + light/dark/hc-light/hc-dark) as a HARD failure —
  deliberately not reusing the existing solid-fill-contrast guards' resolution, which silently SKIPS
  an unresolvable pairing instead of failing (documented there as "nothing to assert against
  statically"), which is exactly the blind spot that let `--warn` go undefined for years; (b) greps
  every `.svelte` file (comments stripped) for a `var(--warn`/`var(--warn-fill` call site that still
  carries a fallback, guarding against a future half-migration. Verified both assertions go red:
  removing hc-dark's `--warn` line makes test (a) fail with `.toMatch() expects to receive a string,
  but got undefined`; reintroducing `var(--warn, #d08b2b)` in SidecarManager makes test (b) fail
  listing that file as an offender — and separately makes the *existing* ratchet test in
  `src/app.css.test.ts` fail too (`expected 443 to be less than or equal to 442`), confirming the
  tightened baseline actually bites. Both reverted after observing red.
- **"Same shape" check (requested, not fixed)**: grepped every `.svelte` + `app.css` for
  `var(--token, <fallback>)` and cross-referenced each token name against everything `app.css`
  actually defines. Found several more undefined-token-with-hex-fallback sites of the identical
  shape, left out of scope for this ticket (fix exactly what was asked; no drive-by refactors) —
  flagging for a follow-up ticket:
  - `--text-muted` (fallback `#9a9a9a`) — AgentTimeline.svelte (~17 sites), ConsultedFiles.svelte
    (2), FileList.svelte (1). By far the biggest offender — same shape as `--warn` was.
  - `--accent-2` (fallback `#209764`) — BackupDashboard.svelte, 1 site.
  - `--bg-dim` (fallback `#0f0f0f`) — SidecarManager.svelte, 1 site.
  - Lower-priority/likely-benign, not colour/WCAG-relevant: `--mono` (font-family fallback, ~20
    files) is a different risk category entirely (no contrast implication). `--agent-accent`,
    `--accent-soft`, `--surface-2` all fall back to *another real, always-defined token* (e.g.
    `var(--agent-accent, var(--agent-unknown))`), not a raw hex, so they're not the same bug — the
    fallback there is an intentional default, not a disguised hard-coded colour.
- Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` (full suite) — **317 test files,
  4193 tests passed** (4182 pre-existing + 11 new in `app.css.warn-token.test.ts`). No Rust touched,
  so no `cargo clippy` run.
- Not user-facing in the CPE-579 sense (no new Section/control — a colour-correctness bug fix to
  existing UI), so no `src/docs/*.md`/`sectionDocs.ts` change.

**2026-08-20 (round 2 — CHANGES REQUESTED, attempt 2 of 3)** — Independent Reviewer + Visual Critic
both found the same root cause: **the round-1 grep methodology was structurally blind to bare-literal
offenders.**

- **Corrected methodology.** Round 1's completeness check grepped for `var(--token, <fallback>)` —
  that shape can only ever find a call site that already references `--warn` through `var()`. It
  cannot see a site that hard-codes the identical amber literal directly with no token reference at
  all, which turned out to be the shape of most of the remaining offenders. Round 2's actual check
  was a plain literal grep — `grep -rn "#b5872b\|#b8860b" src --include="*.svelte" --include="*.ts"`
  — cross-referenced against every hit's surrounding CSS rule to judge foreground/border/tint
  (`--warn`) vs. solid-fill-with-white-text (`--warn-fill`) role by hand. This is the check that
  should have been run in round 1 instead of (or alongside) the `var()`-shaped one.
- **19 more real call sites found this way (20 hex occurrences — one SyncDialog rule carried the
  literal twice), across 11 components not previously touched**, migrated onto `--warn`/`--warn-fill`:
  `--warn-fill` (solid-fill-with-white-text, same role `--danger-fill` already established for
  `.tl-badge.removed`/etc.): `AgentTimeline.svelte:1307` `.tl-badge.modified`,
  `ExplorerPane.svelte:700` `.agent-chip.modified`, `FileList.svelte:926` `.agent-badge.modified`,
  `SyncDialog.svelte:264` `.btn.warn-btn:hover`. `--warn` (foreground/border/tint, everything else):
  `CheckpointDialog.svelte:382,390`, `CompareDialog.svelte:224,234`, `ConflictDialog.svelte:227`,
  `DataBrowser.svelte:154` (found by my own fresh grep, not on the Reviewer's list — a genuine
  `.db-error` state, but the codebase's own precedent, per CPE-1803's TrashView comment,
  distinguishes a recoverable/soft failure rendered in the caution amber from a hard failure in
  `--danger` red; treated as a mechanical literal migration, not a severity redesign),
  `IntegrityDialog.svelte:146`, `Sidebar.svelte:1042` (`.drive-bar-fill.warn` — plain fill, no
  overlaid text, so `--warn` not `--warn-fill`, matching its sibling `.full { background:
  var(--danger); }` which also isn't `--danger-fill`), `StatusBar.svelte:181-183`,
  `SyncDialog.svelte:220,245,252,263`.
- **Semantic inversion fixed** (Visual Critic finding, not on the Reviewer's list):
  `SidecarManager.svelte`'s `.log-error` had been pointed at `var(--warn)` in round 1 — backwards,
  the caution token marking an actual error — while `.log-warn` sat directly below it on a still-bare
  `#c9a227` (2.42:1 on white, below AA and below the 3:1 large-text floor even in the high-contrast
  light theme; in dark only 1.27:1 separated it from `.log-error`, i.e. WARN and ERROR log lines
  rendered indistinguishably). Fixed the pair: `.log-error` -> `var(--danger)`, `.log-warn` ->
  `var(--warn)`. Flagging explicitly per the Foreman's instruction: **a migration that freezes a
  semantic inversion in place is worse than the hex was** — round 1 touched this exact line and
  picked the wrong token without checking what the surrounding pair actually meant.
- **Confirmed already-correct from round 1**: `AgentTimeline.svelte:1307`'s `.tl-badge.modified` was
  independently flagged by both the Reviewer and the Visual Critic as a within-panel amber mismatch
  regression risk — but it was already migrated to `var(--warn-fill)` as part of this round's literal
  sweep (see above), so no separate action was needed once the sweep landed.
- **Deliberately left untouched, per the Foreman**: `src/lib/sessionChip.ts:14`'s `#b5872b` (fixed
  categorical session-identity palette, theme-invariant by its own comment — excluded in both the
  Work Log and in the new guard test's comment). Also explicitly out of scope, filed separately by
  the Foreman: hc-dark never overriding `--log-warn` (inherits the light value at 3.28:1 on
  `#0d0d0d`), and `.tl-badge.created`/`.tl-badge.renamed` (`#3a9d4a`/`#3a72b5`) being the same
  un-themed solid-fill shape in green/blue, with `#3a9d4a` also duplicated in `SidecarManager
  .status.ok`.
- **Ratchet re-tightened** in `src/app.css.test.ts`, exact, no headroom: 87/442 -> 86/422 (the 19-site
  sweep, 20 occurrences, minus the 1 file — DataBrowser.svelte — that dropped out of the "has hex" set
  entirely) -> 86/421 (the log-warn fix, one more literal removed; files-with-hex unchanged since
  SidecarManager still carries other hex elsewhere).
- **Extended `src/app.css.warn-token.test.ts`** with a third invariant: a direct regex guard
  (`/#(?:b5872b|b8860b)\b/i`) that no `.svelte` file may hard-code either raw literal, independent of
  whether it arrives via `var(--warn, <fallback>)` or with no token reference at all — closing the
  exact blind spot round 1's methodology had. `sessionChip.ts` is excluded (a) structurally, since the
  guard only walks `.svelte` files and that file is `.ts`, and (b) explained in the guard's own
  in-file comment for why it would stay excluded even if the walker were ever widened.
- **Red-proofed every new/changed assertion, minimal breakage, each observed then reverted**:
  - New literal guard: put `#b5872b` back into `ConflictDialog.svelte:227`'s `.warn` rule ->
    `.svelte file(s) still hard-coding the raw --warn hex instead of the token:
    src/lib/components/ConflictDialog.svelte`. Reverted.
  - Re-tightened ratchet (the log-warn fix specifically): put `#c9a227` back into
    `SidecarManager.svelte:391`'s `.log-warn` rule -> `total hard-coded hex literal occurrences:
    expected 422 to be less than or equal to 421`. Reverted.
  - (The ratchet's earlier 86/422 tightening from the literal sweep itself was red-proofed the same
    way against 87/442 before this round's further fix landed; not repeated here since the value has
    since moved again — see the two proofs above for the current 86/421.)
- Gates re-run after all fixes: `npm run check` — 0 errors, 0 warnings. `npx vitest run` (full suite)
  — **317 test files, 4192 tests passed**. (Down 1 from round 1's 4193, not a regression: added 1 new
  test to `app.css.warn-token.test.ts`'s invariant (c), but the auto-derived
  `solid-fill-contrast.test.ts`/`hc-solid-fill-contrast.test.ts` scanners each lost their
  theme-invariant "white on hard-coded #b5872b" literal-pairing test — 1 per file, 2 total — because
  there are no longer any literal-hex solid-fill-with-white-text consumers of that hex left anywhere;
  every one of them (`.tl-badge.modified`/`.agent-chip.modified`/`.agent-badge.modified`/
  `.btn.warn-btn:hover`) now resolves through the real `--warn-fill` token instead, which is still
  fully asserted — confirmed by reading the verbose test list before and after.) No Rust touched, so
  no `cargo clippy` run.
**2026-08-20 (round 3 — CHANGES REQUESTED, attempt 3 of 3)** — Independent Reviewer confirmed round
2's completeness work was sound (all 19 sites, all four role assignments, the exact 86/421 ratchet,
the `.svelte`-only guard boundary, and the test-count drop as genuine consolidation rather than
coverage loss). One blocking regression remained, introduced by round 2 itself.

- **The bug**: `SidecarManager.svelte`'s `.logs` pane sets `background: var(--bg-dim, <hex>)`, and
  `--bg-dim` is undefined nowhere in `app.css` — so that pane's REAL background is the fixed literal
  fallback in every theme, never the theme's own `--surface`/`--bg` that `--danger`/`--warn` are
  calibrated against. Round 2's `.log-error` -> `var(--danger)` / `.log-warn` -> `var(--warn)` fix
  was measured against `--surface` (white in light/hc-light) — a surface this pane never renders.
  Re-derived against the pane's actual fixed backdrop: light 3.39:1/3.24:1 and hc-light
  1.92:1/2.45:1 — both under the 4.5:1 AA text floor, hc-light's `.log-error` even under the 3:1 UI
  floor. Strictly worse than the flat pre-ticket literals it replaced (~6.8-7.9:1 against that same
  backdrop, clearing by accident of having been picked for a dark backdrop originally). No existing
  guard catches this shape — `app.css.dark-contrast.test.ts`, `solid-fill-contrast.test.ts`, and this
  ticket's own `warn-token.test.ts` all check a token against `--surface`/`--bg` or white; none checks
  a token against a component-local fixed literal.
- **The fix — reverted, not retuned**: `.log-error`/`.log-warn` restored to their pre-ticket literal
  values (both bare hex, matching the honest shape — `.log-error` was `var(--warn, <hex>)` before
  this ticket ever touched it, an undefined-token-with-fallback site in its own right, so a bare
  literal with the fallback removed is correct, not a re-added fallback). A block comment sits
  directly above both rules explaining exactly why: the pane's fixed backdrop, the specific numbers
  that regressed, that `--danger`/`--warn` are global tokens serving many surfaces so retuning them
  would be the wrong lever, and that this is blocked on CPE-1821 (now extended to own this whole log
  pane) making `--bg-dim` a real token before this pairing can be tokenized correctly. The comment
  deliberately spells hex values without a leading `#` (e.g. writes `<hex>` as a placeholder, matching
  the codebase's existing convention in the AgentTimeline/TrashView comments) so prose explaining the
  bug doesn't itself inflate the hex-literal ratchet.
- **The semantic-inversion observation from round 2 stands and is worth keeping on record**:
  `.log-error` pointed at the caution token while `.log-warn` sat on an untokenized hex was a real
  bug, independent of this round's finding. It just turns out the *correct* fix is blocked on
  `--bg-dim`, and reverting to the flat literals is the honest interim — a worse-contrast token swap
  would have been strictly worse than doing nothing, not merely equivalent.
- **Ratchet re-tightened**, exact, no headroom: 86/421 -> **86/423** (two literals restored;
  files-with-hex unchanged since `SidecarManager.svelte` already carried other hex literals before
  and after this change).
- **Red-proofed the re-tightened ratchet, minimal mutation, observed then reverted**: appended a
  throwaway ` /* temp #123456 red-proof */` comment onto `.log-error`'s declaration in
  `SidecarManager.svelte:406` (one extra hex occurrence) -> `total hard-coded hex literal
  occurrences: expected 424 to be less than or equal to 423`. Reverted. No other assertion changed
  this round (the guard test file itself was untouched — `#d08b2b`/`#c9a227` don't match its
  `/#(?:b5872b|b8860b)\b/i` literal guard, and neither restored rule reintroduces a
  `var(--warn[-fill], <fallback>)` call site), so no further red-proof was needed.
- Gates re-run after the fix: `npm run check` — 0 errors, 0 warnings. `npx vitest run` (full suite) —
  **317 test files, 4192 tests passed** (unchanged from round 2 — this round touched only production
  code, a doc comment, and two ratchet constants; no test was added or removed).
