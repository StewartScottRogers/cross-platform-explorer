---
title: The Explorer
order: 3
---

# The Explorer

The explorer is the core of the app and is tuned to stay **fast, small, and predictable**.

## Navigation

- **Address bar** — type or paste a path; press Enter to go.
- **Back / Forward / Up** — move through history and up the tree.
- **Tabs** — Ctrl+T opens a new tab; each tab remembers its own folder and history.
- **Sidebar** — Home, Favorites (pin folders you use often), and drives with free-space bars.

## Files

- **Progressive loading** — folders stream in: the first rows appear almost immediately and the rest fill
  in as they're read, so even a huge or slow (network) folder stays interactive instead of blocking on a
  blank pane. Changing folders mid-load cleanly abandons the previous listing.
- **Preview** — select a file to see it in the side pane; text is editable in place.
- **Thumbnails** — in the **icons** view, image files (JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF) show a real
  downscaled thumbnail instead of a generic icon. They load lazily as tiles scroll into view, so a folder
  of hundreds of photos stays responsive; non-image files and the list/details views are unchanged.
- **Gallery view** — a fourth view mode (View menu or the command palette) that lays photos out as large
  tiles on a wide grid — a light-table for a folder of images, with bigger thumbnails than the icons view.
- **Quick-look** — press **Space** on a selected image to open a full-screen preview; **←/→** step through
  the folder's images and **Esc** (or Space again) closes it.
- **Sort & filter** — order by name, size, date, or type; filter the list by a pattern.
- **Search** — three complementary tools:
  - a quick **name filter** for the current folder (Ctrl+F; supports `*`/`?` wildcards),
  - **Find files by name** (Ctrl+P) to search the whole tree below the current folder for a name or
    glob and jump straight to a hit,
  - **Search in files** (Ctrl+Shift+F) to grep folder contents, with matches highlighted in each result
    line.

  Your recent queries autocomplete in each.
- **Tags & labels** — right-click a file or folder and choose **Tags…** to attach free-text tags and a
  single colour label. Tagged rows show their tags as small chips, and a labelled row gets a colour dot
  and a soft accent bar in its label's colour. Tags persist across sessions; untagged items look exactly
  as before.
- **Selection** — multi-select with Shift/Ctrl; the status bar shows the count and total size.
- **Operations** — copy, cut, paste, rename, delete (to the trash, restorable), new folder, and batch
  rename. Filesystem operations skip entries they can't read rather than failing the whole listing.
- **File transfers** — a paste that **copies** runs through the transfer manager: a small panel appears
  in the bottom-right showing the progress bar, file count, and any errors, and lets you **cancel**
  mid-copy. It stays hidden when nothing is transferring. (Moves are near-instant same-folder-volume
  renames, so they don't need the panel.) If a copy would overwrite files that already exist, a prompt
  asks once how to handle the whole batch — **Replace**, **Keep both** (auto-numbered), or **Skip**.

## Command palette

Press **Ctrl+Shift+P** to open the command palette — a searchable list of every action in the app
(navigation, file operations, view and sort options, tools, and settings). Type to filter, use the
arrow keys and Enter to run, or Esc to dismiss. Actions that don't apply right now (e.g. Paste with an
empty clipboard) appear greyed out. The palette also lists your **recently-visited folders** so you can
jump back to one by name. It's the fastest way to reach anything without hunting through menus.

## Details & properties

The details pane and the Properties dialog show size, dates, type, and (where relevant) checksums and
duplicate detection. For image files the Properties dialog also shows **dimensions** and any embedded
**EXIF** — camera, lens, date taken, ISO, aperture, exposure, and focal length — omitting whatever the
photo doesn't carry.
