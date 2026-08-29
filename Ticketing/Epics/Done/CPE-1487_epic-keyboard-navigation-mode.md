---
id: CPE-1487
title: "EPIC: Keyboard Navigation Mode — opt-in vim-modal layer (out-TUI the TUIs)"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed: 2026-08-29
---

> **Filed 2026-08-08 (sprint PM, competitive-landscape pass — TUI survey; see research-library
> `competitive-file-managers-2026-08-08`).** Activated + decomposed 2026-08-10 (sprint PM bench-refill
> pass, now unblocked by CPE-1484 shipping). Children: CPE-1552 (mode-state reducer + Settings toggle,
> foundation), CPE-1553 (motion → selection-engine bridge), CPE-1554 (`:` command-line bridge into
> Command Palette verbs), CPE-1555 (mode indicator + cheatsheet + docs page), CPE-1556 (App.svelte
> integration — single opt-in-gated wiring point, dispatched last). 0 of 5 children Done.

## Why (THE biggest keyboard-first differentiator TUIs have that CPE lacks)
Every vim-descended TUI surveyed (ranger, vifm, nnn, lf, felix, xplr) is loved for one thing above all: **you
run the entire file manager without touching the mouse**, using pre-existing vim muscle memory, and it feels
faster than any GUI. CPE has rich shortcuts but no modal layer and no "operate entirely from the home row".
This is the clearest "make a GUI that competes with a TUI" win on the whole survey. Strictly **opt-in /
off-by-default**, so PURPOSE.md's fast/small/predictable core is untouched when off.

## Goal
An optional **Navigation Mode**: a modal key layer over the file list/panes — `h/j/k/l` move, `gg`/`G` jump,
`/` filter, `v` visual-range select, `d`/`y`/`p` cut/copy/paste, `:`-style mini command line into the existing
Command Palette verbs — with a clear on-screen mode indicator.

## Depends on
**CPE-1484 (Hotkey customization)** — build on its keymap store/seam so modal bindings are themselves
remappable and can't collide with global keys. Sequence CPE-1484 → CPE-1487.

## Rough slices (JIT)
- A modal dispatch layer (mode state + key router) reading the CPE-1484 keymap; reuses existing selection /
  file-op / navigation primitives (don't reimplement ops).
- Visual-range multi-select (`v` + motion) on top of CPE-711's selection engine.
- The `:` mini command line bridging to CommandPalette verbs.
- Mode indicator + a discoverable cheatsheet; Settings toggle (off by default).
- Ship its docs page per CPE-579.

## Notes
Pure frontend, no backend. Off-by-default is the guardrail. Marks (`m`/`'`) are a natural follow-on once this
lands (Favorites + Spotlight already cover fuzzy-jump, so marks are a small extension, not a separate epic).

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit) WITH ONE RESIDUAL. All 5 children Done. Opt-in, default off.

Verified: `navMode.ts` `reduceNavKey` is a pure reducer covering motions, counts (`3j`), `gg`/`G`, visual mode, `d`/`y`/`p`, `/` and `:`. Motions drive **the real selection engine**, not a copy. Ops reuse the existing handlers, `/` reuses the toolbar's own filter rather than a second filter UI, and `:` bridges to the Command Palette **through `commandPalette.ts`'s own filter over the same Command list** - so there is one command surface, not two. The opt-in gate is the first condition of the branch and bare-keys-only, so Ctrl/Alt/Meta fall through.

RESIDUAL - modal bindings are **not** routed through CPE-1484's keymap; the h/j/k/l table is hardcoded and cannot be remapped. Both CPE-1552 and CPE-1556 declare this an explicit fast-follow and no ticket was ever filed. Note the other half of that dependency - collision with global keys - **is** solved, by the bare-key gate.
