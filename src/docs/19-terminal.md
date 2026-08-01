---
title: Terminal
order: 19
category: Explorer
categoryOrder: 2
---

# Terminal

The explorer can dock a real, interactive terminal at the bottom of the window — a live shell running
next to the file listing, not a separate app window.

## Opening it

Click **Terminal** in the command bar (above the file list). The panel opens with one tab, running your
operating system's default shell **rooted at the current folder** — the same folder you're browsing.

## Tabs

- **+** opens another tab, also rooted at the current folder.
- Click a tab to switch to it; click its **×** to close it.
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

## Resizing

Drag the panel's bottom-right corner to make it taller or shorter.
