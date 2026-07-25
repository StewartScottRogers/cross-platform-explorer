---
title: Disk usage & Space analyzer
order: 11
category: Explorer
categoryOrder: 2
---

# Disk usage & Space analyzer

The **Space analyzer** turns a folder into an interactive **treemap** — every tile is a child, sized by
how much disk space it takes up (recursively). It's the fast way to answer "what's filling this drive?"
without clicking through folder after folder.

## Opening it

- Click the **disk icon** in the toolbar to analyze the folder you're currently viewing.
- Or press the **?** on the analyzer's own header to jump straight to this page.

The scan runs only while the analyzer is open — the plain explorer stays fast when it's closed.

## Reading the treemap

- **Bigger tile = more space.** The largest child fills the most area, so space hogs jump out
  immediately.
- **Folders** are drillable; **files** are leaves. Hover any tile for its name, exact size, and share of
  the parent.
- The **Largest** list on the right ranks the top items for a quick scan when tiles get small.

## Navigating

- **Click a folder tile** (or a row in the Largest list) to drill into it — the treemap redraws for that
  subfolder.
- Press **Up** in the header to climb back toward the parent. Navigation is instant: once a folder has
  been scanned it's cached, so re-visiting a parent or sibling never re-walks the disk.
- Click a **file** to jump back to it in the explorer.

## Tips

- Start at a drive root to see which top-level folders dominate, then drill toward the culprit.
- A very large tile with a small name is usually a single big file (a VM image, a video, a database) —
  hover to confirm before you act on it.
