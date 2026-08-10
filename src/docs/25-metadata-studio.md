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
