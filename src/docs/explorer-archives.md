---
title: Archives
order: 45
category: Explorer
categoryOrder: 2
---

# Archives

The explorer treats a `.zip` (and several other container formats) as a first-class object: you can
**browse inside it like a folder**, **extract** it, **compress** a selection into a new one, and **check**
a zip for zip-bomb-style risk — without ever needing an external archive tool.

## Supported formats

| Format | Browse / list contents | Extract | Create (Compress) |
|---|---|---|---|
| `.zip` and the zip-family (`.jar .apk .war .ear .ipa .xpi .whl .nupkg .vsix`) | ✅ | ✅ | ✅ |
| `.tar` | ✅ | ✅ | only as part of `.tar.gz`/`.tgz` — see below |
| `.tar.gz` / `.tgz` | ✅ | ✅ | ✅ |
| `.gz` (a single compressed file, not a tar) | ✅ (one synthetic entry) | ✅ | — |
| `.7z` | ✅ | ✅ | ❌ — this build can **read** 7-Zip archives but never **creates** one |
| `.iso` (disc image) | ✅ (capped at 2,000 listed entries) | ❌ | ❌ |
| `.rar` | ✅ (header listing only, capped at 100,000 entries) | ⚠️ **only an uncompressed (STORE) entry**, one at a time — RAR's own compression is proprietary with no free decoder, so a compressed entry refuses cleanly rather than producing garbage | ❌ |

`.xz`, `.bz2`, `.zst`, `.lz`, `.lzma`, `.dmg`, and `.cab` are **not** archive containers this app opens at
all — a single-file compressed blob like `.xz` shows a plain file-info summary instead of a browsable
listing, and disk images/cabinets aren't supported in any form.

**Password protection (Compress with password…)** creates an **AES-256** encrypted `.zip` — there's no
option to choose a different AES strength or the older, weaker "ZipCrypto" scheme some tools offer; every
password-protected archive this app creates is AES-256. Password protection is **zip-only** — you can't
password-protect a `.tar.gz`.

## Browsing inside an archive

A `.zip`/`.tar`/`.tar.gz`/`.tgz`/`.gz`/`.7z`/`.iso`/`.rar` file gets two read-only ways to look inside:

- **Select it** (single click) — the preview pane lists every entry's full internal path and size, as a
  flat table.
- **Double-click it** — the main file list itself switches into the archive, showing its top-level
  entries as regular-looking rows; double-click a folder-like entry to go deeper, and the breadcrumb/back
  button work the same as real navigation. This is a genuinely read-only view: every mutating action
  (rename, delete, paste, new folder, …) is refused with *"This is a read-only view inside an archive."*
- **Opening a file** from inside an archive extracts just that one entry to a temporary file and opens
  the temp copy — the archive itself is never touched by opening something inside it.
- A **password-protected zip can't be listed** without its password. Double-clicking one prompts for the
  password and, once given, extracts it to a sibling folder instead of opening in place (with a notice
  explaining what happened) — the preview-pane listing, by contrast, just reports it can't be opened, with
  no password prompt of its own.

**Alt-drag out of an open archive**: a plain drag on a row inside an archive does nothing (there's nothing
to move — these are synthetic rows, not real files). Hold **Alt** while starting the drag to extract the
selected entries to temp files first, then hand those real files to your OS — the same modifier used
elsewhere in the app to drag a selection out to another application.

## Extract, Extract to…, and Check archive safety…

Right-click an archive (or select it and use the preview pane's action bar) for:

- **Extract** — unpacks into a new folder right there, next to the archive.
- **Extract to…** — same, but you pick the destination folder.
- **Check archive safety…** — scores the archive for zip-bomb-style risk (see below). Only offered for
  true **ZIP-family** archives — `.tar`, `.tar.gz`/`.tgz`, `.7z`, `.iso`, and `.rar` never show this action,
  rather than running a check that can't actually score their format.

**Extract** and **Extract to…** are offered for the zip family, `.tar`, `.tar.gz`/`.tgz`, and `.7z` —
**not** for `.iso` or `.rar`, which are browse-only. Both run through the same transfer queue as copy/move:
a large extract shows live progress in the bottom-corner operations panel, one tick per archive entry, and
stays **cancellable** — cancelling leaves whatever entries had already been written, rather than deleting
the partial result.

Compressing a selection (**Compress to ZIP** / **Compress to .tar.gz** / **Compress with password…**) runs
through the same queue and is cancellable the same way; a cancelled compress still finishes writing a
valid (if incomplete) archive rather than leaving a corrupt file.

## Safety limits

Two independent protections apply to every extraction, automatically — you don't opt into either:

- **Zip-slip (path traversal) protection.** An archive entry whose name would escape the destination
  folder — an absolute path, or one containing `..` — is **silently skipped** during extraction rather than
  written outside where you asked, for every supported format (zip, tar, 7z, the one-entry-at-a-time RAR
  path). This is a structural guard baked into the extractor itself, not something you check separately.
- **Zip-bomb / expansion-ratio scoring**, via **Check archive safety…** — reads a ZIP's central directory
  (no extraction) and compares every entry's compressed size against its uncompressed size. It reports
  the overall compression ratio, total compressed → uncompressed size, how many entries were scanned, and
  flags any individual entry whose own ratio is unusually high. An entry (or the archive as a whole)
  expanding more than **100×** trips a clear **DANGER** banner; otherwise it reports as safe. This
  threshold isn't configurable from the UI.

**Checking safety does not gate extraction.** They're two independent buttons — Extract and Extract to…
never consult the safety score, and there is no "this looks like a zip bomb — extract anyway?" prompt.
Run **Check archive safety…** yourself before extracting anything you don't trust; the app won't do it for
you automatically.

## Read-only vs. modifying operations

| Action | Touches the archive file itself? |
|---|---|
| Browse / list contents | No — read-only |
| Open a file from inside | No — extracts to a temp file, archive untouched |
| Check archive safety… | No — reads directory metadata only |
| Alt-drag a file out | No — extracts to a temp file first |
| **Extract** / **Extract to…** | No (writes new files elsewhere) — the archive itself is left as-is |
| **Compress** / **Compress with password…** | Creates a brand-new archive — never modifies an existing one |

Nothing in this feature ever rewrites bytes inside an existing archive; every operation either only reads
it or produces separate output.

## Error handling

- **Wrong password on extract** — checked up front, before anything is queued, so a wrong password fails
  immediately with a re-prompt ("Wrong password — try again.") rather than producing corrupt output.
- **Corrupt or unrecognised archive** — every reader (zip, tar, 7z, ISO, RAR) fails cleanly with an error
  message rather than crashing the app, including a defensive guard around a known crafted-file crash bug
  in the underlying 7-Zip reader.
- **Failed to open while double-clicking to look inside** — a plain notice naming the archive.

## Worked example

You've received a `report-archive.zip` from an unfamiliar source and want to check it before extracting.

1. Right-click it → **Check archive safety…**.
2. If it reports **DANGER**, don't extract it — a tiny archive claiming a huge expansion ratio is the
   classic zip-bomb signature.
3. If it reports safe, right-click again → **Extract to…** and pick a destination.

## Limits / notes

- **`.iso` and `.rar` are browse-only** — you can look inside either, but there is no Extract action for
  them (RAR has no free decompressor for anything but a STORE-mode entry; ISO extraction isn't wired up).
  Don't expect the right-click menu or preview action bar to offer Extract for these two formats.
- **`.7z` is extraction-only** — this app can open and unpack a 7-Zip archive but never creates one.
- **Plain `.tar` can be extracted but not created** — Compress only offers `.zip` and `.tar.gz`/`.tgz`.
- **RAR can only give you back an uncompressed (STORE) entry**, one file at a time via "open" or Alt-drag —
  a genuinely compressed RAR entry refuses with a clear error rather than corrupting output.
- **"Check archive safety…" is ZIP-family only** and never gates Extract — see *Safety limits* above.
- **A password-protected ZIP can't be safety-checked today — and the dialog says so honestly.** The
  safety scan needs to read every entry's size metadata, and an encrypted entry can't be read without its
  password. The scan still opens the archive and counts how many entries it couldn't read; when that count
  is non-zero, the dialog shows a dedicated **"couldn't be read (likely password-protected) — this
  archive's safety could not be checked"** state instead of the green safe banner — it never claims "No
  zip-bomb risk detected" for an archive it only partially (or never) examined. If some entries *were*
  readable and one of those trips the danger threshold, the **DANGER** banner still leads, with a note
  that other entries couldn't be assessed. There's still no password prompt in this dialog, so an
  all-encrypted zip always reports as unassessed rather than safe or dangerous.
- **No configurable safety thresholds** — the 100× expansion-ratio limit is fixed.
- **No entry-count cap on ZIP/TAR listing itself** (unlike RAR/ISO/the safety scanner, which are capped) —
  a very large archive's listing has no built-in ceiling.
- Compress and Extract share the same transfer queue and cancel/progress behavior as copy and move — see
  the *Files* section of [The Explorer](03-explorer) for that shared convention.
