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
- **Density** — the rows icon near the right end of the toolbar toggles between **Comfortable** (the
  default) and **Compact**, tightening row height and padding across the tab strip, sidebar, and toolbar
  itself so more fits on screen. It takes effect **instantly** (no dialog, nothing to apply) and the
  button reflects whichever mode is active. Your choice persists across restarts.
- **Sidebar** — Home, Favorites (pin folders you use often), and drives with free-space bars. Each
  section (Explore, Quick access, Drives, Favorites, Tags, Smart Folders, Saved Searches, Network,
  Agents) has a header you can click to **collapse** it and reclaim vertical space; your choices persist
  across sessions.
- **Reorder sidebar sections** — grab a section's grip handle and drag it up or down to rearrange the
  sidebar to taste; a thin accent line shows where it will land. The new order persists across restarts
  and is independent of each section's collapsed/expanded state — reordering never changes what's open.
  The grip is also keyboard-actionable: focus it (Tab) and press **Arrow Up / Arrow Down** to move the
  section a slot at a time, no drag required. A **Reset section order** control at the bottom of the
  sidebar puts every section back in its original position.
- **Drives update live** — plug in a USB stick or external disk and it appears in the **Drives**
  section within a few seconds, with its free-space bar and eject button; unplug (or eject) one and it
  drops out. No relaunch or manual refresh needed.
- **Eject a removable drive** — a **removable** drive (a USB stick or external disk) shows a small
  **eject** button on its sidebar row, and a **Safely eject** item in its right-click menu. Choosing it
  flushes and dismounts the volume so Windows reports it safe to remove. Fixed/system and network drives
  never show the control and can never be ejected. If files are still open on the drive the eject is
  refused with a clear message — close them and try again, and nothing is unmounted.

## Home screen

The Home screen has **Quick access** tiles up top and a tabbed lower section with four pills:

- **Recent** — files you've opened recently. **Favorites** — items you've starred. **Folders** — folders
  you've visited (an MRU). Right-click any row for the usual actions (Open, Copy, Rename, Properties,
  and a view-native *Remove from …* that prunes just the list entry without touching the file).
- **Shared** — network locations. It lists the network drives your OS already has mapped (on Windows,
  your mapped drive letters and their `\\server\share` targets; on macOS/Linux, mounted SMB/NFS shares)
  plus any locations you add yourself. Use **＋ Add network location** to type a `\\server\share` or
  `smb://host/share` address; it's remembered across sessions. Right-click a share for **Open**,
  **Copy path**, and either **Disconnect** (for a mapped drive) or **Remove** (for a location you added).
  An unreachable share degrades gracefully — Open reports a clear error while Disconnect/Remove still
  work. The list loads when you open the tab (never on a background timer), so an offline server never
  slows the rest of Home.

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
- **Saving an edited file** — the preview editor answers the symlink questions exactly the way Metadata
  Studio does, so the two never disagree about the same file:
  - **Symlinks are followed.** If the file you opened is a symlink, the save edits the file the link points
    at and the link stays a link.
  - **A broken symlink is refused.** If the link points at something that isn't there any more, the save
    stops and tells you which link it was and that nothing was written — rather than quietly creating a new
    file at the far end of a broken link, which is what a plain save would do and is not something you'd
    ever find again.
  - **Everything else about your file is kept.** The save rewrites the file in place, so its permissions,
    its owner, its Windows attributes (including the "downloaded from the internet" mark), any alternate
    data streams and any hard links to it all survive the edit untouched — and a program that has the file
    open keeps working. Some editors save by writing a new file and renaming it over yours, which quietly
    loses all of that; this one doesn't.
  - **The trade-off: a save that is interrupted can leave the file part-written.** If the app is killed or
    the disk fills up half-way through writing, you can be left with a truncated file. Metadata Studio
    makes the opposite trade (see its page) because a media file is harder to retype than a text file. If
    the file you're editing is precious and unversioned, save early.
- **Log preview** — `.log` files get their own read-only view: each line is tinted by its detected
  severity (Error/Warn/Info/Debug/Trace, or a plain "Other" for lines with no detectable level), with
  chips to filter the view down to just the levels you want. A big real-world log (an incident's
  `CBS.log`, `dism.log`, a service's rolling log, …) opens straight to its **last** portion rather than
  being refused — the note above the log body says exactly which byte range you're looking at (e.g.
  "Showing the last 256 KB of this 15.4 MB file"), and a **Load earlier** button pages further back a
  bounded chunk at a time; **Back to latest** jumps straight back to the tail. Reading always stays
  bounded to that one chunk, never the whole file, so opening a multi-megabyte log is as fast as a small
  one.
- **Thumbnails** — in the **icons** view, image files (JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF) show a real
  downscaled thumbnail instead of a generic icon. They load lazily as tiles scroll into view, so a folder
  of hundreds of photos stays responsive; non-image files and the list/details views are unchanged.
  **PDFs** show their first page and **videos** (MP4, MOV, MKV, WebM, AVI, M4V, MPG/MPEG, WMV, FLV) show a
  representative frame, the same way. Both ride on optional native rendering (a bundled pdfium library for
  PDFs, a bundled ffmpeg for videos) that ships with the app — if either is ever missing or fails to load,
  that file quietly falls back to its plain type icon instead of erroring, exactly like an undecodable
  image.
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
  as before. See [Tags](explorer-tags) for the full editor, the sidebar's click-to-filter tag section, and
  import/export.
- **Smart folders** — a saved tag query surfaced as a virtual, read-only folder under **Smart Folders** in
  the sidebar, listing every file carrying that tag wherever it lives and refreshing live. See
  [Smart Folders](explorer-smart-folders) for how to create, open, and manage one.
- **Saved searches** — a structured search (extension, name pattern, size, age, or "is a folder", built
  the same way as **Select by…**) saved as its own virtual, read-only folder under **Saved Searches** in
  the sidebar. See [Saved Searches](explorer-saved-searches) for the full criteria list, how the captured
  root works, and its live refresh.
- **Selection** — multi-select with Shift/Ctrl; the status bar shows the count and total size.
- **Operations** — copy, cut, paste, rename, delete (to the trash, restorable), new folder, and batch
  rename. Filesystem operations skip entries they can't read rather than failing the whole listing.
- **Securely delete…** — right-click a file (or a multi-file selection, no folders) and choose
  **Securely delete…** to overwrite its bytes before removing it, instead of an ordinary delete. This is
  **permanent and non-recoverable** — unlike Delete, it never goes to the Recycle Bin/Trash, so there's no
  undo. Pick an overwrite scheme (zero-fill, random, DoD 5220.22-M, or Gutmann) and confirm with the
  danger button. Be honest with yourself about what this buys you: overwriting is **best-effort, not a
  guarantee** — on an SSD, wear-levelling can leave the original cells untouched; on a copy-on-write
  filesystem (APFS/Btrfs/ZFS), old data can survive in snapshots; and copies in backups or filesystem
  journals are never touched either way. For an actual guarantee, use full-disk encryption or an
  encrypted vault instead.
- **New ▸** — right-click empty space, a folder, or a drive and open the **New** submenu to create a new
  item in the right place (the current folder, the clicked folder, or the drive root). Beyond **Folder**
  and **Text file** you can create **Markdown, Rich Text, JSON, YAML, XML, HTML, CSS, JavaScript, Python,
  CSV**, and a **Compressed (zipped) Folder**. The file is created empty (ready for you to fill in),
  except Rich Text (a minimal valid `.rtf` stub) and the zipped folder (a valid empty `.zip` archive),
  and it drops straight into inline rename like a new text file.
- **Archives** — right-click a selection to **Compress to ZIP** or **Compress to .tar.gz**, or choose
  **Compress with password…** to set a password that protects the archive (AES-256) — you'll need it
  again to open the archive later. A `.zip`/`.tar.gz`/`.tar`/`.tgz`/`.7z` file can be browsed like a
  folder (double-click to look inside, read-only) or unpacked: **Extract** drops its contents into a new
  subfolder right there, and **Extract to…** lets you pick any destination folder instead. `.iso` and
  `.rar` files can be browsed the same read-only way too, but have no Extract action. Opening or
  extracting a password-protected archive prompts for its password; a wrong password re-prompts rather
  than failing silently. Compress and extract run through the same transfer queue as copy/move: a large
  archive shows live progress in the bottom-corner operations panel and stays cancellable instead of
  freezing the window. See [Archives](explorer-archives) for the full format matrix, safety checks, and
  limits.
- **Batch media** — right-click 2+ selected image files and choose **Batch media…** to queue an ordered
  list of edits (resize, **compress**, convert, rotate, flip, **watermark**, rename, strip metadata) and run
  them across every file at once. Pick an operation and its settings, then click **+ Add** to put it on the
  list — nothing runs until you add at least one op. They apply in the order shown, and a live preview lists
  each file's planned output and a one-line summary as you build the list. (Compress re-encodes JPEGs at the
  chosen quality; watermark overlays a chosen image at a corner/opacity, and is optional — no image, no
  watermark.) Non-image or un-decodable files (e.g. a corrupt or placeholder image) are **skipped**: after the
  run the dialog stays open and lists each skipped file with the reason, so nothing is silently dropped. Runs
  **non-destructively** by default (writes new files alongside the originals)
  and shows a live progress bar while it applies. See [Batch Media](explorer-batch-media) for every
  operation's options, output naming, and failure handling.
- **Drag and drop** — drag any selection onto a folder row or a sidebar place/drive to move or copy it.
  The action follows the OS convention: dropping **within the same drive moves**, **across drives copies**,
  and you can force it with a modifier — hold **Ctrl** to copy, **Shift** to move. Dragging more than one
  item shows a small badge with the count. You can also drag files **in** from the desktop or your system
  file manager: drop them on the window and they're **copied** into the folder under the cursor (or the
  current folder), with a highlight while you drag over. Drops run through the transfer manager, so a large
  one shows the same progress panel as a paste. To drag a selection **out** to another application (drop
  files into your OS file manager, an email, a chat window, and so on), **hold Alt while you start the
  drag** — a plain drag stays inside the app for folder/sidebar drops, and Alt-drag hands the real files to
  the OS. Ctrl/Shift still choose copy vs move. Alt-drag also works **inside an open archive**: since those
  rows are a read-only view into the archive rather than real files on disk, Alt-dragging one out extracts
  it to a temp file first, then drags that — a plain drag inside an archive stays inert (archive rows never
  drop internally, since there's nothing to move).
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

The details pane and the Properties dialog show size, dates, type, and (where relevant) an on-demand
SHA-256 checksum. For image files the Properties dialog also shows **dimensions** and any embedded
**EXIF** — camera, lens, date taken, ISO, aperture, exposure, and focal length — omitting whatever the
photo doesn't carry. Properties is the deeper of the two — see [Properties](explorer-properties) for every
field it shows, where each one comes from, and exactly how it differs from this details pane.

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

### Set as default file manager

The same section has a **Set as default file manager** control. It's honest about what modern Windows
allows: an app **cannot** silently make itself the default — Windows always leaves that choice to you.
So **Register** does two things: it records Cross-Platform Explorer with Windows as a *candidate* (an
app entry under your user account, plus a folder-open command), and it opens **Settings › Default apps**
so you can confirm the choice yourself. CPE never claims to have flipped the default for you.

**Unregister** completely withdraws that registration — everything it added is removed, with nothing left
behind. (It doesn't change any default you may have picked; Windows owns that.) Like shell integration,
this is Windows-only for now and shows a "coming soon" note elsewhere.

## Agent Watch

Agent Watch is a mode layered over the explorer, not a separate app or a toggle you hunt for: launch a
coding agent from the AI Console, navigate into (or already be sitting in) its project folder, and an
**"Agent Watch — ⟨name⟩"** strip appears above the file list with live change chips and a **Log** drawer
(Live / Replay / Cost / Radar / History tabs, plus checkpoint-backed revert from a bad edit). See
[Agent Watch](explorer-agent-watch) for the full drawer walkthrough, the sidebar Agents section, the
session-history export tool, and exactly what "off means off" does and doesn't cover.
