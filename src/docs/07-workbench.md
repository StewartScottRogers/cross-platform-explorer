---
title: Workbench
order: 7
category: Development
categoryOrder: 11
---

# Workbench

The Workbench is a read-only **diff review overlay**: it runs `git diff` (working tree vs `HEAD`) on
whatever folder the explorer is currently browsing and renders every changed file, hunk-by-hunk, without
you leaving the window. It's the quick way to check what an agent — or you — just did to a folder, right
next to [Agent Watch](explorer-agent-watch)'s live change feed; where Agent Watch shows changes streaming
in as they happen, the Workbench is a point-in-time `git diff` you pull up on demand.

Open it from **Workbench** in the left sidebar. There's no command-palette entry or shortcut — the
sidebar button is the only way in. It always diffs the folder currently open in the main pane (in a
dual-pane layout, that's pane A); it isn't per-selection or per-file, and it has no awareness of pane B.

## The diff

On open it runs `git diff` immediately and shows a loading message while that's in flight. Once loaded,
the titlebar shows the current **branch** (or "detached" with no branch) and, when there are changes, a
summary chip: **`+added −removed · N files`**.

Each changed file gets its own collapsible block:

- **Header** — the file's label (its new path; `old → new` for a rename; `path (new)` for an added file;
  `path (deleted)` for a removed one), a `binary` tag if it's a binary file, and (for text files) a
  per-file **`+added −removed`** count. Click the header — or its **▸/▾** chevron — to collapse or
  expand just that file.
- **Hunks** — each `@@ … @@` header followed by its lines, each tagged added / removed / context and
  shown with the **old and new line numbers** in a fixed gutter. When a removed line is immediately
  followed by an added line (a modified line, not a pure add/delete), the Workbench highlights the exact
  changed **span** within the line — a cheap prefix/suffix comparison, not a full word-diff, so it's
  good for single-token edits but can highlight more than strictly necessary on a heavily rewritten line.

Per file, two buttons sit next to the stat badge:

| Button | What it does |
|---|---|
| **Copy** | Copies that file's diff, reconstructed as a standalone unified-diff patch, to the clipboard. The button flips to "✓ Copied" for about a second. Silently does nothing if the clipboard is unavailable. |
| **Edit** | Opens that file with whatever application your OS has registered for its file type — this is **not** an in-app editor, so a `.rs` file might open in VS Code, a `.txt` in Notepad, or nothing if no handler exists. Opening a file also **closes the Workbench** so you land in your editor without the overlay in the way. |

When there's more than one changed file, the toolbar also gets **Collapse all** and **Expand all**
buttons. **Refresh** re-runs `git diff` from scratch (useful after an agent makes another pass, since the
Workbench doesn't auto-refresh or watch the folder). The **?** help button opens this page; **×** closes
the overlay.

## Address bar (embedded browser)

Below the titlebar, a single address field lets you open a running app **beside** the diff, so you can
compare code and behavior without alt-tabbing. Type a URL — a bare host like `localhost:3000` is accepted
and gets `http://` added automatically — and press **Enter** or click **Open in browser**. Only
**http/https/localhost/IP** targets are accepted (never `file:`, `javascript:`, or anything else); an
invalid entry shows "Enter an http/https or localhost URL." instead of opening anything.

Each click opens a **new, separate OS-level browser window** (1000×720, titled with the URL) — it is a
real webview window, not an iframe inside the app, which is what keeps it safe under the app's strict
content-security policy. Nothing is reused: opening the same URL twice, or two different URLs, opens two
windows. The last URL you typed is remembered (per browser, via local storage) and refilled the next time
you open the Workbench.

## Edge cases

Rather than a raw error, the Workbench shows one of these states in place of the diff body:

- **No folder open** — "Open a folder first": navigate to a project folder, then reopen the Workbench.
- **Git isn't installed** — the Workbench needs `git` on your `PATH`.
- **Not a Git repository** — the current folder has no `.git`; it points you at cloning one from
  **[Repositories](08-repositories)** instead.
- **Couldn't read the diff** — any other `git diff` failure (e.g. a corrupted repo), with the raw message
  shown.
- **No changes** — a green "✓ No changes — ⟨branch⟩ matches HEAD" when the working tree is clean.

## Worked example

An agent just finished a refactor and you want a quick before/after without opening Agent Watch's
history.

1. Navigate the explorer into the project folder, then click **Workbench** in the sidebar.
2. Skim the file list; click a file's header to collapse the ones you don't care about, or **Collapse
   all** to start from a clean slate and expand only what you want to review.
3. Spot a line that needs fixing — click **Edit** to jump straight into your usual editor for that file
   (the overlay closes automatically).
4. Made more changes since? Click **Refresh** rather than closing and reopening the Workbench.

## Limits / notes

- **Read-only, and only `git diff` against `HEAD`.** The Workbench never stages, commits, or reverts
  anything — it can't show a **staged-only** view (`git diff --cached`) or diff against anything other
  than `HEAD`, and it has no relationship to the [two-way sync](08-repositories) Pull/Push/Sync tools.
- **No pagination or virtualization on large diffs.** The whole `git diff` output is fetched and parsed
  client-side in one go; a very large diff loads and renders in full rather than being chunked or
  truncated. Collapsing files hides their hunks from view but doesn't reduce what was already parsed.
- **It doesn't watch the folder.** Unlike Agent Watch's live strip, the Workbench is a snapshot from the
  moment you opened it (or last hit Refresh) — it won't update while an agent keeps editing.
- **Intra-line highlighting is a heuristic**, not a real diff algorithm: it finds the common prefix and
  suffix around a changed span and highlights what's left in the middle. It's accurate for small edits
  and can over-highlight on a line that was substantially rewritten.
- **Every "Open in browser" click is a new window.** There's no single reused preview window — closing
  the Workbench doesn't close windows you've already opened from it, and repeated clicks accumulate
  windows rather than reusing one.
