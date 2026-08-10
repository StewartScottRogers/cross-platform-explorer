---
title: Properties
order: 47
category: Explorer
categoryOrder: 2
---

# Properties

**Properties** is the modal dialog that shows everything the app knows about a file or folder — size,
dates, attributes, a checksum, image EXIF, encoding/type inspection, and (opt-in) native OS tags — in one
place, fetched fresh each time you open it.

## How to open it

- **Keyboard: Alt+Enter**, with a file, folder, or multi-selection active.
- **Right-click → Properties** on a selected row. This item is always present — it isn't disabled for a
  multi-selection or a folder.
- **Command palette** → *Properties* (Alt+Enter shown as its shortcut).
- A few other surfaces open the same dialog on a synthetic single-item selection: right-clicking **empty
  space** in a folder (Properties for the folder itself), a **drive** tile/sidebar row, and a **Home
  screen** row (Recent/Favorites/Folders/Shared).

## Single item vs. multiple

- **One item selected** — the full detail view described below.
- **Two or more selected** — an aggregate summary instead: **Folders** count, **Files** count, and
  **Size of files** (the sum of file sizes only — a selected folder contributes nothing to that total,
  since folder sizes require a separate recursive walk). If the selection includes any folders, a note
  says so: *"Folder contents are not included in the total."* Nothing here is async — it's arithmetic over
  the selection you already had.

## Every field (single item)

| Field | Shown for | Where it comes from |
|---|---|---|
| Icon + name | any | the selection itself |
| **Type** | any | the selection itself |
| **Location** | any | the full path, as plain monospace text |
| **Size** | files: immediately, from the selection. Folders: **"Calculating…"** while a recursive byte count runs, then the total (or **"Unavailable"** on error) | a backend recursive-size scan, folders only |
| **Created** / **Modified** | any | a backend metadata fetch, run once on open |
| **Attributes** | any | the same fetch — a comma list of **Read-only**/**Hidden**, or **None** |

Every field below this line is **best-effort**: if its fetch fails, the row is simply omitted — nothing
turns the whole dialog into an error state except a failure on the core Created/Modified/Attributes fetch
itself.

### Images

For a recognised image file, dimensions and any embedded EXIF, each row shown only if that value is
actually present in the file:

**Dimensions**, **Camera**, **Lens**, **Date taken**, **ISO**, **Aperture**, **Exposure**, **Focal
length**.

### File inspection

For any non-folder file: **Encoding**, **Line endings**, **File type** — a best-effort sniff of the file's
real content, plus a separate warning row, **Type mismatch**, shown only when the file's content doesn't
match what its extension claims (e.g. an executable saved with a `.jpg` extension).

### Checksum (SHA-256)

Shown for files only (no row for a folder). **Never computed automatically** — a **Compute** button runs
it on demand, showing "Computing…" while it works. It is **not cached**: closing and reopening Properties
on the same file starts from the button again, even moments later. Once computed:

- **Copy** puts the digest on the clipboard (a brief "Copied" confirmation).
- Paste a digest you already have into the **verify** field to compare — it's whitespace/case-insensitive,
  showing **✓ Match** or **✗ No match** as you type. Nothing is sent anywhere; the comparison is local.

### Text stats

For a recognised text/code file only, a **Count** button (same on-demand, not-cached pattern as the
checksum) reports line/word/character counts.

### Native metadata (opt-in)

Only appears when **Settings → Native metadata bridge** is turned on (off by default), and only for a
single item. A bordered section below the main fields, titled *"Native metadata"* plus the OS store's name
(e.g. "NTFS alternate data streams"):

- **Tags** — shown as chips, or *"No tags"*.
- **Label** — a colour-label name, or *"None"*.
- A **Pull** button that reads the OS-native store for this path and merges what it finds into the app's
  tag store (non-destructively — it only adds, never removes).

**Read this carefully:** the Tags/Label chips shown here are read from the app's **own** tag store — the
same one the **Tags…** editor uses — not a live read of the native store. If you've tagged a file inside
the app but never clicked Pull (or Push, from the Tags… editor), what you see here may not match what
Finder/Explorer/your file manager's own tags actually show. Click **Pull** to bring the two back in sync.
See [Native Metadata Bridge](17-native-metadata) for the full picture, including **Push** (only available
from the Tags… editor, not from here).

## What Properties does *not* show or do

- **No app tag/label editing.** The chips above are read-only and only appear via the native-bridge
  section; editing your own tags and colour label is the separate **Tags…** context-menu item.
- **No attribute editing.** Read-only and Hidden are shown as plain text with no toggle here — changing
  them (plus a Unix permission mode on macOS/Linux) is a **different, palette-only** tool ("Attributes…" —
  search the command palette), not linked from inside Properties.
- **No duplicate detection.** Properties does not check whether the file has duplicates elsewhere —
  that's the separate "Find duplicate files" tool (Tools menu / command palette), which has no connection
  to this dialog.
- **No size-on-disk or Accessed date** — only the logical byte size and Created/Modified.
- **No live updates.** Properties fetches everything once, when it opens. If the file changes on disk
  while the dialog is still open, nothing here refreshes — close and reopen it to see current values.

## How this differs from the Details pane and the Preview pane

The right-hand side pane has a **Preview | Details** toggle — easy to confuse with Properties, so here's
the three-way split:

| | **Details pane** (side pane tab) | **Preview pane** (side pane tab) | **Properties** (Alt+Enter dialog) |
|---|---|---|---|
| What it shows | A small, synchronous field list | The file's actual **content** (image, text, hex, archive listing, media player, …) | A richer field list, fetched on demand |
| Updates live? | Yes — tracks the selection as you click around | Yes — tracks the selection | **No** — a one-time snapshot per open |
| Fields | Type, Size (files only), **Date modified**, Path | (content-driven, not a field list) | Type, Location, Size, **Created and Modified**, Attributes, image EXIF, checksum, inspection, native metadata |
| Backend calls | None — pure formatting of data already loaded | Per-type preview loaders | Several, in parallel, on open |

The Details pane is deliberately the *lightweight* one — it's what you see just by clicking a file, no
extra step. For a multi-selection it shows the same Folders/Files/size-of-files summary as Properties and
ends with a hint: *"Folder contents aren't included in the size. Press Alt+Enter for full properties."* —
that hint is the app's own pointer from Details to Properties.

## Worked example

You want to confirm a downloaded file matches its published checksum before trusting it.

1. Select the file, press **Alt+Enter**.
2. Under **SHA-256**, click **Compute** — wait for the digest to appear.
3. Paste the checksum you copied from the download page into the verify field.
4. **✓ Match** confirms the file is byte-identical to what was published; **✗ No match** means it isn't —
   re-download rather than trust it.

## Limits / notes

- **Snapshot, not live** — reopen the dialog to see current values if the file changed while it was open.
- **Checksum/text-stats are opt-in and never cached** — every dialog open starts from the Compute/Count
  buttons again, even for the same file.
- **SHA-256 only** — no MD5/SHA-1/CRC32 option.
- **No editing surface** — Properties is read-only except for the native-metadata **Pull** button; use
  **Tags…** for your own tags/label and the palette's **Attributes…** tool for Read-only/Hidden/permission
  changes.
- **Native metadata chips can be stale** relative to the actual OS store until you click **Pull** — see
  above.
- **No duplicate detection and no size-on-disk/Accessed date**, despite what you might expect from a
  traditional OS file-properties dialog.
