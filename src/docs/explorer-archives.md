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

Three independent protections apply automatically — you don't opt into any of them:

- **A symlink at the destination is refused, never written through.** Creating a file at a symlink's name
  doesn't replace the link: it writes *through* it, into whatever the link points at, and reports success
  about a file you never named. So if the archive you're about to create — or the file a single-file `.gz`
  would unpack to — lands on a name that already holds a symlink, the operation stops before writing
  anything and tells you it found a link, rather than quietly overwriting the link's target. During a **ZIP
  or 7-Zip** extraction the same check applies per entry: an entry that would land on an existing link in
  the destination folder is **skipped** and the rest of the archive still extracts. Overwriting an ordinary
  existing file is unaffected — that stays allowed, because it's a thing you can reasonably mean.
- **Nothing an extraction writes can end up outside the folder you picked — including via a folder
  shortcut.** This is a second, separate check from the one above, and it covers the case a name-only
  check cannot see: if the destination already contains a **folder** shortcut (a symlink, or on Windows a
  junction — which any account can create, no special permission needed) and an archive entry is
  addressed *through* it, like `sub/report.txt` where `sub` is the shortcut, then writing it normally
  would put the file wherever the shortcut leads. Every entry's full path is now resolved before anything
  is written, and an entry that doesn't provably stay inside the folder you chose is **skipped** — with a
  note in the operations panel saying so — while the rest of the archive still extracts. The archive's own
  folder entries get the same treatment, so an extraction can't create directories out there either.
  Extracting into a **new, empty folder** (what the plain **Extract** action always does) has no shortcuts
  to run into in the first place.
- **What happens to a link already sitting in the destination differs by format**, and it's worth knowing
  which you're using. Extracting a **ZIP** or a **7-Zip** archive through the normal Extract / Extract to…
  flow skips just that entry, as above, and leaves both the link and the file it points at alone. A **TAR**
  (`.tar`, `.tar.gz`, `.tgz`) behaves differently at the library level: it **replaces the link with a
  regular file** — the file the link pointed at is left alone, but the link itself is gone. If a
  destination folder contains shortcuts you care about, extract into a new empty folder instead.
  For the *folder*-shortcut case in the bullet above the formats differ again, though every one of them is
  safe: ZIP and 7-Zip skip the entry and carry on, while **TAR refuses the whole extraction** with
  *"trying to unpack outside of destination path"* and unpacks nothing at all. (An earlier version of this
  page said a folder shortcut "is still followed" during extraction. That was never true of TAR, and is no
  longer true of anything.)
- **Zip-slip (path traversal) protection.** An archive entry whose name would escape the destination
  folder — an absolute path, or one containing `..` — is **silently skipped** during extraction rather than
  written outside where you asked, for every supported format (zip, tar, 7z, the one-entry-at-a-time RAR
  path). This is a structural guard baked into the extractor itself, not something you check separately.
- **Entry *names*, not just where they point, are checked too.** Separately from the traversal guard
  above, an entry whose own leaf name isn't one the local filesystem can safely hold is also skipped
  rather than written: a name containing a colon (`file:stream`) — which on Windows/NTFS would otherwise
  divert the bytes into a hidden alternate data stream on a neighbouring file, leaving no visible file at
  all — a name that starts with `..` without being a traversal component (`..evil`), and, on Windows
  only, a reserved device name (`con`, `nul`, `com1`, …) or a name ending in a run of `.`/space. These are
  the same per-segment rules the Network program's downloads already apply; extraction skips the entry
  rather than renaming it, so the entry is missing rather than silently landing somewhere else — check
  the operations panel if a count looks lower than expected.
- **Zip-bomb / expansion-ratio scoring**, via **Check archive safety…** — for the ordinary case, reads a
  ZIP's central directory (no extraction) and compares every entry's compressed size against its
  uncompressed size. It reports the overall compression ratio, total compressed → uncompressed size, how
  many entries were scanned, and flags any individual entry whose own ratio is unusually high. An entry
  (or the archive as a whole) expanding more than **100×** trips a clear **DANGER** banner; otherwise it
  reports as safe. This threshold isn't configurable from the UI.
  - **The check no longer takes an archive's declared sizes on faith.** A ZIP's compressed/uncompressed
    sizes are numbers the archive states about itself, and nothing stops those numbers from being wrong in
    either direction — a real bomb can be re-packaged with an artificially small declared uncompressed
    size (making it read as a tiny, harmless file), or with an artificially large declared compressed size
    (making its ratio read as unremarkable). Every entry is cross-checked several ways before its declared
    sizes are trusted: its local file header against the central directory; its declared sizes against
    what's physically possible for its compression method; and its declared *compressed* size against a
    hard ceiling derived from the entry's real position in the file — no archive can claim a compressed
    size larger than the actual gap to whatever comes next on disk without literally writing that many
    extra bytes. Whenever any of those checks finds something implausible, the scan verifies the entry for
    real by decompressing it — but only up to a **capped** number of bytes, never the whole thing. If that
    capped read proves the entry expands past the threshold, it's reported dangerous without decompressing
    any further; if the scan can't finish verifying an entry within its own time/byte budget, that entry is
    reported as **not fully assessed** (the same "couldn't be checked" state below), never as safe. An
    ordinary archive's metadata agrees with itself and its own file layout and never triggers this extra
    work, so the common case is exactly as fast as before.
  - **What this still doesn't guarantee:** the check is a best-effort risk signal, not a proof. It only
    looks at ZIP entries (not nested archives-within-archives, and not other formats). Its own
    decompression verification is deliberately capped for both a single entry and the archive as a whole,
    so the scanner itself can never be turned into the bomb — a suspicious entry with an unusually large
    *compressed* size can hit that hard cap before the ratio math alone can prove it dangerous, in which
    case it's reported "not fully assessed" rather than a guess either way, instead of ever quietly
    passing as safe. Treat a "safe" verdict as "nothing suspicious found," not as a guarantee the archive
    is harmless.

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
- **"That name is taken" messages name the name.** Two spots used to hand you the operating system's own
  wording, which is unhelpfully vague: creating a new empty archive over an existing file reported only
  *"The file exists"* (which file?), and extracting into a folder whose name is held by a **broken
  shortcut** reported *"Cannot create a file when that file already exists"* — sending you to delete a file
  that isn't there, when what's actually at that name is the shortcut. Both now name the full path and say
  which of the two things is meant.
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
- **Not every entry can always be vouched for — and the dialog says so honestly.** An entry lands in the
  "couldn't be read" count for either of two independent reasons: it's **encrypted** (an AES/ZipCrypto
  entry can't be read without a password, and this dialog has no password prompt), or its declared sizes
  looked implausible and the scan's own bounded verification (see the decompression-verification bullet
  above) **ran out of budget** before it could confirm the entry either way. Both land in the same
  tri-state, since the dialog can't reliably tell them apart and the honest answer is the same either
  way — "not fully assessed", not "safe". When the count is non-zero, the dialog shows a dedicated state
  instead of the green safe banner: **"{count} entries couldn't be read (some may be password-protected,
  others too complex to verify in time) — this archive's safety could not be checked."** It never claims
  "No zip-bomb risk detected" for an archive it only partially (or never) examined. If some entries *were*
  readable and one of those trips the danger threshold, the **DANGER** banner still leads, with a note
  that other entries couldn't be assessed. An all-encrypted zip, and an archive whose suspicious entries
  all exhausted the verification budget, both report as unassessed rather than safe or dangerous.
- **The symlink refusal covers ZIP and 7-Zip extraction, but not TAR.** Stated plainly rather than
  implied: the per-entry link check described under *Safety limits* is applied wherever this app gets to
  decide before the write — archive creation, single-file `.gz` unpacking, and ZIP **and 7-Zip**
  extraction. 7-Zip entries are written by the decoder library, but it hands this app each entry's
  destination first, so the same check runs and the entry is skipped. **TAR** gives no such hook: its
  entries go straight to the decoder, which replaces the link with a regular file (see the format bullet
  under *Safety limits*). Extract into a **new, empty folder** (the plain **Extract** action already does
  exactly that) if you don't trust the archive. This is only about a link at an entry's **own name** — the
  separate "can't land outside the folder you picked" guarantee above holds for every format, TAR
  included.
- **No configurable safety thresholds** — the 100× expansion-ratio limit, the lower ratio that triggers
  decompression verification, and the verification time/byte caps are all fixed.
- **No entry-count cap on ZIP/TAR listing itself** (unlike RAR/ISO/the safety scanner, which are capped) —
  a very large archive's listing has no built-in ceiling.
- Compress and Extract share the same transfer queue and cancel/progress behavior as copy and move — see
  the *Files* section of [The Explorer](03-explorer) for that shared convention.
