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
- **Preview** — select a file to see it in the side pane; text is editable in place. Text files (source
  code and plain text alike, e.g. `.txt`) get a richer read view: syntax highlighting when a language is
  recognised, a **line-number gutter**, **fold triangles** on collapsible blocks (click to hide a range,
  click again to expand — it shows a "⋯ N lines" marker while collapsed), faint **indent guides**, and a
  **minimap** down the right edge whose highlighted box tracks your scroll position — click or drag the
  minimap to jump. Select-all and copy still yield the whole file as plain text. Above the code, the
  **symbol outline** strip lets you jump straight to a function or class when one is recognised.
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
  list of edits (resize, **compress**, convert, rotate, flip, **watermark**, rename, strip metadata) and run
  them across every file at once. Pick an operation and its settings, then click **+ Add** to put it on the
  list — nothing runs until you add at least one op. They apply in the order shown, and a live preview lists
  each file's planned output and a one-line summary as you build the list. (Compress re-encodes JPEGs at the
  chosen quality; watermark overlays a chosen image at a corner/opacity, and is optional — no image, no
  watermark.) Non-image or un-decodable files (e.g. a corrupt or placeholder image) are **skipped**: after the
  run the dialog stays open and lists each skipped file with the reason, so nothing is silently dropped. Runs
  **non-destructively** by default (writes new files alongside the originals)
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

## Add metadata columns

The details view can show extra columns beyond Name/Date/Type/Size — pixel **dimensions** for images,
**duration** for audio/video, **page count** for PDFs, and typed tag columns (Title/Artist/Track/Year for
audio and video, Title/Author/Subject for PDFs).

Open **Manage columns…** from the command palette, or click the small columns icon at the right end of
the details-view header. Pick columns from **Available** with **+**; reorder or remove an active column
with the ↑ / ↓ / × buttons next to it in **Active**. Values fill in as you scroll (only the rows on
screen are fetched), and a column with no value for a given row shows a dim **—** rather than a blank
that could be mistaken for a fetch still in progress.

Click a metadata column's header to sort by it — dimensions, duration, and page counts sort **numerically**
(smallest/shortest first ascending), not alphabetically on the formatted text. Clicking again reverses the
direction, same as the built-in columns.

The active column set, its order, and each column's width are remembered **per folder** — reopening a
folder brings its columns back exactly as you left them, and a folder you've never customized shows just
the four built-ins.

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

## Organizing a folder

**Organize this folder…** (command palette, or Tools menu) sorts a folder's files into subfolders by a
rule you pick — a safe **propose → review → apply** flow: nothing moves until you click Apply.

1. Pick a rule: **By kind** (Images / Documents / Audio / Video / Archives / Code / Other), **By
   extension** (a subfolder per uppercased extension, e.g. `PNG`), **By year modified** (a subfolder per
   4-digit year), or **By size** (Tiny / Small / Large buckets).
2. The dialog previews every proposed move, grouped by destination subfolder, with a running count —
   switching rules updates the preview live. An empty folder (or a folder with no eligible files) says so
   plainly instead of showing an empty list.
3. Click **Organize** to apply. It always **takes a checkpoint of the folder first**, then creates each
   destination subfolder and moves the files in — so the whole reorganization is a **single undo**: the
   result panel names the checkpoint and offers a one-click way to open **Checkpoint & rollback** and
   revert it. A file that can't be moved (locked, or a name collision at the destination) is reported by
   name rather than failing the whole run.

Only files move — subfolders already in the listing are left alone, and nothing is ever deleted.

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

## Agent Watch

Agent Watch is a mode layered over the explorer, not a separate app or a toggle you hunt for. It appears
only when there's something to show: launch a coding agent from the AI Console, then navigate the
explorer into (or already be sitting in) that agent's project folder. An **"Agent Watch — \<name\>"**
strip appears above the file list, with a live dot, a running feed of recent change chips (created /
modified / renamed / removed, fading over time), and a **Log** button.

**Off means off.** Leave the folder, or let the agent's session end, and the strip disappears along with
everything it drives — no watched session means no Agent Watch, no background watcher, and no cost, on
top of the plain explorer.

Click **Log** to open the Agent Watch drawer on the right — a tabbed activity panel: **Live**, **Replay**,
**Cost**, **Radar**, **History**.

### Live

The default tab: a durable, newest-first log of every filesystem action the agent has taken this session
— created, edited, deleted, moved. Click any row to jump the explorer to its containing folder. A
**consulted files** list above the log surfaces files the agent has *read* (not just changed) this
session — reads aren't visible to a filesystem watcher, so they're parsed from the agent's own output and
shown as a dimmer, distinct signal from actual writes. A row whose edit captured a before/after diff shows
a small `+added −removed` line count; hover or focus it to peek the diff inline, or click **Open full
diff** for a side-by-side view.

### Replay

Scrub back and forth through the session's recorded activity with a slider — plus jump-to-start,
jump-to-end, step back/forward, and play/pause at 0.5×/1×/2×/4× speed. Moving the scrubber reconstructs
the folder listing exactly as it stood at that instant: files that had been created, modified, or removed
by that point in time are shown, read-only, right in the drawer. A **"Show in file pane"** checkbox
graduates that same reconstruction into the main explorer pane itself — pausing its live listing until you
switch the toggle off or leave the Replay tab. If a path was edited more than once during the session, its
diff at an earlier scrub point isn't retained; the drawer says so rather than showing a diff from the
wrong moment.

### Cost

Live, per-session token and cost usage: input/output/total tokens and a USD estimate for every reporting
session, plus files touched, edit count, churn (bytes changed), and wall-clock time, with per-minute and
per-file throughput ratios once there's enough data to derive them. These figures are scraped from the
agent's own printed output — **advisory, not a billing record**.

### Radar

Flags paths that two or more distinct actors have touched within a short window — the "two agents, or an
agent and you, editing the same file" signal — as a list of paths with the actors involved and a relative
timestamp; click a row to jump there. It's deliberately worded as an **activity overlap**, not a
"conflict": a filesystem watcher can't prove two touches came from genuinely unrelated processes rather
than the same agent revisiting its own file, so an overlap involving an unresolved actor carries a hedge
note. A **Competing renames** section below it separately flags same-source or same-destination rename
divergences across distinct actors.

### History

A cross-session rollup read from a small local history log, loaded once the first time you open the tab:
totals (sessions, cost, tokens, time, files touched, churn), throughput ratios, per-model and per-agent
breakdowns with each one's share of total cost, and a bar chart of cost or tokens per day. Same advisory
framing as the Cost tab — best-effort figures, never billing.
