---
title: Finding Similar Images
order: 18
category: Search & Discovery
categoryOrder: 4
---

# Finding Similar Images

Photo libraries fill up with **near-duplicates**: the same picture saved twice, a resized copy, a
re-encoded export, or a lightly edited version. They aren't byte-for-byte identical — so the exact
[duplicate finder](#) misses them — but they *look* the same and waste space. **Find similar images**
catches exactly those.

Open it from the **Tools** menu (*Find similar images…*) or the **command palette**
(Ctrl/Cmd+Shift+P → *Find similar images…*). Both are scoped to the folder you're currently in.

## How it works

1. **Scan.** The app walks the current folder (and subfolders) and reduces every image to a compact
   *perceptual fingerprint* (a dHash). Two images with fingerprints close enough to match are grouped
   as near-duplicates — even if their bytes, format, or size differ.
2. **Review.** Each group is shown **side by side with thumbnails**, so you can see at a glance which
   copies are the same picture. Click any thumbnail to jump straight to that file in the explorer.
3. **Clean up (optional).** Tick the copies you don't want and click **Move to Bin**.

## Safety — nothing is deleted by surprise

Removing photos is destructive, so the cleanup is deliberately cautious:

- **Recoverable, always.** Removed images go to the system **Recycle Bin / Trash** — never a hard
  delete. You can restore anything you change your mind about.
- **A checkpoint first.** Before a bulk move the app takes a **checkpoint** of the folder, so the
  action is reversible beyond the Bin too.
- **At least one copy is always kept.** The app never lets you remove *every* image in a group — the
  **Move to Bin** button is disabled the moment your selection would wipe out a whole group.
- **Nothing is pre-selected.** The scan selects *nothing* by default. You opt in to each removal. The
  **Select extras** shortcut ticks every image except the first in each group — a safe starting point
  that always keeps one.

## Similar images vs. exact duplicates

Use the **exact** [duplicate finder](#) when you want byte-identical copies (safe to delete freely).
Use **similar images** when you want visually-alike photos — recompressed, resized, or tweaked — that
exact matching can't see. The two are complementary.
