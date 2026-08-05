---
title: Declutter
order: 23
category: Explorer
categoryOrder: 2
---

# Declutter

**Declutter** scans a folder for likely junk — files that are almost always safe to clear out but
that nothing else in the app flags on its own. It's a rules-based, review-first surface: it *finds*
candidates and lets you decide, it never deletes anything on its own.

Open it from the **Tools** menu (*Find clutter…*) or the **command palette** (Ctrl/Cmd+Shift+P →
*Find clutter…*). The scan is always scoped to the folder you're currently in.

## What it finds

Declutter groups its findings by reason:

- **Empty files** — zero-byte files that carry no content.
- **Installers** — setup/installer packages (`.exe`, `.msi`, `.dmg`, `.pkg`, and similar) that have
  already served their purpose once the app is installed.
- **Temporary / partial downloads** — in-progress or interrupted downloads (`.crdownload`, `.part`,
  `.tmp`, and similar patterns) left behind by a browser or download manager.
- **Backups / leftovers** — editor and application backup files (`~`-suffixed, `.bak`, and similar)
  that outlived whatever they were backing up.

## Reviewing results

1. **Scan for clutter.** The app walks the current folder and reports every match, grouped under its
   reason with a count.
2. **Review.** Click any item's name to jump straight to that file in the explorer, so you can check
   it before deciding.
3. **Select and clean up.** Nothing is pre-selected — you tick the checkbox next to each item you
   actually want gone. **Move to Bin** only clears the items you've ticked, and it's disabled until at
   least one is selected.

## Safety — nothing auto-deletes

Declutter only ever *surfaces* candidates; removal is always a deliberate, manual step:

- **Nothing is selected by default.** The scan never pre-ticks anything — you opt in to every item.
- **Recoverable, always.** Selected items go to the system **Recycle Bin / Trash**, never a hard
  delete, so anything you change your mind about can be restored.
- **A checkpoint first.** Before moving anything, the app takes a best-effort **checkpoint** of the
  folder, so the cleanup is reversible beyond the Bin too. A checkpoint failure never blocks the
  (already recoverable) move — it just proceeds without one.

## Unlike Near Duplicates, no keeper guard

Declutter's findings are independent junk, not grouped copies of the same thing — there's no "keep at
least one" rule to enforce, because there's nothing to keep a copy *of*. You can select and bin every
finding in one pass if you're confident they're all junk. Contrast this with
**Near-Duplicate Documents & Folders** (its own page in this library), where a keeper guard stops you
from removing every copy in a group.
