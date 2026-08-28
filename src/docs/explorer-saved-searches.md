---
title: Saved Searches
order: 50
category: Explorer
categoryOrder: 2
---

# Saved Searches

A **saved search** is a named, structured query — built the same way as **Select by…** — captured as its
own virtual folder in the sidebar. Unlike a [smart folder](explorer-smart-folders) (which is always a
single tag, matched anywhere in the tag store), a saved search can combine extension, name pattern, size,
age, or "is a folder", and it always re-scans **recursively from the one real folder it was captured in**
— there's no whole-computer index to search "everywhere" the way a tag can appear anywhere.

## Building and saving one

Both **[Select by…](organizing-select-by)** and **Save search…** open the same criteria dialog (Command Palette; both are
palette-only — no context-menu entry, no keyboard shortcut — and both are disabled while you're in Home,
an open archive, a smart folder, or another saved search):

1. Pick a criterion kind: **Extension**, **Name (glob)**, **Size**, **Older than** (N days), **Newer
   than** (N days), or **Is folder**.
2. Fill in its fields (comma-separated extensions like `ts, md, png`; a glob like `*.min.js`; min/max
   bytes; a day count; or a folder/file checkbox).
3. Click **Select** to apply the criterion to the current folder's selection immediately (this is the
   plain "Select by…" behaviour, unrelated to saving), or click **Save search…** to reveal a name field
   inline — a second click (or Enter in the name field) actually saves it. **Save search…** from the
   palette jumps straight to that revealed name field instead of making you find the button first.

The condition is captured **exactly as built** — one condition, combined with `match: "all"` (the only
sensible choice with a single condition) — and the search remembers the folder you were in when you saved
it as its **root**. That's the folder it recursively scans from every time you open it afterwards, not
wherever you happen to be standing when you reopen it. A search saved before this app tracked a root (or
whose captured folder no longer resolves) falls back to whichever folder is open at the moment you open it.

## Opening it

Click the saved search's row under **Saved Searches** in the sidebar (hidden entirely with none saved).
The app recursively scans the captured root **up to 12 levels deep** and filters the flattened tree
through the saved condition, showing every matching file or folder, however deep, in one flat list.

## It stays live

Like a smart folder, an open saved search recomputes automatically when the filesystem changes anywhere
under its root — the app watches that one root directory and recomputes, debounced ~300ms, so a burst of
changes (an extraction, a git checkout) collapses into a single recompute. There's no live re-evaluation
on a *tag* change, since a saved search's conditions never involve tags.

## What you can't do inside one

Same enforcement as a smart folder: it's a **read-only view**. Delete, Cut, Copy, Rename, and Paste are
all blocked with an explanatory notice ("This is a saved search — a read-only view. Open a file's real
location to change it.") — including Copy, even though copying wouldn't actually change anything.
Properties, Reveal, and navigating into a listed subfolder all still work. **Select by…** and
**Save search…** themselves are unavailable while a saved search is open — exit back to a real folder
first. To do that: click any crumb in the breadcrumb, or open the address bar (Ctrl+L) and press Enter —
it opens showing the real folder you came from, and the saved search sits on top of that folder rather
than replacing it, so Enter on its own takes you back there.

## Managing saved searches

Right-click a saved search in the sidebar: **Rename** (a non-empty, changed value commits; anything else
just closes the popover), **Delete** (instant, no confirmation — it only removes the saved query, never a
real file), and **Move up / Move down** to reorder the list (disabled at the ends).

## Worked example

You want a running view of every large video file under your `Projects` folder, wherever it's nested.

1. Navigate to `Projects`, open the Command Palette, run **Save search…**.
2. Set the criterion to **Size**, min bytes `104857600` (100 MB) — leave max blank.
3. Type a name, `Big Videos`, and confirm.
4. Later, from anywhere in the app, click **Big Videos** under Saved Searches — it re-scans `Projects`
   fresh and lists every file over 100 MB found anywhere beneath it, updating as new large files land.

## Limits / notes

- **One condition, `match: "all"`** — the underlying model supports multiple AND/OR-combined conditions,
  but neither "Select by…" nor "Save search…" ever builds more than one, so that richness isn't reachable
  from the UI today.
- **Six criterion kinds exist**, not just extension/size/date — Extension, Name (glob), Size, Older than,
  Newer than, and **Is folder** (matching folders themselves, not just files).
- **Recursion is capped at 12 levels deep** from the captured root — anything nested deeper is invisible
  to the search.
- **Palette-only, and only from a real folder** — no context-menu entry, no shortcut, and both "Select
  by…" and "Save search…" are disabled while you're in Home, an archive, a smart folder, or another saved
  search.
- **Read-only, strictly enforced**, exactly like a smart folder — see [Smart Folders](explorer-smart-folders)
  for the same caveat about Copy also being blocked.
- **The captured root is fixed at save time** — moving or renaming that folder later doesn't re-target the
  search; it falls back to whichever folder happens to be open when a stale root can't be resolved.
