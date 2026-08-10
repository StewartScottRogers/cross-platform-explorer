---
title: Searching for Files
order: 12
category: Search & Discovery
categoryOrder: 4
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

## Search file contents — ranked results from a local content index

Open the command palette and choose **"Search file contents…"** for a third way to search inside files —
this one ranks results by how well a file's *indexed text* matches your query, instead of listing every
literal line that contains it (that's "Search in files" above). It's built on a small, **dependency-free
local model that runs entirely on this device** — no API key, no network call, nothing leaves your
machine. It's honestly a **content search**, not a magic mind-reader: it scores files by shared words/
phrases with your query, so results improve when the query uses words likely to actually appear in the
file (the underlying model is pluggable, so a stronger one can be dropped in later without changing this
UI).

- **Build the index first.** A folder needs a **content index** before it can be searched this way — the
  first time you open it for a folder with none yet, you'll see a **"Build content index"** prompt
  instead of an error. Building walks the folder's text-like files (skipping binaries and anything
  oversized) and shows live progress (files indexed so far) without freezing the app; once it finishes,
  the query box unlocks.
- **Type to search.** Once an index exists, results appear after a short pause as you type — no need to
  press Enter. Each result shows the **file name**, its **path relative to the folder**, a **match-score
  bar** (how strong a hit it is), and a **snippet** of the matching text. Click a result to jump to it
  (its folder opens and the file is selected).
- **Refresh the index** any time from the **Rebuild index** button in the dialog's header — useful after
  adding, editing, or removing a lot of files, since the index isn't kept live automatically.
- If a search comes back with **no matches**, that means the index has no file scoring above zero for
  that query — try different wording, or rebuild the index if files changed recently.

### Use a real embeddings model (optional)

By default, "Search file contents" uses a small **built-in local model** — no key, no network, nothing
leaves your machine. If you'd like stronger, more meaning-aware ranking, you can point it at a **real
embeddings model** instead, in **Settings → AI content search**. It works with **any OpenAI-compatible
`/embeddings` endpoint**:

- **A local server (no key).** Run something like **LM Studio** or **Ollama** with an embedding model
  loaded, then set the **Endpoint URL** to that server's address (for LM Studio, `http://localhost:1234/v1`)
  and the **Model** to the embedding model's name. Leave the **API key** blank — a local server needs none.
  Nothing leaves your machine.
- **OpenAI or another hosted provider (with a key).** Set the **Endpoint URL** (e.g.
  `https://api.openai.com/v1`), the **Model** (e.g. `text-embedding-3-small`), and paste your **API key**.
  The key is stored only in your operating system's secure **keychain** — never in a settings file, never
  in a log, and it's never shown back to you.

Click **Test connection** to check the endpoint is reachable and see the vector size it reports. Then
turn the section **on**. The endpoint URL may be given **with or without** the `/v1` segment — both work.

> **Switching models rebuilds the index.** Each folder's content index is tied to the exact model that
> built it, so when you enable a real model (or change the endpoint/model), the folder will show the
> **"Build content index"** prompt again the next time you search it — build it once with the new model
> and searches are fast again. Turning the feature back **off** returns to the built-in local model. If
> the endpoint is unreachable when you build or search, you'll get a clear error rather than wrong
> results — check that your server is running and the URL/model are correct.

## Instant search (Ctrl+K) — every indexed folder, any drive

The two search boxes above both work **inside** a folder you've opened. **Instant Search** (**Ctrl+K**,
from anywhere — including the Home screen) is different: it searches an in-memory **index** of file
names built ahead of time, so matches across an entire drive appear **as you type**, instead of walking
the tree live.

- Press **Ctrl+K** to open the overlay: a single box, keyboard-first — **↑/↓** to move, **Enter** to
  reveal the selected file in its folder (and select it), **Esc** to close.
- Results stream in and **re-rank live**; typing further narrows them without waiting for a full search
  to finish, and an older still-running search is dropped the moment a newer keystroke supersedes it.
- **Off means off.** Nothing is indexed until you ask. The first time you open Instant Search on a drive
  with no index yet, you'll see a **"Build index"** action instead of a blank list — click it to crawl
  that drive once (with live progress); after that, searches against it are instant. No index is ever
  built silently in the background.
- The index only covers **file and folder names** (not contents) — for "find text inside my files" use
  **Search in files** above.

## Tips

- Very large trees are **capped** for responsiveness; if a search is truncated the results panel says so —
  narrow the query to see everything.
- The recursive find skips hidden system dot-folders and symlinked loops, and never fails the whole search
  on a single unreadable folder.
- Prefer to keep a search open in its own panel? The same recursive find is available any time with
  **Ctrl+P** ("Find files by name").
