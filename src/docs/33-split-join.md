---
title: Split & Join Files
order: 33
category: Power Tools
categoryOrder: 7
---

# Split & Join Files

Chunk a large file into fixed-size numbered parts for transferring or storing it somewhere that caps
individual file sizes (an old floppy/CD-sized limit, a FAT32 volume, an email attachment cap, a chat
upload limit), then rejoin the parts back into the original later — the classic orthodox-commander
"Split file" / "Combine files" utility.

## Splitting a file

Right-click a single **non-empty** file and choose **Split file…**.

- **Part size** — pick a preset (1.44 MB floppy, 650 MB CD, or 4 GB − 1 byte, the FAT32 maximum file
  size) or choose **Custom…** and enter a size in MiB or GiB.
- **Output folder** — a native Browse picker chooses where the parts + manifest are written; it defaults
  to the source file's own folder.

Splitting writes `<name>.001`, `<name>.002`, … alongside a small `<name>.split-manifest.json` manifest
that records the original filename, size, part count, and a whole-file SHA-256 checksum. The split
refuses to overwrite a pre-existing manifest or part in the output folder rather than silently mixing
runs together — remove the old ones first if you want to re-split into the same folder.

On success the dialog shows a summary: part count, per-part size, and the output folder.

## Joining parts back together

Right-click any of the numbered part files (e.g. `<name>.001`), or the `<name>.split-manifest.json`
manifest itself, and choose **Join parts…**.

- The dialog previews the manifest (part count, total size) when it can read it, and pre-fills the
  **output path** with the original filename in the same folder as the parts.
- **Output path** — a native Browse (save) picker lets you write the rejoined file anywhere else.

Joining reconstructs the file from its parts and verifies the result against the manifest's SHA-256
checksum. If a part is missing, short, or corrupted, or the checksum doesn't match, the join fails with a
clear error and any partial output is removed — never a silent partial file. Joining also **refuses to
overwrite** a file that already exists at the output path; pick a different path (or delete the existing
file first) and try again.

### Links at an output path are refused, not followed

If the output path you pick — for a join, or for any part or manifest name a split would write — is a
**symlink, junction or shortcut-style link**, the operation refuses and names the link instead of writing
through it. Both kinds of link are refused, including a **broken** one whose target no longer exists.

That matters because following a link would write your file to the link's *target*, which is a different
path from the one you chose, and the operation would then report success about a file that isn't where
you asked for it. A link that points nowhere is the worst case: the app would create the missing target
and tell you the join worked.

The same rule protects the cleanup. When a join fails part-way it deletes the partial file it created —
but only if that really is a file it created. A link sitting at the output name is never deleted, so a
failed join can no longer remove something you made while reporting an unrelated problem.

If you meant to write through the link, remove the link first; the app won't guess.

## Honest limits

This first cut shows only the app-wide busy cursor while a split or join of a large file runs — there's
no dedicated in-dialog progress bar yet, since a multi-GB split/join is still bounded and streamed on the
backend even though the frontend call itself is a single request/response round-trip. There's also no
in-dialog "replace existing file" confirmation for a join that lands on an occupied path in this first
cut — the dialog surfaces the backend's refusal as a plain error and leaves picking a different path (or
clearing the target) to you.
