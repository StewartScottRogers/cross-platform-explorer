---
title: Navigation Mode
order: 37
category: Appearance & Input
categoryOrder: 9
---

# Navigation Mode

Navigation Mode is an **experimental, opt-in** vim-style keyboard layer over the file list. With it
on, single keys move the selection, jump to the top/bottom, extend a range, and cut/copy/paste — no
mouse needed.

## Enabling it

Open **Settings → Navigation Mode → Keyboard Navigation Mode (vim-style)** and turn it on. It's
**off by default**, so nothing about how the file list behaves changes until you opt in.

## Modes

Navigation Mode has two modes:

- **NORMAL** — the resting mode. Motions move the selection one item at a time; they don't extend
  a range.
- **VISUAL** — entered with `v`. Motions extend a range selection from where you entered visual
  mode to wherever you move next, the same way visual mode works in vim.

A small badge shows which mode you're in whenever Navigation Mode is active.

## Bindings

| Keys | Action |
|---|---|
| `h` `j` `k` `l` | Move left / down / up / right |
| `gg` / `G` | Jump to the first / last item |
| `v` | Enter or exit visual mode (extend a range selection) |
| `d` / `y` / `p` | Cut / copy / paste the selection |
| `/` | Start filtering |
| `:` | Start a command |
| `Esc` | Exit visual mode back to normal |

A number typed before a motion repeats it — `3j` moves down 3 items, `3gg` jumps to line 3, the
same as vim's count prefix.

A quick cheatsheet listing these same bindings is available from within Navigation Mode itself, so
you don't need to come back to this page to remember a binding mid-session.

## Turning it off

Press `Esc` to drop back to normal mode at any point, or flip the **Keyboard Navigation Mode
(vim-style)** switch in Settings back off to leave Navigation Mode entirely and return to plain
mouse-and-keyboard use of the explorer.
