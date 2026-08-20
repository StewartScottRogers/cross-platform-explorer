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
- [ ] Define `--bg-dim` per theme so the pane has a real background.
- [ ] Then tokenize `.log-error` → `--danger` and `.log-warn` → `--warn`, which fixes a genuine
      semantic inversion (the caution token currently marks errors), and verify contrast **against
      the pane's own resolved background**, not against `--surface`.

**Guard gap worth closing here too:** no existing guard checks a token against a component-local
fixed-literal background — `dark-contrast`, `solid-fill-contrast` and `warn-token` all reason about
`--surface`/`--bg` or white. That is why this regression reached review instead of CI. A guard that
resolves each rule's *actual* background chain before computing contrast would have caught it.
