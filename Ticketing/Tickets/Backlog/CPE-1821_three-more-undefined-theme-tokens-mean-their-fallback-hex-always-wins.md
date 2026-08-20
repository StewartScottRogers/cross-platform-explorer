---
id: CPE-1821
title: three more undefined theme tokens mean their fallback hex always wins, in every theme
type: bug
priority: Medium
status: Backlog
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

- [ ] `--text-muted`, `--accent-2` and `--bg-dim` are defined in **every** theme block `src/lib/theme.ts`
      can select at runtime — bare `:root`, light, dark, hc-light, hc-dark — reusing existing `--pal-*`
      primitives where one already carries the right value, adding primitives only where none does.
- [ ] All 22 call sites drop their hex fallback and read the bare token, matching CPE-1810's migration.
- [ ] The WCAG contrast guard test covers the three new tokens in every theme; the derived ratios are
      recorded in the work log. `--text-muted`'s light value in particular must pass, not merely be
      carried over from the old grey.
- [ ] The `src/app.css.test.ts` hex ratchet is tightened to the new true counts (CPE-1810 left it at 87
      files / 442 occurrences) and is tight, not slack.
- [ ] A guard test fails if any of the three tokens is removed from any theme block, and fails if the
      `var(--token, #hex)` fallback idiom reappears for them — the shape CPE-1810's
      `app.css.warn-token.test.ts` already pins for `--warn`. Extend that guard rather than adding a
      parallel one if it generalises cleanly.
- [ ] Red-proof each new assertion: state which production change makes it fail, make that change, observe
      red, revert.
- [ ] `npm run check` clean and the full vitest suite green.

## Notes

Found by the CPE-1810 worker while migrating `--warn`; it correctly left them out of scope rather than
widening its diff. Follow CPE-1810's precedent for the split-role question: a token used both as a
foreground and as a solid fill under white text needs two values (`--danger`/`--danger-fill`,
`--warn`/`--warn-fill`), because one value cannot satisfy both contrast roles. Check whether `--accent-2`
is used that way before assuming a single value is enough.
