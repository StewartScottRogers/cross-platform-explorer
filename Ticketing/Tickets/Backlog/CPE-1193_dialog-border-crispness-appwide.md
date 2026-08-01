---
id: CPE-1193
title: "App-wide: make modal dialog borders read as a clearly-visible edge, not just a shadow"
type: chore
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
Visual-Critic observation during the CPE-705 pass: the PasswordPromptDialog reads as resting on a drop-shadow
halo with no crisp edge. On inspection its border CSS is **identical to `ConfirmDialog.svelte` and the shared
dialog standard** (`.dialog { border: 1px solid var(--border-strong); box-shadow: 0 20px 50px rgba(0,0,0,.25) }`)
— so this is **app-wide**, not specific to one dialog. Per [[dialogs-need-visible-border]] every dialog should
have a *clearly-visible* thin border, not just a shadow; the current `--border-strong` on a white surface over a
dimmed backdrop can read as shadow-only, especially on smaller modals.

## Build
- Raise dialog-edge crispness **consistently across all modals** — e.g. bump the shared `.dialog` border to a
  more contrasting token (or add a subtle inset ring) in the common dialog styling, so ConfirmDialog,
  PasswordPromptDialog, PropertiesDialog, SettingsDialog, TemplatesDialog, etc. all gain the crisper edge
  together. Do NOT fix a single dialog in isolation (that breaks the design-system consistency).
- Re-verify a few dialogs via gui-smoke screenshots + the Visual Critic afterwards (light-theme only).

## Acceptance Criteria
- [ ] Modal dialogs show a clearly-visible thin border (not shadow-only) uniformly; no single dialog is an
      outlier; `npm run check` + tests green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift) from the CPE-705 Visual-Critic finding; scoped app-wide because the
  password modal already matches the shared dialog border standard exactly.

## Evidence 2026-08-01 (workshift)
The epic-704/CPE-1221 Visual Critic pixel-sampled the `NearDuplicatesDialog` edges and found the
`1px solid var(--border-strong)` present in CSS does NOT render as a distinct border line against the
dimmed backdrop — it reads only as a white→grey transition (border colour ≈ backdrop value). Concrete
confirmation that `--border-strong` is too weak for the dialog-on-dimmed-backdrop case this ticket
targets. Whatever fix lands here should be verified by re-capturing a dialog screenshot and pixel-
sampling the edge, not just by reading the CSS. Applies to all dialogs sharing this pattern
(SimilarImagesDialog, NearDuplicatesDialog, DuplicatesDialog, …).
