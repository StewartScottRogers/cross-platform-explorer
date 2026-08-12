---
title: Batch Media
order: 46
category: Explorer
categoryOrder: 2
---

# Batch Media

**Batch media…** applies an ordered list of image edits — resize, convert, rotate, flip, rename, strip
metadata, compress, watermark — to every file in a multi-image selection at once, with a live preview of
each output's path before anything is written.

## When to use it

Batch media is for applying the **same transform pipeline to many images** in one pass — resizing a
folder of photos for the web, converting a batch to WebP, stripping EXIF before sharing, or stamping a
watermark across a set. For a single image, or for anything not covered here (crop, color adjustment,
format-specific edits), there is no in-app editor — batch media only ever runs the eight ops below.

It is strictly an **image** tool. Despite the "media" name, nothing here touches video or audio; a
non-image file in the selection is filtered out before the dialog even opens (see *Selection* below).

## How to open it

- **Select 2 or more files, with at least one recognised image among them**, then right-click →
  **Batch media…**.
- There is **no command-palette entry** and **no keyboard shortcut** — the context menu is the only opener.

## Selection

The menu item itself appears once **2+ items are selected and at least one is a supported image**
(`png`, `jpg`/`jpeg`, `gif`, `webp`, `bmp`, `tif`/`tiff` — case-insensitive). That set is deliberately
**narrower** than the format list the thumbnailer/Quick-look can *decode* (which also covers `avif`):
batch media's encoder can only **write** those eight formats, so a format it can decode but never
re-encode is excluded up front rather than being offered and failing on every run.

Clicking the menu item re-checks the selection:

- Non-image files (and folders) are silently dropped, with a notice — *"N of M files aren't images and
  will be skipped."* — if any were.
- If fewer than **2** images remain after that filter, the dialog never opens; instead you see *"Not
  enough image files in the selection for batch media."* Because the menu item only requires **one**
  eligible image to appear, a selection of one image plus one unrelated file shows the menu row but then
  bounces with this message instead of opening — the item's own count is what actually decides.

## Building the op list

The dialog's op builder has one row: pick an operation, fill in its settings, click **+ Add** to append it
to the list. **Nothing runs until you've added at least one op.** Ops apply **in the order they were
added**, top to bottom — each op's output feeds the next op's input (same decoded pixels, carried through
the whole pipeline, decoded once at the start and encoded once at the end). You can add the **same
operation twice** (e.g. two resizes), and remove any op from the list with its pill's **✕** — but there is
**no drag-to-reorder**; to change the order, remove and re-add in the sequence you want.

| Operation | Fields | Default | What it does |
|---|---|---|---|
| **Resize** | Max size (px, longest side) | 1024 | Downscales so neither dimension exceeds the value — **never upscales** a smaller source; a source already under the limit passes through untouched. |
| **Convert** | Target extension | `webp` | Re-encodes to a different format. Only **six** output formats exist: `png`, `jpg`/`jpeg`, `gif`, `webp`, `bmp`, `tif`/`tiff` — anything else (`heic`, `avif`, `psd`, `svg`, …) fails that file with a clear error rather than silently no-op'ing. The extension itself is folder-checked the same way a Rename template is (CPE-1623) — see *Where results land* below. |
| **Rotate** | Degrees: 90 / 180 / 270 | 90 | Rotates clockwise by an exact right angle — no arbitrary-angle input. |
| **Flip** | Horizontal / Vertical | Horizontal | Mirrors the image on the chosen axis. |
| **Rename** | Template | `{stem}` | Renames the output using tokens `{stem}` (original filename, no extension), `{n}` (1-based position in the batch), and `{ext}` (the extension at that point in the pipeline) — e.g. `photo-{n}` on 3 files → `photo-1`, `photo-2`, `photo-3`. The template can only change the **name**, not the folder or the drive: `\`, `/`, and a whole `..` component are rejected everywhere, and `:` is rejected **on Windows only** (CPE-1623 / CPE-1640) — see *Where results land* below. A literal `..` inside an otherwise ordinary name (`shot..final`, a version stamp `v1..2`) is fine — only `..` occupying a whole path segment is a traversal risk. |
| **Strip metadata** | — | — | Drops all embedded EXIF/IPTC/XMP by re-encoding from decoded pixels, which never carry it. Because a phone photo's on-disk orientation often depends on the EXIF `Orientation` tag, this first **bakes that orientation into the pixels** (an equivalent rotate/flip) so the image doesn't silently change apparent orientation once the tag is gone. |
| **Compress** | Quality (1–100) | 80 | Re-encodes at the given quality. This **only has an effect when the output is a JPEG** — png/gif/bmp/tif have no quality knob, and this build's WebP encoder is lossless-only, so Compress on any of those is a graceful no-op (the file is still re-encoded, just at that format's normal settings). Order matters: **Convert then Compress** applies quality to the new format; Compress before a later Convert has nothing to act on. |
| **Watermark** | Image (Browse…), corner (one of 5), opacity (0–100) | no image · bottom-right · 80 | Alpha-composites the chosen image onto each file at the given corner and opacity. **Optional by construction** — leave the image unset and the op contributes nothing (not even to the output filename). An overlay bigger than the base image is anchored at the corner and clipped, never scaled down. A missing or undecodable overlay file fails that op for that run (see *Failures* below), not the whole batch. |

A live **plan preview** below the op list shows every file as `original → planned output` plus a
one-line summary of the ops that will run, updating automatically (debounced ~200ms) as you edit ops —
capped at showing the first 300 rows for a very large selection, with a note of how many more there are.

## Where results land

**Write to new files (non-destructive)** is checked by default. With it checked, the output always lands
in **the same folder as the input** under a modified name, and is guaranteed to differ from every input
and from every other planned output in the batch:

- **Resize** appends `-{px}` (`cat.jpg` → `cat-1024.jpg`).
- **Rotate** appends `-rot{degrees}` (`cat-rot90.jpg`).
- **Flip** appends `-fliph` or `-flipv`.
- **Convert** changes only the extension (`cat.png` → `cat.webp`).
- **Rename** replaces the whole stem from your template, and clears any suffix the other ops would have
  added.
- **Compress**, **Strip metadata**, and **Watermark** add **no suffix of their own** — if that leaves the
  output identical to the input, the planner falls back to a generic `-out` suffix (`cat.jpg` →
  `cat-out.jpg`), so don't expect a descriptive name like `cat-compressed.jpg` from those three ops alone.
- If two inputs would land on the same output name, later ones are disambiguated `-2`, `-3`, …

**Unchecking the box** turns off both guarantees: an op combination that has no suffix of its own (a lone
Compress, Strip metadata, or Watermark) then plans to **overwrite the original file in place**. A
persistent reminder appears under the checkbox the moment it's unchecked, spelling out exactly which ops
that affects, so the risk is visible before you've even built a plan that triggers it. Resize, Rotate,
Flip, Convert, and Rename **usually** produce a differently-named file too, but not as an absolute
guarantee: **Convert** to the extension a file already has, or **Rename** left at its pre-filled default
`{stem}` template, can still resolve to the same name as the input. This is judged by **same underlying
file**, not exact text (CPE-1613): Convert always lower-cases its target extension, so converting
`IMG_1.JPG` to `jpg` plans `IMG_1.jpg` — a different-looking name that is still the identical file on
Windows and default macOS (case-insensitive filesystems), so it's treated as in-place there exactly like
an unchanged extension would be; on Linux, where filenames are case-sensitive, that pair is two distinct
possible files and isn't flagged. Whichever op combination you use, the live plan preview always shows the
real planned path, and Apply always confirms first whenever the actual plan would overwrite something —
that check (see next section) is what you can rely on, not a mental list of "safe" ops.

There is **no subfolder option** — outputs always sit alongside their inputs. This is enforced, not just
the default behaviour: a **Rename** template (and, as of the same fix, a **Convert** target extension)
can't contain `\` or `/`, or be a whole `..` path component (CPE-1623) — the field rejects it
immediately (before you can even click **+ Add**), and the backend independently re-derives the same
"does this stay inside the input's own folder?" check — both when it plans a batch AND, again, right
before it writes each file (CPE-1624) — so a template can't be used to walk the output out of the selected
folder and quietly land on — and overwrite — an unrelated file elsewhere, no matter how the plan reached
the write step. Separately, a computed output name that happens to already exist as a **real file this batch
never selected** is treated exactly like an in-place overwrite: in non-destructive mode the planner picks
a different, genuinely free name instead (the same `-2`, `-3` disambiguation already used for a collision
within the batch); with the box unchecked, it's refused the same way an in-place overwrite is (see the
next section) — Apply never silently clobbers a stranger's file.

**A colon (`:`) is rejected on Windows only (CPE-1640).** On Windows a colon is reserved twice over — it
separates a drive letter (`C:foo` means "drive C's current folder", not a file called `C:foo`) and it
separates an NTFS *alternate data stream*, a hidden second body of bytes attached to an existing file. So
on Windows a template containing a colon anywhere is refused, in the field and again in the engine. On
**Linux and macOS a colon is an ordinary, legal filename character**, so a template like `10:30am-photo`
or `session:final` is simply accepted there. This is not a relaxation of any safety rule: the guarantee
that outputs stay in their input's folder is a separate, unconditional check on the finished path, and it
behaves identically on all three platforms.

**Every safety question is answered on the file the bytes actually go into, at the moment they go in
(CPE-1624).** A long batch — or one with slow per-file work like watermarking — leaves a window in which
something else on the machine (another app, a sync client, a malicious script) can change what a name
points at after the batch has started. Checking the *name* again is not enough, because the whole trick
is changing what the name refers to. So the app instead **opens the output file once, refuses to follow
any shortcut or link at it, checks that exact opened file, and writes through it** — the file being
checked and the file being written are guaranteed to be the same one. If the check fails, that file is
**skipped with a reason** in the results panel and the rest of the batch carries on normally.

Two consequences you may notice:

- **Batch Media never writes through a shortcut, symlink or junction.** If a planned output name turns
  out to be one, it is skipped rather than followed — following it could put your images somewhere you
  never chose.
- **A file with more than one name is checked properly.** On Windows and Linux a single file can have
  several names (hard links). If a planned output has other names living outside the folder you picked,
  writing to it would change a file outside that folder, so it is refused.

### Confirming an in-place overwrite (CPE-1590)

If the live plan preview would overwrite one or more originals — checked automatically off the concrete
planned paths, not just "the box is unchecked" — clicking **Apply** does **not** run the batch
immediately. Instead it swaps the action row for a danger-styled confirmation panel that:

- States exactly **how many original files** will be overwritten in place.
- Reminds you this is **not on the Undo (Ctrl+Z) stack** — see [Undo](safety-undo).
- Tells you the app **will attempt** to checkpoint the affected folder(s) first, as a recovery net (see
  *Recovery* below) — worded as an attempt, not a guarantee, because it's a best-effort step that can
  itself fail; if it does, you're warned afterward rather than left thinking you have a safety net you
  don't.

Only its own **"Overwrite N files"** button (not the space where Apply used to be, and not a stray
Enter/click) starts the run; **Cancel** — or **Escape**, which backs out of just this panel rather than
the whole dialog — returns you to the op list with nothing written. Editing the op list, the selection, or
the checkbox while the panel is open dismisses it automatically, so a confirm can never be granted for one
plan and silently applied to a different, later-edited one. A plan where every output already differs from
its input (the default, non-destructive path, or any op combo with its own suffix) is completely
unaffected — Apply still runs immediately, with no new friction.

**The engine enforces this too, not just the dialog (CPE-1599).** The confirmation isn't only a frontend
courtesy — the backend batch-execute engine itself refuses to run any plan containing an in-place
overwrite unless it's told, explicitly, that this exact confirmation was given. In normal use you'll never
see this: this dialog's "Overwrite N files" button is the only place in the app that ever gives that
explicit go-ahead, so from here everything behaves exactly as described above. It exists as a defence in
depth against anything that might one day call the batch-media engine some other way (a scripting/
automation surface, for instance) — such a caller would get a clear refusal instead of a silent
in-place write.

## Applying

**Apply** is enabled once at least one op is added and the plan resolved cleanly. Progress renders
**inside this dialog** (not the shared bottom-corner transfer panel other file operations use) as a bar
with a live "N/M done" count, streamed as each file finishes. **There is no mid-run cancel** — once
started, a batch runs to completion; Cancel (and Escape) are disabled while it's applying.

### Recovery for a confirmed in-place overwrite

Once you click **"Overwrite N files"** on the confirm panel, the dialog takes a **best-effort checkpoint**
(`commands.checkpointCreate`, the same mechanism [Checkpoints & Rollback](16-checkpoints) uses elsewhere)
of every folder that has a file about to be overwritten — one checkpoint per distinct folder, taken once
before any byte is touched. That gives you a way back afterward (revert that checkpoint), even though
batch media's writes are still never pushed onto the app's Ctrl+Z undo stack. This is a **bonus safety
net, not a gate**: if the checkpoint itself fails (e.g. the disk is full), the confirmed write still
proceeds — you already explicitly agreed to it on the confirm panel.

**A checkpoint problem is never silent — and the dialog is honest about which kind it is.** Either way, the
dialog holds itself open afterward on a warning naming exactly which folder(s) are affected — the same
"stay open until acknowledged" treatment the skipped-files panel uses — instead of closing normally while
you still believe the promised recovery net exists. Click **Done** once you've read it. There are two
distinct warnings, worded deliberately differently because they mean very different things for recovery:

- **"No checkpoint was taken"** — the checkpoint attempt failed outright (e.g. the disk was full). That
  folder has **zero** recovery net; your only recovery for files there is your own backup.
- **"The checkpoint didn't fully cover…"** — the checkpoint succeeded, but named file(s) inside it were
  too large to capture (or hit the store's budget) and were left out. Everything else in that folder IS
  covered by the checkpoint; only the specific file(s) named in the warning would need your own backup.

If a run hits both kinds across different folders, you'll see both warnings.

## Failures and partial success

Batch media is **skip-on-error**: each file is processed independently, and one that can't be handled —
unreadable, not actually decodable despite its extension, a missing/broken watermark overlay, or a write
failure — is skipped with a reason, never aborting the rest of the run. An extremely large or
maliciously-crafted image (a declared canvas over 20,000px on a side, or a decode that would need more
than ~256 MB) is rejected the same way rather than risking a hang or an out-of-memory failure.

If anything was skipped, the dialog **stays open** on a results panel — *"✓ N written · ⚠ M skipped"* —
listing every skipped file by name with its reason, so nothing is silently dropped; click **Done** to close
it and refresh the folder. A clean run (nothing skipped) closes and refreshes immediately. The plan preview
itself does **not** predict which files will fail — a corrupt-but-correctly-named file only surfaces as a
skip after you click Apply, not beforehand.

## Worked example

You've selected 40 JPEGs to prep for a web gallery: shrink them, convert to WebP, and strip camera EXIF.

1. Select all 40, right-click → **Batch media…**.
2. Add **Resize** at 1600px, click **+ Add**.
3. Switch the operation dropdown to **Convert**, set the extension to `webp`, click **+ Add**.
4. Switch to **Strip metadata**, click **+ Add** — three pills now show: *Resize 1600px*, *Convert →
   webp*, *Strip metadata*.
5. Leave **Write to new files** checked. The preview lists all 40 as `IMG_0001.jpg →
   IMG_0001-1600.webp`, etc.
6. Click **Apply**. The progress bar counts up to 40/40; if any file couldn't be decoded it's listed with
   a reason on the results panel instead of silently vanishing.

## Limits / notes

- **Images only, always.** No video/audio operation exists despite the feature's name; a selection with
  no eligible image never opens the dialog.
- **Six writable output formats.** `png`, `jpg`/`jpeg`, `gif`, `webp`, `bmp`, `tif`/`tiff` — Convert to
  anything else fails that file. The bundled WebP encoder is **lossless-only**, so Compress has no effect
  on a WebP output.
- **No reordering** once an op is added — remove and re-add to change the sequence.
- **No mid-run cancel.** Once Apply is clicked, the batch runs to completion.
- **No pre-flight failure check.** The live preview shows a planned path for every file regardless of
  whether it will actually decode; you only learn about a bad file from the post-run skip panel.
- **Overwrite mode requires an explicit confirmation (CPE-1590).** Unchecking "Write to new files" and
  running Compress, Strip metadata, or Watermark alone no longer overwrites originals silently — Apply
  opens a danger-styled confirm panel naming the file count first, and a best-effort checkpoint of the
  affected folder(s) is attempted on confirm — and if that attempt fails, the dialog says so afterward
  rather than staying quiet about it. It's still **not** on the [Undo](safety-undo) Ctrl+Z stack — recovery
  after a confirmed overwrite is that checkpoint (see [Checkpoints & Rollback](16-checkpoints)) or your own
  backup, same as before this ticket; only the "no warning at all" gap is fixed.
- **The confirmation is enforced end-to-end, not just in this dialog (CPE-1599).** The backend refuses to
  run any plan containing an in-place overwrite unless it's explicitly told this confirmation happened —
  this dialog's "Overwrite N files" button is the only place that ever does. You won't notice this in
  normal use; it exists so nothing else that might call the batch-media engine can skip the confirmation.
- **A Rename template (or Convert extension) can't leave the selected folder (CPE-1623).** `\`, `/`, and a
  whole `..` path component are rejected in the field itself (plus `:` on Windows — see CPE-1640 above),
  and the backend independently re-checks containment on its own — both when it plans a batch and again
  immediately before writing each file — so outputs always stay in the same folder as their inputs, with no
  exception, regardless of how a plan reached the write step. A rename that would otherwise land on a real,
  pre-existing file this batch never selected is treated as an overwrite too: renamed past automatically in
  non-destructive mode, or refused (same as an in-place overwrite) with the box unchecked.
- **Hidden writes onto a *different* file are refused outright (CPE-1624).** On Windows, a path containing
  a colon can name an "alternate data stream" — hidden bytes stored inside an existing, unrelated file,
  invisible in Explorer and not counted in that file's size. Batch Media never produces such a path itself,
  and now refuses one outright if anything hands it one, saying so in plain terms rather than reporting it
  as a folder escape (it isn't one — the data would have stayed in the folder, just hidden on the wrong
  file).
- **No command-palette entry or shortcut** — right-click is the only way in.
