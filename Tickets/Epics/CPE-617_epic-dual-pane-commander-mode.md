---
id: CPE-617
title: "EPIC: Dual-pane commander mode"
type: Task
status: Proposed
priority: Low
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Add an optional **two-pane (commander) layout** — two independent folder views side by side, à la
Norton Commander / Total Commander / Midnight Commander — with a keyboard-driven workflow for
copying/moving between the active and the opposite pane. A power-user mode that makes the most common
two-location task (move things from A to B) dramatically faster.

## Why

Copying between two folders in a single-pane explorer means tabs, back-and-forth, and clipboard juggling.
The dual-pane idiom — "source pane, target pane, F5 to copy, F6 to move" — is the fastest known workflow
for that, and it has a devoted user base. It's a pure **additive mode**: toggle it off and the app is the
familiar single-pane explorer, so the fast/small/predictable core is untouched.

## Rough scope (areas, not child tickets)

- A layout toggle: single-pane ⇄ dual-pane (split view), with a clear "active pane" indicator and Tab to
  switch focus between panes.
- Each pane is a full, independent explorer view (its own path, history, selection, sort) — ideally the
  existing view component instantiated twice rather than a fork.
- Commander keybindings: copy/move to the *other* pane (F5/F6 or configurable), swap panes, mirror the
  other pane's path, sync navigation.
- Integration with the transfer manager ([[CPE-613]]) so cross-pane copy/move uses the same queue + UX.
- Remember the layout choice + pane paths across sessions (reuse the settings + tab persistence).

## Open questions (resolve at activation)

- Can the current explorer view be cleanly instantiated twice (state isolation), or does it assume a
  single global instance? (This determines the effort — investigate first.)
- Vertical vs horizontal split, and how it interacts with the preview/details pane and Agent Watch.
- Keybinding scheme: adopt Total-Commander defaults or the app's own idiom? Make it configurable?
- Does dual-pane belong to a "power user" settings gate, or is it always one toggle away?

## Definition of Done

- A toggle splits the window into two independent panes with a visible active-pane indicator + Tab-switch.
- Copy/move between panes works via keyboard and the shared transfer queue.
- Single-pane mode is completely unchanged (default), and the layout choice persists.
- No measurable cost to single-pane startup/memory when dual-pane is off.
