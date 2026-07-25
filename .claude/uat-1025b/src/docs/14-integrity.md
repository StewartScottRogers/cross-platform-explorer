---
title: Integrity Guard (Bitrot Detection)
order: 14
category: Explorer
categoryOrder: 2
---

# Integrity Guard — Catching Silent Corruption

Disks quietly rot. A photo, an archive, or a backup can have its bytes flip on disk with **no warning and
no change to its modified date** — so you'd never notice until you open it and it's garbage. The
**Integrity Guard** catches exactly that.

Open it from the **command palette** (Ctrl/Cmd+K → *Integrity check…*).

## How it works

1. **Baseline a folder.** Pick a folder and click **Baseline**. The app records a checksum (SHA-256) plus
   the size and modified time of every file under it. This is your known-good snapshot.
2. **Verify later.** Click **Verify** any time. The app re-scans and compares each file to the baseline,
   sorting every path into one of five buckets:

   | Result | What it means |
   |---|---|
   | **Intact** | identical to the baseline |
   | **Edited** | content changed **and** the modified time moved — a normal, intended edit |
   | **Corrupted** | content changed but the modified time did **not** — **silent bitrot**, the alarming case |
   | **Missing** | in the baseline, gone from the folder now |
   | **New** | in the folder now, not in the baseline |

   The **corrupted** and **missing** rows are surfaced first — those are the ones worth acting on.
3. **Rebaseline** to accept the current state as the new known-good (after an intended round of edits).

The comparison runs in the backend and returns only the small report, so verifying a large photo library
or archive stays responsive.

## Monitoring everything at once

- **Verify all baselined folders…** (command palette) checks *every* folder you've baselined in one pass
  and gives you a one-line verdict — e.g. *"2 of 9 baselined folders have issues — 3 files corrupted or
  missing."* Then open Integrity on the affected folder to see which files.
- **Verify on startup** — a checkbox in the Integrity dialog. When on, that same all-folders check runs
  once, a moment after the app launches, so you're told about silent corruption without lifting a finger.
  It's **opt-in and off by default** — nothing scans in the background unless you turn it on.

## Tips

- Baseline folders whose contents *shouldn't* change on their own — photo/video libraries, document
  archives, finished backups. That way any **corrupted** result is a real red flag.
- A **corrupted** file (hash changed, mtime unchanged) is the signal to restore that file from a backup —
  the on-disk copy is no longer trustworthy.
