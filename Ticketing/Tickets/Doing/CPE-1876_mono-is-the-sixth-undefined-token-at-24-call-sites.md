---
id: CPE-1876
title: --mono is the sixth undefined token, at ~24 call sites across ~20 components
type: bug
priority: Medium
status: Backlog
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

- [ ] `--mono` resolves in all five theme selectors.
- [ ] No `var(--mono, …)` fallback remains in `src/`.
- [ ] Changing `--mono` in one place visibly changes every monospace surface — demonstrated, not asserted.
- [ ] A guard fails if the definition is removed.

## Work Log

- **2026-08-23 16:20 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Fourth known instance of this defect class: CPE-1810 (`--warn`), CPE-1821 (three colour tokens),
  CPE-1875 (the guard only enumerates), and now this. The recurrence is the argument for CPE-1875.
