---
title: Smart Folders
order: 49
category: Explorer
categoryOrder: 2
---

# Smart Folders

A **smart folder** is a saved [tag](explorer-tags) query that behaves like a folder in the sidebar but
actually lists every item carrying that tag **anywhere in the tag store**, regardless of which real
folder each one lives in. It re-evaluates live, so it's always current.

## Creating one

There's exactly one path in: right-click a tag in the sidebar's **Tags** section → **Save as smart
folder**. The new smart folder is named after the tag and queries that same tag — there's no dialog to
pick a different name up front (rename it afterwards if you want something else) and no way to build a
smart folder from anything richer than one tag. For a multi-condition query (extension, size, age, "is a
folder", …), see [Saved Searches](explorer-saved-searches) instead — a different, complementary feature.

## Opening it

Click the smart folder's row under **Smart Folders** in the sidebar (hidden entirely when you have none —
it costs nothing until you save your first one). The main pane switches to a **read-only virtual listing**:
every path in the tag store that currently carries the folder's tag, sorted, each one **statted fresh**
from disk at open time. A tagged path that's since been deleted is silently skipped rather than shown as
an error, the same tolerant behaviour the rest of the app's filesystem commands use.

## It stays live

While a smart folder is open, the listing recomputes automatically on two independent triggers:

- **Tag changes** — add or remove the smart folder's tag from any file, anywhere, and the listing
  updates reactively (it's driven by the same reactive tag store the Tags section itself reads).
- **Disk changes** — the app arms a filesystem watcher on the parent directory of every currently-matched
  file (watching the tagged files themselves wouldn't work — the underlying watcher only arms on
  directories) and recomputes, debounced ~300ms, when something changes in one of those directories. A
  burst of changes (a multi-file move, a git checkout) collapses into a single recompute rather than one
  per event.

## What you can't do inside one

Opening a real file's location is genuinely the only way to change it — the view enforces this rather than
just documenting it. Delete, Cut, Copy, Rename, and Paste are all blocked with an explanatory notice
("This is a smart folder — a live view of tagged files. Open a file's real location to change it."); note that
even **Copy** is blocked, not just the operations that would actually mutate something. **Properties**,
**Reveal**, and navigating into a subfolder shown inside the smart folder all still work, since none of
those change anything. "Select by…" and "Save search…" are unavailable while a smart folder is open (they
only work in a real, on-disk folder) — exit back to a real folder first.

## Managing saved smart folders

Right-click a smart folder in the sidebar for a small popover:

- **Rename** — type a new name and confirm (or press Enter). An empty or unchanged value just closes the
  popover without renaming.
- **Delete** — removes the saved smart folder. No confirmation dialog; nothing on disk is touched either
  way, since a smart folder is only ever a saved query.
- **Move up / Move down** — reorder it within the Smart Folders list. Both are disabled at the ends of the
  list.

## Worked example

You tag every invoice you send with `invoice`, spread across a dozen client folders.

1. Tag three files across different folders with `invoice` (see [Tags](explorer-tags)).
2. Right-click `invoice` in the sidebar's Tags section → **Save as smart folder**.
3. Click the new **invoice** row under Smart Folders — all three files appear together, regardless of
   which client folder they actually live in.
4. Tag a fourth file `invoice` from anywhere in the app; the open smart folder picks it up without you
   doing anything else.

## Limits / notes

- **Single-tag queries only.** A smart folder can't combine two tags, exclude a tag, or add any other
  criterion (size, extension, date) — for that, use [Saved Searches](explorer-saved-searches).
- **No naming dialog on creation** — the tag's own name is used; rename afterwards if you want something
  else.
- **Read-only, strictly enforced** — Delete/Cut/Copy/Rename/Paste are all blocked, including Copy, which
  doesn't actually mutate anything but is blocked anyway. Open the real file's location to act on it.
- **A tagged-but-deleted file just disappears** from the listing rather than showing an error — there's no
  "missing file" indicator.
- **No confirmation on delete** — removing a saved smart folder is instant (it only deletes the saved
  query, never any real file).
