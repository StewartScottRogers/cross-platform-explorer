---
title: Batch Rename
order: 39
category: Organizing & Tagging
categoryOrder: 3
---

# Batch Rename

**Batch Rename** renames several files or folders in one pass — replace text, add a prefix/suffix,
number them in sequence, or change their case — with a **live preview** of every resulting name before
anything on disk changes.

## When to use it (vs. a single rename)

- **One item** → press **F2** (or right-click → **Rename…**) for the ordinary inline rename.
- **Two or more items selected** → the same **Rename…** action opens the **Batch Rename** dialog instead,
  since a single inline text field can't usefully rename several files at once.

Batch Rename only ever touches the **name** of each item — it doesn't move files between folders (use
cut/paste or drag-and-drop for that) and it doesn't touch file contents.

## How to open it

- **Select 2 or more items**, then **right-click → Rename…**.
- There is currently no command-palette entry and no dedicated keyboard shortcut for the batch dialog
  specifically — **F2** always opens the single-item rename, and reaches Batch Rename only indirectly
  (it's disabled/inapplicable when more than one item is selected). The context-menu **Rename…** item is
  the one opener today.

## The four modes

Switch modes with the tab strip at the top of the dialog; the preview list below updates live as you
type, whichever mode is active.

### Find & Replace

| Field | Default | Effect |
|---|---|---|
| **Find** | empty | Text to search for in each name. An empty Find is a no-op — nothing changes. |
| **Replace with** | empty | Text that replaces every match. Treated as a **literal string** — special replacement patterns like `$&` or `$1` are not interpreted, so a name like `US$5` or `v1` can't get mangled by an accidental backreference. |
| **Case sensitive** | **off** | Off matches `Find` regardless of case; on requires an exact-case match. |

Every occurrence of `Find` in the **whole name** (base name **and** extension) is replaced — so
`Find: jpg` also matches inside `.jpg` extensions, not just the base name.

### Add prefix/suffix

| Field | Default | Effect |
|---|---|---|
| **Prefix** | empty | Text inserted at the very start of the name. |
| **Suffix** | empty | Text inserted at the end of the **base name**, before the extension — `report.pdf` + suffix `-v2` → `report-v2.pdf` (not `report.pdf-v2`). |

Both empty is a no-op. You can set both at once.

### Number sequence

| Field | Default | Effect |
|---|---|---|
| **Name pattern** | empty | The new base name. A run of `#` characters marks where the number goes and sets its zero-padded width — `photo-###` numbers as `photo-001`, `photo-002`, … A pattern with **no** `#` gets the number appended plainly (`photo` → `photo1`, `photo2`, …). An empty pattern is a no-op. |
| **Start at** | **1** | The first number used; each subsequent item in the list gets the next integer. |

The original extension is always kept, appended after the generated base name. Items are numbered in
the **order they appear in the preview list** (the order they were selected in the file list), starting
from **Start at**.

### Change case

| Field | Default | Effect |
|---|---|---|
| **Case** | **lowercase** | One of **lowercase**, **UPPERCASE**, or **Title Case** (first letter of each word capitalised, e.g. `my report notes` → `My Report Notes`). |

Only the **base name** is changed — the extension is left exactly as it was, so `README.TXT` with
lowercase applied becomes `readme.TXT`, not `readme.txt`.

## The preview

Every selected item is listed as `original → proposed`, live-updated as you change fields:

- An item whose name **wouldn't change** is dimmed (still listed, so you can see nothing was missed).
- An item that would **collide with another item's new name** (two files ending up with the same name)
  is flagged in the danger color, and a warning line explains a conflict exists.
- A status line under the list summarizes the outcome: *"N of M will be renamed"*, *"Nothing would
  change"*, or the conflict warning.

## Applying

The **Rename** button is disabled until at least one name would actually change **and** there is no
conflict. Click it to apply every changed name in one pass. If a rename fails partway (e.g. a name is
now taken by something the preview didn't know about), the app reports which ones failed rather than
silently skipping them.

Applying a batch rename is a **single undoable step** — see [Undo](safety-undo). Press **Ctrl+Z** right
after and every renamed file reverts to its original name in one action, not one Ctrl+Z per file.

**Escape** or clicking outside the dialog cancels without renaming anything.

## Worked example

You select 4 vacation photos named `IMG_0001.jpg` … `IMG_0004.jpg` and want them called
`beach-2024-01.jpg` … `beach-2024-04.jpg`:

1. Select all 4, right-click → **Rename…**.
2. Switch to the **Number sequence** tab.
3. Set **Name pattern** to `beach-2024-##` and **Start at** to `1`.
4. The preview shows `IMG_0001.jpg → beach-2024-01.jpg`, `IMG_0002.jpg → beach-2024-02.jpg`, and so on —
   no conflicts, all 4 changed.
5. Click **Rename**. All 4 files are renamed in one step; **Ctrl+Z** would restore the original
   `IMG_000N.jpg` names if needed.

## Limits / notes

- **One mode at a time.** You can't combine, say, Find & Replace with a number sequence in a single
  pass — apply one mode, then reopen the dialog on the result for a second pass if you need to stack
  transforms.
- **Conflict detection is intra-batch only.** The preview flags names that would collide *with each
  other* inside this batch; it does not check the target folder for a pre-existing file of that exact
  name ahead of time. The backend refuses to silently overwrite an existing file when you click
  **Rename**, so a real collision surfaces as a per-item failure rather than data loss — but you won't
  see it flagged in the preview beforehand.
- **Names only, not paths.** This dialog never moves items between folders; all renamed items stay
  exactly where they were.
- See [Undo](safety-undo) for how to reverse a completed batch rename, and [Searching for
  Files](12-search) for finding the items you want to select in the first place.
