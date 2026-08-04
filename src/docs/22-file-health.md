---
title: File Health
order: 22
category: Explorer
categoryOrder: 2
---

# File Health

**File Health** is a tabbed panel that surfaces the file-inspection detectors built for the explorer —
facts and hazards the plain listing doesn't show. It grows one tab at a time; today it wires the first
one: **dangling and cyclic symlinks**.

Open it from the **Tools** menu (*Find dangling links…*) or the **command palette**
(Ctrl/Cmd+Shift+P → *Find dangling links…*). Both are scoped to the folder you're currently in.

## Dangling links

A symlink is **dangling** when it points at a target that no longer exists — often left behind after the
target was moved, renamed, or deleted. A symlink is **cyclic** when following its chain of targets loops
back on a path already seen, so it can never resolve to real content either way.

1. **Scan.** The app walks the current folder (and subfolders) looking for symlinks in either state.
   Results **stream in live** as they're found, so a large tree starts showing hits immediately instead
   of making you wait for the whole walk to finish.
2. **Review.** Each flagged link is shown with its name, location, and a **Missing target** or
   **Cyclic link** badge. Click any entry to jump straight to that file in the explorer.

This tab is **read-only** — nothing is deleted or modified. It's a review surface, not a cleanup tool;
use the explorer's normal delete/move actions on anything you want to fix once you've found it.

## More tabs coming

The panel is built to grow: future slices add tabs for the explorer's other file-health detectors
(disguised file types, archive expansion-ratio warnings, orphaned sidecar files, empty-folder cascades)
alongside this one, so File Health becomes a single place to review everything the explorer has noticed.
