---
title: The Explorer
order: 3
category: Explorer
categoryOrder: 2
---

# The Explorer

The explorer is the core of the app and is tuned to stay **fast, small, and predictable**.

## Navigation

- **Address bar** — type or paste a path; press Enter to go.
- **Back / Forward / Up** — move through history and up the tree.
- **Tabs** — Ctrl+T opens a new tab; each tab remembers its own folder and history.
- **Sidebar** — Home, Favorites (pin folders you use often), and drives with free-space bars. Each
  section (Explore, Quick access, Drives, Favorites, Tags, Smart Folders) has a header you can click to
  **collapse** it and reclaim vertical space; your choices persist across sessions.

## Files

- **Progressive loading** — folders stream in: the first rows appear almost immediately and the rest fill
  in as they're read, so even a huge or slow (network) folder stays interactive instead of blocking on a
  blank pane. Changing folders mid-load cleanly abandons the previous listing.
- **Preview** — select a file to see it in the side pane; text is editable in place. Source-code files get
  a richer read view: syntax highlighting, a **line-number gutter**, **fold triangles** on collapsible
  blocks (click to hide a range, click again to expand — it shows a "⋯ N lines" marker while collapsed),
  faint **indent guides**, and a **minimap** down the right edge whose highlighted box tracks your scroll
  position — click or drag the minimap to jump. Select-all and copy still yield the whole file as plain
  text. Above the code, the **symbol outline** strip lets you jump straight to a function or class.
- **Thumbnails** — in the **icons** view, image files (JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF) show a real
  downscaled thumbnail instead of a generic icon. They load lazily as tiles scroll into view, so a folder
  of hundreds of photos stays responsive; non-image files and the list/details views are unchanged.
- **Gallery view** — a fourth view mode (View menu or the command palette) that lays photos out as large
  tiles on a wide grid — a light-table for a folder of images, with bigger thumbnails than the icons view.
- **Quick-look** — press **Space** on a selected image to open a full-screen preview; **←/→** step through
  the folder's images and **Esc** (or Space again) closes it.
- **Sort & filter** — order by name, size, date, or type; filter the list by a pattern.
- **Search** — three complementary tools:
  - a quick **name filter** for the current folder (Ctrl+F; supports `*`/`?` wildcards and `{a,b}`
    brace groups),
  - **Find files by name** (Ctrl+P) to search the whole tree below the current folder for a name or
    glob and jump straight to a hit — results stream in as they're found, so a big tree lists matches
    progressively instead of waiting for the whole walk,
  - **Search in files** (Ctrl+Shift+F) to grep folder contents, with matches highlighted in each result
    line.

  Your recent queries autocomplete in each.

  Both name searches understand the same glob syntax: `*` matches any run of characters, `?` exactly one,
  and a **brace group** `{a,b,c}` matches any one of the comma-separated alternatives — so `*.{jpg,png,gif}`
  finds all three image types at once. Wildcards work inside a group, and groups combine (`{img,pic}.{jpg,png}`).
  A brace with no comma inside (or an unmatched brace) is treated as a literal character.
- **Tags & labels** — right-click a file or folder and choose **Tags…** to attach free-text tags and a
  single colour label. Tagged rows show their tags as small chips, and a labelled row gets a colour dot
  and a soft accent bar in its label's colour. Tags persist across sessions; untagged items look exactly
  as before.
- **Smart folders** — a saved search surfaced as a virtual folder. Right-click a tag in the sidebar's
  **Tags** section and choose **Save as smart folder**; it appears under **Smart Folders** in the
  sidebar. Opening it lists every file carrying that tag, wherever they live, and the view refreshes
  automatically as you add or remove that tag. It's a read-only view (open a file's real location to
  change it); rename or delete a smart folder by right-clicking it. Smart folders persist across
  sessions and cost nothing when you have none.
- **Selection** — multi-select with Shift/Ctrl; the status bar shows the count and total size.
- **Operations** — copy, cut, paste, rename, delete (to the trash, restorable), new folder, and batch
  rename. Filesystem operations skip entries they can't read rather than failing the whole listing.
- **Batch media** — right-click 2+ selected image files and choose **Batch media…** to queue an ordered
  list of edits (resize, convert, rotate, flip, rename, strip metadata) and run them across every file at
  once. Add ops one at a time — they apply in the order shown — and a live preview lists each file's
  planned output and a one-line summary as you build the list; non-image files in the selection are
  skipped with a notice. Runs **non-destructively** by default (writes new files alongside the originals)
  and shows a live progress bar while it applies.
- **Drag and drop** — drag any selection onto a folder row or a sidebar place/drive to move or copy it.
  The action follows the OS convention: dropping **within the same drive moves**, **across drives copies**,
  and you can force it with a modifier — hold **Ctrl** to copy, **Shift** to move. Dragging more than one
  item shows a small badge with the count. You can also drag files **in** from the desktop or your system
  file manager: drop them on the window and they're **copied** into the folder under the cursor (or the
  current folder), with a highlight while you drag over. Drops run through the transfer manager, so a large
  one shows the same progress panel as a paste.
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

## Dual-pane (commander) mode

Turn on **Dual pane** from the command palette (Ctrl/Cmd+K → *Dual pane*) to split the file list into two
independent panes side by side — a classic two-pane file-manager layout for copying and moving between
folders. It's **off by default**; toggling it off returns to the normal single-pane view unchanged, and
the layout (plus each pane's folder) is remembered across restarts.

The **active pane** carries an accent ring — click a pane, or press **Tab**, to switch which one is active.
Commander keys act on the active pane's selection:

| Key | Action |
|---|---|
| **F5** | Copy the selection to the other pane's folder |
| **F6** | Move the selection to the other pane's folder |
| **Ctrl+U** | Swap the two panes' folders |
| **Tab** | Switch the active pane |

The same actions (plus **Mirror path to other pane**) are in the command palette under *View*. Copies and
moves run through the transfer manager, so large operations show progress and can be cancelled.

## Shell integration — "Open in Cross-Platform Explorer"

**Settings › Shell integration** adds an **"Open in Cross-Platform Explorer"** item to the operating
system's right-click menu — on folders, the folder background (empty space inside a folder), and drives —
so you can jump into CPE from anywhere the OS offers a context menu. It's **off until you turn it on**, and
turning it off removes every entry it added, with nothing left behind.

The toggle registers under your **user** account only, so enabling it never needs administrator rights.
It's available on **Windows** today; the macOS and Linux equivalents are on the way, and the toggle shows a
"coming soon" note on those platforms until then.
