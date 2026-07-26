---
title: Searching for Files
order: 12
category: Explorer
categoryOrder: 2
---

# Searching for Files

The **Search box** on the navigation toolbar (the one with the magnifying glass) works like the Windows
File Explorer search — but with a clear split between an instant filter and a deep, recursive find.

## Two ways it works

- **Type to filter the current folder.** As you type, the file list narrows to items in the *current
  folder* whose name matches — instant, no waiting. Clear the box (or press **Esc**) to show everything
  again.
- **Press Enter to search subfolders.** Hit **Enter** and the app runs a **recursive** search of the
  current folder *and everything beneath it*, listing every match with its location. Results stream in as
  the tree is walked, so hits appear immediately on large folders. Click a result to jump straight to that
  file (its folder opens and the file is selected).

> At the **Home** screen there is no folder to search inside, so open a drive or folder first, then search.

## Query syntax

Search is **case-insensitive** and supports the same wildcards you know from Windows:

| You type | It matches |
|---|---|
| `report` | any name **containing** `report` — `Report.docx`, `2024-report.pdf`, `reporting/` |
| `*.md` | names **ending** in `.md` |
| `report.*` | `report` with **any** extension |
| `img_????.jpg` | `img_` + exactly **four** characters + `.jpg` (each `?` is one character) |
| `*.{jpg,png,gif}` | any of several extensions — brace groups expand to a list |
| `2024-*-invoice*` | `*` matches any run of characters, anywhere in the pattern |

**Rules of thumb**

- `*` matches any run of characters (including none); `?` matches exactly one character.
- `{a,b,c}` is a **brace group** — it matches any of the comma-separated alternatives.
- With **no** wildcards, the query is a plain **substring** match (like typing part of a name).
- A wildcard query is **anchored** — `*.md` means "ends with `.md`", not "contains `.md` somewhere".

## Power-filters (instant filter only)

Typing to filter the current folder also understands a small set of `key:value` filters, plus boolean
`OR`/`NOT`/parentheses to combine them. These apply to the **instant filter** (the type-as-you-go
narrowing of the current folder); the recursive **Enter** search still uses plain name/glob matching.

| Filter | Examples | Matches |
|---|---|---|
| `size:` | `size:>1mb`, `size:<=500k`, `size:1mb..1gb`, `size:2.5g` | File size. Units are `k`/`kb`, `m`/`mb`, `g`/`gb`, `t`/`tb` (1024-based, case-insensitive); a bare number is bytes. Operators: `>`, `<`, `>=`, `<=`, `=` (default with no operator); `lo..hi` is an inclusive range. |
| `date:` / `modified:` | `date:today`, `date:yesterday`, `modified:<7d`, `modified:>1w`, `date:2024`, `date:2024-07`, `date:2024-07-25` | Last-modified time. Relative windows use `d`/`w`/`m`/`y` (day/week/~30-day month/~365-day year); `<7d` means "modified within the last 7 days", `>1w` means "older than a week". Absolute forms match a whole year/month/day. Files with no reported modified time never match a date filter. |
| `type:` | `type:image`, `type:image,video` | The file's extension classified as `image`, `video`, `audio`, `document`, `archive`, `code`, or `executable`. A comma list matches any of them. |
| `ext:` | `ext:png`, `ext:png,jpg` | The exact extension (a leading dot is fine: `ext:.png`), comma list matches any of them. |
| `path:` | `path:reports` | A substring anywhere in the entry's full path, not just its name. |

**Combining filters** — terms side by side are ANDed (`size:>1mb type:image` = big **and** an image).
Add boolean structure with:

- **`OR`** — either side matches: `type:image OR type:video`.
- **`NOT`** or a leading **`-`** (no space) — negates the next term: `NOT type:archive`, `-tmp`.
- **`( … )`** — groups a sub-expression to override the default precedence (`OR` binds loosest, then
  `AND`/juxtaposition, then `NOT`/`-`): `(type:image OR type:video) size:>1mb -tmp`.

A plain word (no recognised `key:` prefix) is still matched as a name/glob term exactly as above, so
`report OR type:pdf` works — `report` matches by name, `type:pdf` by file type.

## Searching inside files (content search)

Besides finding files by **name**, you can search their **contents**. Both search boxes carry a small
**book (Docs) button** in their header that opens this page.

- **Find files by name** (**Ctrl+P**) — the recursive name/glob find described above, in its own panel.
- **Search in files** — searches the **text inside** files under the current folder and groups the hits by
  file; click a hit to jump to its file.
  - **Match case (`Aa`)** — off by default (case-insensitive). Toggle it on to match capitalisation
    exactly; the choice is remembered across searches.
  - **Filter files** — when many files match, a filter box narrows the *result* list by file name.

Both boxes remember your **recent queries** — start typing to pick one from the drop-down. Results
**stream in** as the folder tree is walked, so matches appear immediately even on large trees.

## Tips

- Very large trees are **capped** for responsiveness; if a search is truncated the results panel says so —
  narrow the query to see everything.
- The recursive find skips hidden system dot-folders and symlinked loops, and never fails the whole search
  on a single unreadable folder.
- Prefer to keep a search open in its own panel? The same recursive find is available any time with
  **Ctrl+P** ("Find files by name").
