---
title: Metadata Studio
order: 25
category: Organizing & Tagging
categoryOrder: 3
---

# Metadata Studio

**Metadata Studio** is an editable, tabbed inspector for the metadata embedded inside a file
itself — ID3/Vorbis tags on audio, EXIF/IPTC/XMP on images, and document/video metadata — as opposed
to the app's own tag store (see **Native Metadata Bridge**, its own page in this library, for that —
a different, OS-level feature).

Select one or more files and open **Metadata Studio…** from the **right-click context menu** (next to Properties) or the command palette
(Ctrl/Cmd+Shift+P). The first selected file is the one shown and edited; selecting more than one
enables the batch controls described below.

## Viewing metadata

Every readable field is listed under a tab for its group (Audio, Image, IPTC, XMP, Document, Video —
only the groups the file actually has appear). Formats the app can only *read* today show every field
as view-only, with a **View only** badge; formats it can also write (MP3/FLAC today) show their
editable fields as normal text inputs, with any remaining read-only fields displayed alongside them.

## Editing and saving

Type into any editable field to stage a change — the field's row highlights while the edit is
pending. Nothing touches disk until you click **Save**:

- **Checkpoint before save.** The first time you save, the app takes a best-effort **checkpoint** of
  the containing folder *before* writing anything, because a metadata write has no built-in undo of
  its own. The checkpoint is taken once per save (not per field, not per file), and a checkpoint
  failure never blocks the write — it's a bonus safety net, logged quietly, not a gate.
- **Apply to all.** With more than one file selected, check **Apply to all N selected** to write your
  edits to every selected file instead of just the first. The checkpoint still fires once before the
  whole batch.

- **A failed save never damages the original.** Edits are written to a temporary file first and only
  then moved into place, in one step. If the save fails part-way through — the disk fills up, the file
  is locked by another program — your file is left exactly as it was, never half-rewritten, and the
  temporary file is cleaned up. (This is about *failed saves*, not about power cuts: like most desktop
  apps, the app doesn't force the change all the way down to the physical disk before reporting success.)
- **If the app is killed mid-save, your file is still safe, and the leftover cleans itself up.** The
  original is untouched either way. An app that is force-quit or crashes during a save can leave a stray
  file next to yours whose name ends in `.cpe-tmp` — it's the half-written copy, and nothing else will
  ever read it. You don't need to delete it by hand: the next time that same file is saved, the app
  notices it's stale and removes it automatically. (It lingers if that particular file is never saved
  again. It can also linger in a folder holding a very large number of files — the app only checks part
  of such a folder on each save, so it may never look at that leftover. It is harmless either way, and
  safe to delete by hand.)
- **What that safety costs, so you can judge it.** Writing a new file and moving it into place means the
  saved file is technically a *new* file. Almost everything attached to the old one is now carried across
  with it — but not quite everything, and the exceptions are listed rather than glossed over.

  Carried across, so you don't lose it:

  - **Permissions.** On Linux and macOS a file you'd made private (`0600`) stays private, and an
    executable file keeps its executable bit. On Windows the file's security settings come with it too.
  - **Windows file attributes and alternate data streams**, including **Hidden** and the
    `Zone.Identifier` mark that records a file was downloaded from the internet.
  - **Extended attributes on Linux and macOS** — which is where macOS keeps **Finder tags** and its
    "downloaded from the internet" quarantine flag. One caveat worth knowing: these are copied one at a
    time and on a best-effort basis, so an attribute the system won't let the app re-apply (a security
    label managed by the OS itself, for instance) is dropped without a warning. That's still far better
    than before, when *every* extended attribute was lost on every save, but it isn't a guarantee the way
    the two entries above are.
  - If any of these can't be read before the save starts, the save is **refused** and nothing is written,
    rather than handing you back a file that is more open than the one you saved.

  Still not carried across:

  - **Ownership on Linux and macOS.** A saved file belongs to whoever saved it. (If that changes the
    owner, a `setuid`/`setgid` bit is deliberately dropped rather than silently re-pointed at you.)
  - **A save can still fail where a plain save would have worked** if another program has the file open —
    on Windows it reports that the file is in use by another process. Nothing is written or damaged when
    that happens; close the other program and save again.
  - **A program that had the file open while you saved keeps reading the old contents** until it reopens
    it. This is inherent to saving safely this way, and is how most editors behave.
  - **Other hard links to the file keep the old metadata** (see below).
  - Saving is also a little slower than a plain write, because carrying all of the above across costs a
    few extra milliseconds. You won't notice it on a single save.
  - The preview pane's plain-text editor makes the **opposite** trade — see **Explorer → Files → Saving an
    edited file**. The two agree about symlinks, which is the part that could otherwise corrupt something;
    they differ only in how the bytes are put down.

## Symlinked files

If the file you opened is a **symlink** — the usual arrangement for a music library organised into
playlist folders — Metadata Studio edits the file the link points at, and the link stays a link.

Because the save writes a temporary file next to the file it is really editing, that temporary file
appears in the folder the **link points at** — the library folder, not the playlist folder you're looking
at. A failed save cleans it up immediately; if the app is killed before that, the next save of the same
file removes it automatically, the same as everywhere else — see above.

If the link is **broken** (it points at something that isn't there any more), the save is refused and
tells you which link it was and that nothing was written. The alternative would be to quietly replace
your link with a new file, which is worse than an error message.

A **hard link** is a different thing and behaves differently: the file you opened receives the edit as
normal, but the other name for it keeps the old metadata, because the save writes a new file and moves
it into place. That's how every editor that saves this way behaves.

## Batch operations

Two buttons appear once more than one writable file is selected, next to **Apply to all**:

- **Strip editable metadata** — stages every editable field to empty, across the whole selection when
  Apply to all is checked. This only *stages* the clear; nothing is written until you click Save.
- **Copy from first** — stages the first file's current editable values as edits and automatically
  turns **Apply to all** on, so Save pushes the first file's metadata onto every other selected file.
  Copying the first file onto itself is a no-op; the point is propagating its values to the rest of
  the selection.

## Undoing an edit before you save

Every edit can be discarded without touching disk, right up until you click Save:

- **Per-field revert** — a small revert icon appears next to any field you've changed; clicking it
  discards just that field's pending edit and restores the value loaded from disk.
- **Reset all edits** — discards every pending edit across the whole selection in one click, restoring
  everything to the values that were loaded. Like per-field revert, this never writes to disk and
  never touches the checkpoint — it's a purely client-side discard.

## Read-only files

If a file's format has no write codec yet, the dialog still shows its metadata for reference, but
every field is view-only and there's no Save button — Metadata Studio never pretends it can write a
format it can't.
