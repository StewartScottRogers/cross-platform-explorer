---
title: Native Metadata Bridge
order: 17
category: Organizing & Tagging
categoryOrder: 3
---

# Native Metadata Bridge

The explorer keeps its own tag/colour-label store (see the tag editor) so tagging works identically
on every platform, including filesystems that can't hold OS-native metadata at all. The **native
metadata bridge** is an opt-in layer on top of that: it lets the app read — and optionally write —
the metadata your operating system *already* understands, so tags applied elsewhere (Finder, File
Explorer, a shell script) show up here too, and vice versa.

It is **off by default**. Nothing native is read or written until you turn it on.

## Turning it on

Open **Application → Settings** and find **Native metadata bridge**. Flip **"Sync tags with
OS-native file metadata"** on. That's the only switch — there's no separate per-folder or per-file
opt-in.

Turning it on adds two things to the app:

- **Pull/Push controls** in the tag editor, next to the usual tag chips.
- A read-only **Native metadata** section in the **Properties** dialog for a single selected file.

Turning it back off hides both immediately; nothing already pulled into the app's own tag store is
deleted.

## What it actually touches, per platform

The bridge reads and writes whatever your OS calls "tags" natively:

| Platform | Native store |
|---|---|
| macOS | Finder tags (and colour labels) |
| Windows | NTFS alternate data streams (ADS) |
| Linux | Extended file attributes (xattr) |

If the file lives on a filesystem that can't hold any of the above (a FAT-formatted USB stick, some
network shares, an archive view), the bridge degrades to a silent no-op — it never errors out or
blocks the rest of the app. The Properties dialog's Native metadata section still renders in that
case; it just has nothing to show.

## Properties → Native metadata

With the bridge on, opening **Properties** (right-click → Properties, or **Alt+Enter**) for a single
file shows a **Native metadata** section below the usual details:

- **Tags** — the native tags currently known for this file, as chips. Empty shows "No tags" rather
  than nothing, so it's clear the section is working, just has no native tags to report.
- **Label** — the native colour label, if the platform's native store carries one (macOS Finder
  labels); otherwise "None".
- **Pull** — re-reads the file's native tags right now and folds them into the app's own tag store
  (a non-destructive union — it never removes a tag the app already has). This is how a tag applied
  in Finder or File Explorer shows up inside the app.

Properties is read-mostly for native metadata: **Pull** is the only write the dialog itself performs
(into the app's *own* store, not back out to the OS). Pushing the app's tags out to the native store
is the tag editor's job — see below.

## Tag editor → Pull / Push

The tag editor (opened from a file's tag chip, or via **Tags…** in the context menu) gains two extra
buttons once the bridge is on:

- **Pull** — same as Properties' Pull: reads native tags into the app's store, non-destructively.
- **Push** — writes the app's *current* tags for this file out to the native store, overwriting
  whatever native tags were there before. Use this after tagging a file inside the app when you want
  Finder/File Explorer to show the same tags.

The app's internal tag store is always the source of truth for the app itself — Pull and Push are
explicit, on-demand sync actions, never automatic background writes.

## Native Tags column

The details view's column picker (**Manage columns…**) offers an opt-in **Native Tags** column,
independent of the bridge toggle above. It shows each row's native tags (comma-joined) read lazily,
one file at a time, only for rows currently visible — never a bulk directory-wide scan — so adding it
doesn't slow down browsing a large folder.

## Why opt-in

Reading native metadata means extra per-file OS calls, and on some platforms/filesystems those calls
can be slow or simply unsupported. Keeping the bridge off by default means the plain explorer stays
exactly as fast and predictable as it always has been; turning it on is a deliberate trade — a little
more I/O per file, in exchange for two-way visibility with the OS's own tagging.
