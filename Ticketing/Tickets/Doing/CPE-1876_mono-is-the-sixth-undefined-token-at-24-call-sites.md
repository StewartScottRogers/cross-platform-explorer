---
id: CPE-1876
title: --mono is the sixth undefined token, at ~24 call sites across ~20 components
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

`--mono` is referenced as `var(--mono, ui-monospace, monospace)` (and similar chains) at **~24 call
sites across ~20 components**, and is **defined nowhere** — not in `src/app.css`, not anywhere else.
So the inline fallback font stack is the only value that ever applies, in every theme.

Found by PR #1009's independent UAT, which was asked one question needing no browser: *is there a
sixth undefined token sitting in the tree right now?* It grepped every `var(--token, <fallback>)` in
`src/**/*.svelte` and diffed against every token actually defined in `src/app.css`.

Call sites span: `BinaryPreview`, `CertPreview`, `ConflictDialog`, `DiffPeek`, `DiffSideBySide`,
`EmailPreview`, `IcalPreview`, `JsonTreeNode`, `JwtPreview`, `KeyboardBindingsDialog`, `LogPreview`,
`MacrosDialog`, `NotebookPreview`, `PreviewPane`, `SyncDialog`, `TemplatesDialog`,
`VaultCreateDialog`, `WorkbenchView`, `BoardView`.

That is **more call sites than CPE-1821's three tokens combined**.

## Severity, stated honestly

Lower than the colour tokens, and this is the reason it is Medium rather than High: a monospace font
stack is a *reasonable* thing to fall through to, so nothing looks broken today. Contrast that with
`--text-muted`, whose `#9a9a9a` fallback was applying grey text on dark backgrounds and in both
high-contrast themes.

But it is the identical shape — a token that reads as themeable and is not — and it means the app has
**no single place to change its monospace face**. Anyone who sets `--mono` expecting it to take effect
will find it silently ignored at all 24 sites.

## What the UAT also ruled out (do not re-investigate these)

- `--accent-soft`, `--surface-2` — undefined, but each is used as a **two-level chain** that falls
  through to the real, live `--surface` (`var(--accent-soft, var(--surface))`). They render
  theme-correctly today. Odd, dead first-level reference; not this bug.
- `--agent-accent`, `--indent`, `--sw` — genuinely local per-element custom properties set via inline
  `style="--x: …"` in JS (per-agent row colour, per-line indent guide, per-tag swatch). Correct,
  intentional pattern.

## What to do

1. Define `--mono` once, in every theme block, as the app's monospace stack. It is almost certainly
   theme-invariant — say so explicitly rather than duplicating the same value five times without
   comment.
2. Strip the now-dead fallback from all ~24 call sites, the way CPE-1821 did for its three.
3. Add it to the guarded-token list — **or**, better, land **CPE-1875** first (replace the
   enumerated five-name guard with a detector) and let that catch this automatically. If CPE-1875
   lands first, this ticket shrinks to "define the token and strip the fallbacks", and the detector
   proves the job is complete.

## Acceptance criteria

- [x] `--mono` resolves in all five theme selectors.
- [x] No `var(--mono, …)` fallback remains in `src/`.
- [x] Changing `--mono` in one place visibly changes every monospace surface — demonstrated, not asserted.
- [x] A guard fails if the definition is removed.

## Work Log

- **2026-08-23 16:20 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Fourth known instance of this defect class: CPE-1810 (`--warn`), CPE-1821 (three colour tokens),
  CPE-1875 (the guard only enumerates), and now this. The recurrence is the argument for CPE-1875.
- **2026-08-26** — CPE-1875 landed first (as the ticket hoped) and its detector already carried a
  dated `--mono` ALLOWLIST entry pointing at this ticket, so this ticket shrank to exactly what its
  own "What to do" section predicted: define the token and strip the fallbacks.
  - Defined `--mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;` once per theme, in
    all five `src/app.css` blocks (bare `:root`, light, dark, hc-light, hc-dark) — theme-invariant
    by design (a monospace typeface has no light/dark/contrast dimension), following the same
    duplicated-not-derived precedent CPE-1821 set for `--accent-2`. Value chosen as the richest of
    the pre-existing per-call-site fallbacks (DiffPeek/DiffSideBySide's), so those two sites are
    visually unchanged and every shorter chain (`monospace` alone, or missing SFMono-Regular/Menlo/
    Consolas) gains the platform-specific names it was previously missing.
  - Stripped the fallback at all 32 call sites across 21 components (`var(--mono, ...)` ->
    `var(--mono)`) — more than the ticket's own "~24 across ~20" estimate; the actual count also
    turned up `DiagnosticsOverlay.svelte` and `YamlTomlPreview.svelte`, not named in the ticket body.
  - Removed the now-stale `--mono` entry from `src/app.css.warn-token.test.ts`'s CPE-1875
    ALLOWLIST (the fallback that made it "discoverable" no longer exists) and added `--mono` its
    own dedicated resolution + no-dead-fallback guard in that same file, parallel to but separate
    from `GUARDED_TOKENS`/`resolveHex` — CPE-1875's generic detector asserts resolution via
    `HEX_RE`, which a font-family stack structurally can never match, so `--mono` needs its own
    non-hex resolution check to satisfy AC1/AC4 rather than relying on the generic detector alone.
    Verified by deliberately deleting one theme's `--mono` declaration and confirming the new guard
    fails loudly (2 assertions), then restoring it and confirming green again.
  - `npm run check` clean; `npx vitest run` 4611/4613 passing (the 2 pre-existing `msrvSync.test.ts`
    failures reproduce identically on `main` with these changes stashed — unrelated to this ticket,
    CPE-1855's CI MSRV job is simply missing). `npx vite build` compiles cleanly; verified the
    built CSS carries the identical `--mono` declaration in all five theme blocks.
  - Visual surfaces touched: every monospace text run in `BinaryPreview`, `BoardView` (no-project
    path), `CertPreview`, `ConflictDialog`, `DiagnosticsOverlay`, `DiffPeek`, `DiffSideBySide`,
    `EmailPreview`, `IcalPreview`, `JsonTreeNode`, `JwtPreview`, `KeyboardBindingsDialog`,
    `LogPreview`, `MacrosDialog`, `NotebookPreview`, `PreviewPane` (code/markdown views),
    `SyncDialog`, `TemplatesDialog`, `VaultCreateDialog`, `WorkbenchView`, `YamlTomlPreview` — the
    rendered typeface is unchanged from before (canonical value matches the richest pre-existing
    fallback) except sites whose old fallback was a shorter chain (`monospace` alone, or missing
    SFMono-Regular/Menlo/Consolas), which now pick up the same platform-correct monospace face the
    richer call sites already had. No colour/layout change; light vs. dark are identical here since
    `--mono` is intentionally theme-invariant.
