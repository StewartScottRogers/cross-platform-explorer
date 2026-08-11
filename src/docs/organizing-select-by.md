---
title: Select By…
order: 42
category: Organizing & Tagging
categoryOrder: 3
---

# Select By…

Three ways to select a group of items in the **current folder's visible list** by criteria, instead of
clicking (or Ctrl/Shift-clicking) each one by hand:

| Tool | Opens from | What it builds |
|---|---|---|
| **Select by…** | Command palette | One structured condition — extension, name glob, size, age, or folder-vs-file — applied to the whole list. |
| **Select by pattern…** | Right-click empty area | A single glob box (quicker, name-only). |
| **Select all `.ext`** | Right-click a file | Instant, zero-typing: selects every visible item sharing that one file's extension. |

## When to use which

- **Select by…** when you want size/age/folder criteria, or you want to capture the same condition as a
  named **Saved Search** (see *Save search…* below) instead of just selecting.
- **Select by pattern…** when a quick glob is all you need — `*.jpg`, `report-*`, `img_????.png`.
- **Select all `.ext`** when you already have one representative file selected and just want its
  siblings, with no dialog at all.

## How to open each

- **Select by…** — command palette (**Ctrl+Shift+P**) → **"Select by…"**. There is no context-menu
  entry and no dedicated keyboard shortcut for it.
- **Select by pattern…** — right-click an **empty area** of the file list (not on an item) →
  **"Select by pattern…"**, alongside **Select all** and **Invert selection** in the same menu section.
  No shortcut.
- **Select all `.ext`** — right-click a **single file** (not a folder, and only when exactly one item is
  selected) → the item context menu shows **"Select all `.ext`"** with the real extension filled in,
  e.g. *"Select all .jpg"*. Selecting more than one item, or a folder, hides this entry.

## Select by… — criterion kinds

Pick one kind from the dropdown; the fields below it change to match:

| Kind | Fields | Matches |
|---|---|---|
| **Extension** | Comma list, e.g. `ts, md, png` | Any of the listed extensions (leading dot optional). |
| **Name (glob)** | One glob, e.g. `*.min.js` | The name against the pattern — same `*`/`?` rules as [Searching for Files](12-search). |
| **Size** | Min bytes / max bytes (either may be blank) | File size within the given range. |
| **Older than** | Days | Last-modified **older** than N days. |
| **Newer than** | Days | Last-modified **within** the last N days. |
| **Is folder** | A checkbox | Folders when checked, files when unchecked. |

Only **one** condition kind builds at a time — there's no AND/OR combination the way the instant
filter's `key:value` syntax supports (see [Searching for Files](12-search)). Pressing **Enter** in the
**Extension** or **Name (glob)** field submits immediately; the Size/age fields don't have an
Enter-to-submit shortcut, so use the **Select** button for those.

### Actions

- **Select** — applies the built condition to every currently **visible** entry and updates the
  selection; your scroll position is kept rather than jumping to the match. Disabled fields (an empty
  Extension/glob, or both Size fields blank, or a non-numeric/non-positive day count) simply produce no
  selection change.
- **Cancel** / **Esc** — closes without changing the selection.
- **Save search…** — the **same** condition you've built can instead (or also) be captured as a named
  **Saved Search**: the first click reveals a name field inline (the criterion picker stays visible, not
  a second modal); a second click (or **Enter** in the name field) saves it. Running the command
  palette's own **"Save search…"** entry opens this dialog with that name field already revealed. See
  [Saved Searches](explorer-saved-searches) for what happens once it's created — where it lives in the
  sidebar, how its live refresh works, and its read-only enforcement.

## Select by pattern…

A single glob field, defaulting to `*.` — clear it first if you want a different starting point.
**Enter** or the **Select** button applies the glob (case-insensitive, `*`/`?` wildcards, same rules as
Search) to the visible list; **Cancel**/**Esc** closes without changing anything.

## Worked example

You're in a Downloads folder and want to select every file **100 MB or larger** so you can review them
before deleting:

1. Command palette (**Ctrl+Shift+P**) → **"Select by…"**.
2. Leave the kind as **Extension**'s default swapped to **Size**.
3. Type `104857600` into **min bytes** (100 MB in raw bytes — this field takes a plain number, not a
   `100mb` shorthand).
4. Click **Select**. Every visible file ≥100 MB is now selected — press **Delete**, or continue
   Ctrl-clicking to fine-tune the set before acting on it.

## Limits / notes

- **Current folder's visible list only.** None of the three tools searches subfolders — they select
  among whatever the file list is currently showing. For a selection across an entire subtree, use
  Search's recursive **Enter** find or **Find files by name** (**Ctrl+P**) and select from those results
  instead.
- **Size fields are raw byte counts.** Unlike the Search box's `size:>100mb` shorthand (see [Searching
  for Files](12-search)), **Select by…**'s Size fields take plain numbers only.
- **Selecting is never undoable** and doesn't need to be — it only changes what's highlighted, not
  anything on disk. See [Undo](safety-undo) for what actually is undoable.
- **Select all `.ext`** only appears for exactly one selected **file** (not a folder); it disappears the
  moment more than one item, or a folder, is selected.
