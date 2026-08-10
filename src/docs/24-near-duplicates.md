---
title: Near-Duplicate Documents & Folders
order: 24
category: Search & Discovery
categoryOrder: 4
---

# Near-Duplicate Documents & Folders

Beyond exact byte-for-byte duplicates, folders accumulate items that are *almost* the same: a
document re-saved under a new name, an old draft next to its revision, or two folders that hold
nearly the same set of files. This feature finds those near-matches for **documents** and
**folders** — it's the text/folder counterpart to **Finding Similar Images** (its own page in this
library), which covers photos specifically via a different dialog.

Open either scan from the **Tools** menu or the **command palette** (Ctrl/Cmd+Shift+P):

- **Find similar documents…** — compares text content (a SimHash fingerprint) to group documents
  that read as near-duplicates of each other, even when their file names or exact bytes differ.
- **Find near-identical folders…** — compares folder contents (a Jaccard similarity over the files
  each folder holds) to group folders that overlap almost entirely.

Both are scoped to the folder you're currently in (and its subfolders).

## Reviewing results

1. **Scan.** The app walks the current folder (and its subfolders) and reports how many files/folders
   it scanned, then groups the near-matches it found.
2. **Review.** Each group lists its members with their name and location. Click any item to jump
   straight to it in the explorer.
3. **Select and clean up.** Tick the copies you don't want, or use **Select extras** to tick every
   item except the first in each group — a safe starting point that always leaves one behind.

## Safety — a keeper guard, not just a confirmation

Removing a document or folder is destructive, so cleanup here carries the same rails as
**Finding Similar Images**:

- **Nothing is selected by default.** The scan never pre-ticks anything — you opt in to every item.
- **At least one copy per group is always kept.** You can never select every member of a group — the
  moment your selection would wipe out an entire group, **Move to Bin** disables itself until you
  uncheck something. This is the *keeper guard*: it stops you from accidentally deleting every copy
  of a document or every one of two near-identical folders, leaving nothing behind.
- **Recoverable, always.** Removed items go to the system **Recycle Bin / Trash**, never a hard
  delete — you can restore anything you change your mind about.
- **A checkpoint first.** Before a bulk move, the app takes a best-effort **checkpoint** of the
  folder, so the action is reversible beyond the Bin too. A checkpoint failure never blocks the
  (already recoverable) move.

## Documents vs. folders vs. images

- **Similar documents** compares text content, useful for drafts, notes, and readmes that were
  copied and lightly edited.
- **Near-identical folders** compares the *set of files* two folders contain, useful for spotting a
  folder that was duplicated wholesale (a backup copy, an old export) alongside the original.
- **Similar images** (its own page in this library) is a separate dialog using a perceptual image
  fingerprint, not text or folder-content comparison — reach for it specifically for photos.
