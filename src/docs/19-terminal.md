---
title: Terminal
order: 19
category: Power Tools
categoryOrder: 7
---

# Terminal

The explorer can dock a real, interactive terminal at the bottom of the window — a live shell running
next to the file listing, not a separate app window.

## Opening it

Click **Terminal** in the command bar (above the file list). The panel opens with one tab, running your
operating system's default shell **rooted at the current folder** — the same folder you're browsing.

## Tabs

- **+** opens another tab, also rooted at the current folder.
- Click a tab to switch to it; click its **×** to close it (or Tab to the **×** and press Enter/Space).
- Closing the panel (the **×** at the right of the tab strip) closes every open tab's shell — nothing is
  left running in the background once the panel is off.

## Shell picker

The dropdown next to **+** chooses which shell the *next* new tab launches (System default, PowerShell,
Command Prompt on Windows; bash/sh/zsh elsewhere). A tab already running can't change shells — open a new
tab to try a different one.

## Follow folder

The **Follow folder** checkbox, when on, `cd`s the active tab's shell to match whenever you navigate to a
different folder in the explorer. It's off by default — a terminal only follows navigation when you ask
it to, so a command you're in the middle of typing is never disturbed by browsing elsewhere.

**While it's on, the `cd` is sent regardless of what the shell is doing.** There's no check for whether
the shell is actually idle at a prompt — if you're mid-command, or running something in the foreground
(an editor, a dev server, a REPL, ...), navigating the explorer types `cd <folder>` into that program
instead, exactly as if you'd typed it yourself at the wrong moment. Detecting "is this shell at a prompt
right now" reliably isn't practical from the explorer side (it would mean pattern-matching the shell's own
prompt text, which varies per shell/theme/customization), so this is a deliberate tradeoff rather than a
bug: turn Follow folder on only while you're not mid-command, or turn it off before running something
that shouldn't receive stray keystrokes.

## Resizing

Drag the panel's bottom-right corner to make it taller or shorter.
