---
title: Link Forge (New Link…)
order: 40
category: Power Tools
categoryOrder: 7
---

# Link Forge (New Link…)

**New Link…** creates a **symlink**, **hardlink**, or (Windows only) a **directory junction** pointing at
an existing file or folder — a way to make one piece of data appear in more than one place without
duplicating it on disk.

## When to use each kind (vs. a copy or shortcut)

| Kind | Can target | Available on | Use it when… |
|---|---|---|---|
| **Symlink** | A file or a folder | Windows*, macOS, Linux | You want a reference that can point anywhere (even a different drive) and is easy to spot as "not the real file" — e.g. a `current` link that always points at the latest versioned folder. |
| **Hardlink** | A file only (no filesystem supports a directory hardlink) | Windows, macOS, Linux — but only **within the same volume/drive** as the target | You want the file to genuinely exist in two folders with **zero extra disk space**, indistinguishable from the original — editing through either path changes the same underlying data. |
| **Junction** | A folder only | **Windows only** | You want a folder that appears in a second location and don't want to fight Developer Mode / admin elevation — a junction needs neither. |

\* On Windows, creating a symlink normally requires **Developer Mode** or running elevated — see *Limits*
below. If you just want a reference to a folder on Windows without that hurdle, prefer a **Junction**
instead.

This is a different tool from an ordinary copy: a copy duplicates bytes and the two files are
independent from that point on; a link keeps one underlying file (hardlink) or one reference to a path
(symlink/junction).

## How to open it

- **Command palette** → **"New link…"** (keywords: symlink, hardlink, shortcut).
- **Right-click an empty area** of the file list → **New ▸ New Link…**.
- There is no dedicated keyboard shortcut — both openers above create the link **in the folder you're
  currently viewing**; there's no way to target a different destination folder from the dialog itself.

## Options

| Field | Default | Notes |
|---|---|---|
| **Link type** | **Symlink** | Choose **Symlink**, **Hardlink**, or **Junction** (the Junction option only appears on Windows). |
| **Target** | empty | The existing file/folder the link should point at. Type a path or click **Browse…** for a native picker. The picker defaults to picking a **file** (hardlinks can only ever target a file, and a file target is the common symlink case too); switching **Link type** to **Junction** flips the picker to **folder** mode, since a junction can only target a directory. A symlink to a folder can still be typed by hand into the field. |
| **Link name** | empty | The new link's file/folder name, created inside the current folder. |

Choosing **Junction** shows an inline hint that the target must be a folder.

## Actions

| Action | Effect |
|---|---|
| **Create** | Validates both fields are filled, then creates the link. Disabled while a creation is in progress. |
| **Cancel** / **Esc** | Closes the dialog without creating anything. |
| Enter (in either field) | Same as clicking **Create**. |

There is no separate "keyboard shortcut" for Create beyond Enter while the dialog is focused.

## Worked example

You keep dated project folders (`project-2024-01`, `project-2024-02`, …) and want a stable
`project-current` folder that always points at the newest one, without moving anything:

1. Open the command palette → **New link…**.
2. Leave **Link type** as **Symlink**.
3. Click **Browse…**, pick `project-2024-02` as the **Target**.
4. Set **Link name** to `project-current`.
5. Click **Create**. `project-current` now appears in the folder list; opening it shows the contents of
   `project-2024-02`. When `project-2024-03` is ready, delete `project-current` and repeat with the new
   target — the link is just a pointer, so removing it never touches the real folder.

## Limits / notes

- **Windows symlinks need a privilege you may not have.** Creating a symlink on Windows requires
  **Developer Mode** or an elevated (admin) process. If creation fails for this reason, the dialog shows
  the OS error **plus** a note that Developer Mode/elevation is needed — the app never pops a silent
  elevation prompt on your behalf; you decide whether to enable Developer Mode and try again. A
  **Junction** sidesteps this entirely for folder targets.
- **Hardlinks are same-volume only.** Because a hardlink is two directory entries pointing at the same
  on-disk data, the target and the new link must be on the **same drive/volume** — the OS itself rejects
  a cross-volume hardlink, and the app surfaces that error as-is rather than falling back to a copy.
- **Junctions are Windows-only** and only ever target a **directory** — pointing one at a file is
  rejected up front with a clear message rather than a confusing OS error.
- **Broken links.** If a link's target is later moved or deleted, the link becomes "broken" (visible in
  the file list). Repairing a broken link is a separate flow (right-click a broken link) not covered by
  this page.
- **Not undoable as a single click-to-remove step.** Creating a link is not pushed onto the
  [Undo](safety-undo) stack the way a rename/move is — to remove one, delete it like any other file
  (deleting a symlink/junction removes only the pointer; deleting a hardlink removes that one directory
  entry and leaves the underlying data alone as long as another link or the original still references
  it).
- A hardlink is **indistinguishable from the original file** in the file list — there's no "this is a
  hardlink" badge the way broken symlinks are flagged, since, from the filesystem's point of view, both
  directory entries genuinely are the same file.
