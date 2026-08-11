---
title: Keyboard Shortcut Reference
order: 44
category: Appearance & Input
categoryOrder: 9
---

# Keyboard Shortcut Reference

This page is a **complete reference** — every built-in keyboard shortcut in the app, grouped by area,
with what each one actually does. If you want to *change* a binding instead of just looking one up, see
[Keyboard shortcuts](36-keyboard-shortcuts) (the rebind dialog); if you've turned on the vim-style
[Navigation Mode](37-navigation-mode), its motions are a separate layer with their own key set, listed
on that page rather than here.

## Opening the quick cheat sheet in-app

Press **?** to pop open a read-only shortcuts list right over the explorer — the same groups and rows
as this page, for a quick glance without leaving what you're doing. It fires whenever the file list has
keyboard focus; while you're typing in a text field (search, the address bar, an in-progress rename) a
bare `?` types a literal question mark there instead, same as any other printable key. **F1** opens this
Documents library to the page for whatever section is currently focused.

## Navigation

| Keys | Action |
|---|---|
| `Alt+←` | Back |
| `Alt+→` | Forward |
| `Alt+↑` | Up one folder |
| `Backspace` | Up one folder |
| `F5` | Refresh |
| `Ctrl+L` | Edit address (type a path) |
| `Alt+D` | Edit address (type a path) |
| `Ctrl+F` | Search the current folder |
| `Ctrl+P` | Find files by name (recursive) |
| `Ctrl+Shift+F` | Search inside files (content search) |
| `Ctrl+K` | Instant Search — every indexed folder, any drive |
| `Enter` | Open the selected item |
| Type a name | Jump to the matching item |

## Tabs

| Keys | Action |
|---|---|
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Shift+T` | Reopen last closed tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |

## Selection

| Keys | Action |
|---|---|
| `Ctrl+A` | Select all |
| `↑` / `↓` | Move selection (arrow keys) |
| `Shift+↑` / `Shift+↓` | Extend selection |
| `Home` / `End` | Jump to the first / last item |
| `Esc` | Clear selection |

## File actions

| Keys | Action |
|---|---|
| `Ctrl+C` | Copy |
| `Ctrl+X` | Cut |
| `Ctrl+V` | Paste |
| `Ctrl+D` | Duplicate |
| `Ctrl+Shift+D` | Add to Drop Stack |
| `Ctrl+Z` | Undo |
| `F2` | Rename |
| `Delete` | Delete to Recycle Bin / Trash |
| `Shift+Delete` | Delete permanently |
| `Ctrl+Shift+N` | New folder |
| `Ctrl+Shift+C` | Copy as path |
| `Alt+Enter` | Properties |

## View

| Keys | Action |
|---|---|
| `Alt+P` | Toggle the details panel |
| `Ctrl+Shift+O` | Pop out the preview |

## General

| Keys | Action |
|---|---|
| `Ctrl+Shift+P` | Command palette — find and run any action |
| `F1` | Documentation for the current section |
| `?` | Show the quick shortcuts cheat sheet |

## Macros

| Keys | Action |
|---|---|
| *(user-configured)* | Run a saved macro over the selection — bind a Ctrl/Alt hotkey per macro in the Macro Library |

Macros don't ship with a default chord — each one gets its own binding only once you assign it in the
[Macros](organizing-macros) library, so there's nothing to list here beyond "you set it."

## These are remappable

Every row above except **Enter** (open), **Esc** (clear selection), and **?** (cheat sheet) can be
rebound to a different chord, or cleared entirely, from **Settings → Keyboard shortcuts → Customize
shortcuts…**. See [Keyboard shortcuts](36-keyboard-shortcuts) for how rebinding, conflict warnings, and
resetting to defaults work. If you've changed a binding, the chord shown there — not the default listed
on this page — is the one that actually fires.

## Limits / notes

- **Some rows list two chords for one action.** Up one folder answers to both `Alt+↑` and `Backspace`;
  edit address answers to both `Ctrl+L` and `Alt+D`. Only one of a pair can currently be changed via the
  rebind dialog (the dialog tracks a single chord per action) — the other stays fixed.
- **A few keys are context-sensitive rather than plain shortcuts** and are excluded from rebinding for
  that reason: `Enter` behaves differently depending on what's focused, `Esc` clears whatever the current
  context needs cleared, and `?` opens the cheat sheet whenever the file list has keyboard focus — but
  types a literal `?` instead while a text field (search, address bar, an in-progress rename, …) is
  focused, same as any other printable key.
- **Navigation Mode uses a different key set entirely.** With the opt-in vim-style
  [Navigation Mode](37-navigation-mode) turned on, single letters like `h`/`j`/`k`/`l` and `d`/`y`/`p`
  take on their own vim-style meanings layered over the file list — those bindings live on that page,
  not this one, and don't share this reference's chords.
- **Macros have no default chord.** Unlike every other group, a macro's hotkey exists only once you set
  one for it in the Macro Library — there's no built-in binding to look up until then.
