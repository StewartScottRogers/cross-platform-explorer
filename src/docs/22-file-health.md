---
title: File Health
order: 22
category: Safety & Recovery
categoryOrder: 5
---

# File Health

**File Health** is a tabbed panel that surfaces the file-inspection detectors built for the explorer —
facts and hazards the plain listing doesn't show. It grows one tab at a time; today it wires four:
**dangling and cyclic symlinks**, **type mismatches**, **orphan sidecars**, and **empty folders**.

Open it from the **Tools** menu (*Find dangling links…* / *Find type mismatches…* / *Find orphan
sidecars…* / *Find empty folders…*) or the **command palette** (Ctrl/Cmd+Shift+P → the same four
entries). Each entry opens the panel straight to its own tab — even if the panel is already open on a
different tab — and every scan is scoped to the folder you're currently in.

## Excluding folders from a scan

Below the tab strip is an **Exclude** box, shared across all four tabs — one exclude list, not a
separate one per tab. Type a glob pattern (for example `node_modules`, `*.log`, or `.git`) and press
**Enter** to add it as a pill; click a pill's **×** to remove it. **Quick add** offers one-click chips
for the three most common noisy folders — `node_modules`, `.git`, `target` — which add nothing until
you actually click them, so a scan you run without touching this box behaves exactly as before.

Excludes take effect on your **next** Scan or Rescan — editing the list never re-runs a scan by itself,
and a scan already in progress keeps using whatever excludes were active when you clicked Scan. A
matching folder is pruned entirely (nothing inside it is walked or reported), which both speeds up the
scan and keeps noisy directories like `node_modules` out of the results.

## Dangling links

A symlink is **dangling** when it points at a target that no longer exists — often left behind after the
target was moved, renamed, or deleted. A symlink is **cyclic** when following its chain of targets loops
back on a path already seen, so it can never resolve to real content either way.

1. **Scan.** The app walks the current folder (and subfolders) looking for symlinks in either state.
   Results **stream in live** as they're found, so a large tree starts showing hits immediately instead
   of making you wait for the whole walk to finish.
2. **Review.** Each flagged link is shown with its name, location, and a **Missing target** or
   **Cyclic link** badge. Click any entry to jump straight to that file in the explorer.

## Type mismatches

A file has a **type mismatch** when its actual content doesn't match what its extension claims — for
example, a `.jpg` that's really a renamed Windows executable, or a document saved with the wrong
extension. The app sniffs each file's real bytes rather than trusting the name.

1. **Scan.** The app walks the current folder (and subfolders) checking each file's sniffed type against
   its claimed extension. Results **stream in live** as they're found.
2. **Review.** Each flagged file is shown with its name, location, and a badge reading *"claims `.ext` →
   looks like <detected type>"*. Click any entry to jump straight to that file in the explorer.

## Orphan sidecars

A **sidecar** is a companion file that only makes sense alongside a primary file — a `.srt` subtitle
track, an `.xmp` metadata sidecar, and similar pairings. An **orphan sidecar** is one whose matching
primary file is missing, so it no longer has anything to accompany.

1. **Scan.** The app walks the current folder and subfolders looking for sidecar files with no matching
   primary in the same folder. Results **stream in live** as they're found.
2. **Review.** Each flagged file is shown with its name and location. Click any entry to jump straight to
   that file in the explorer.

## Empty folders

An **empty folder** is one with nothing in it — or one that contains only other empty folders (an "empty
cascade"). Removing the topmost folder in a cascade removes the whole thing, so only the topmost
cascade-empty folder in each branch is reported, not every nested empty folder inside it.

1. **Scan.** The app walks the current folder and subfolders looking for cascade-empty folders. Unlike
   the other three tabs, this scan runs as a single pass rather than streaming results in live — it's
   typically a fast walk, so the whole result lands at once.
2. **Review.** Each flagged folder is shown with its name and location. Click any entry to jump straight
   to that folder in the explorer.

## Read-only

Every tab in this panel is **read-only** — nothing is deleted or modified. It's a review surface, not a
cleanup tool; use the explorer's normal delete/move actions on anything you want to fix once you've found
it.

## Archive safety

**Check archive safety…** is a related, one-off check rather than a fifth tab: right-click a `.zip`-family
archive (`.zip`, `.jar`, `.apk`, `.war`, `.ear`, `.ipa`, `.xpi`, `.whl`, `.nupkg`, `.vsix`) and choose
**Check archive safety…** to score it for zip-bomb / expansion-ratio risk — a tiny, highly compressible
payload that decompresses to something enormous.

1. **Scan.** The dialog reads the archive's central directory and compares every entry's compressed size
   against its uncompressed size — a single, immediate check, not a background walk.
2. **Review.** The dialog reports the archive's overall compression ratio, its compressed → uncompressed
   size, how many entries were scanned, and any individual entries whose own ratio is unusually high
   (flagged as pills). A clear **danger** indicator appears when the archive as a whole crosses the
   zip-bomb threshold; otherwise it reports as safe.

Only true ZIP containers are scored today — `.tar`, `.tar.gz`/`.tgz`, `.7z`, `.iso`, and `.rar` aren't ZIP
archives, so the action doesn't offer itself for them (rather than silently reporting "0 entries scanned"
as if it had checked). Support for those formats is a later addition. See [Archives](explorer-archives)
for the full format/extract/create matrix and the rest of the archive feature set this check is part of.

A password-protected ZIP opens fine (its central directory needs no password) but its individual entries
can't be read without one — the dialog counts those and reports a dedicated "couldn't be read, likely
password-protected" state rather than the plain safe banner, so an unassessed archive is never mistaken
for a clean one. See [Archives](explorer-archives) for the full behavior.

## More tabs coming

The panel is built to grow: future slices add tabs for the explorer's remaining file-health detectors
alongside these four, so File Health becomes a single place to review everything the explorer has
noticed.
