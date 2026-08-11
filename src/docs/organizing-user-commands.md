---
title: User-Defined Commands
order: 43
category: Organizing & Tagging
categoryOrder: 3
---

# User-Defined Commands

A **user command** is a named, reusable **shell command template** you define once — e.g. `code
"{path}"` named "Open in VS Code" — and run over the current selection from wherever you've chosen it
to surface: the command palette, the right-click context menu, and/or the toolbar. Every run shows you
the exact, fully-resolved command line(s) it's about to execute and requires an explicit click before
anything runs.

> **Safety framing — this runs real shell commands.** A user command is passed to your OS's shell
> exactly as written: `cmd /C` on Windows, `sh -c` on macOS/Linux. Anything valid in that shell — pipes,
> redirects, `&&`, backticks, `$(...)` — is honored exactly as typed, with the **same privileges as the
> app itself**. There is no sandbox and no allow-list of "safe" programs. This is why every run goes
> through the mandatory confirm dialog showing the literal command line first, and why there's **no
> Undo** for it (see *Limits* below) — treat a saved user command exactly as you would typing that same
> line into a terminal yourself.

## When to use it (vs. a Macro)

- Use a **[Macro](organizing-macros)** first if a built-in step (rename/move/tag/convert) covers what
  you need — it stays inside the app's own dry-run-then-Undo safety net.
- Use a **user command** when you actually need to invoke an external program or script — opening a
  file in a specific editor, running a linter, kicking off `git`, `ffmpeg`, a custom script, etc.

## How to open it

- Command palette (**Ctrl+Shift+P**) → **"Manage user commands…"** — the only way to open the manager
  itself; there's no menu item and no dedicated shortcut for it.
- **Running** a saved command: pick it up on whichever surface(s) you checked for it —
  - **Palette** — the command palette lists it by the exact **Name** you gave it.
  - **Context** — right-click a selection; bound commands appear under a **"Run command ▸"** submenu
    (hidden entirely if none are bound to Context).
  - **Toolbar** — a button per bound command appears on the toolbar, labelled with its **Name**.

  Every surface runs the same command over the same selection and opens the same confirm dialog.

## The manager

- **+ New command** reveals the editor; **✎** on an existing row re-opens it for editing.
- **Name** — the label shown wherever the command surfaces (the palette entry, the context-menu row, and
  the toolbar button all use it).
- **Command template** — the literal command line. Placeholders, substituted per selected item:

  | Token | Value |
  |---|---|
  | `{path}` | The full path. |
  | `{name}` | The filename (with extension). |
  | `{dir}` | The containing folder's path. |
  | `{ext}` | The extension, **no dot**. |
  | `{stem}` | The filename **without** its extension. |

  An unrecognized `{token}` is left in the output verbatim (not treated as an error), and `{{`/`}}`
  escape to literal `{`/`}`.
- **Run** — **once per item** (the template is expanded and run separately for each selected entry) or
  **once (joined)** (the template is expanded **once**, with each known token replaced by every selected
  entry's value, space-joined and individually double-quoted — e.g. `sha256sum {path}` over two files
  becomes one line, `sha256sum "a" "b"`).
- **Show in** — **toolbar** / **context** / **palette** checkboxes, each independently wired (check any
  combination); a command's row displays a pill per surface it's checked for, plus a pill for its run
  mode. A brand-new command starts with **Context** and **Palette** both checked, so it's reachable
  immediately by right-click and by search — clearing every checkbox before saving falls back to that
  same pair rather than persisting a command bound to nothing.
- **↑ / ↓** reorder a command (its order in whichever surface shows it); the trash icon removes it
  immediately, no confirmation (this only deletes the saved *template* — see *Limits*).
- Closing the manager: the header's **✕**, **Esc**, or clicking outside the dialog — there's no separate
  bottom "Close" button.

## Running a command

Picking a command opens the **confirm dialog**, which never auto-runs anything:

1. It lists the **exact, resolved command line(s)** that would run, and states how many external
   commands this is and which folder they'd run in (the folder you're currently viewing; blank at
   Home, which falls back to the app's own default working directory). An empty selection shows
   *"Nothing to run for the current selection."* and disables **Run**.
2. **Cancel** closes without running anything; **Run** executes every listed line and switches the
   dialog to show, per command: its **exit code** (or "signal" if it didn't exit normally), captured
   **stdout**, and **stderr** — each capped, with an "output truncated" note if the cap was hit.
3. **Close** dismisses the results.

## Worked example

You want a one-click way to open a file in VS Code from inside the app:

1. Command palette → **"Manage user commands…"** → **+ New command**.
2. Name: `Open in VS Code`. Template: `code "{path}"`. Leave **Run** as **once per item**.
3. Leave **Palette** checked (the default) and also check **Context** if you'd rather reach it from a
   right-click. **Save**.
4. Select two files, then either command palette → **"Open in VS Code"**, or right-click the selection →
   **Run command ▸** → **Open in VS Code**.
5. The confirm dialog lists two lines, one per file (`code "C:\...\a.txt"`, `code "C:\...\b.txt"`).
6. Click **Run** — both files open in VS Code, and the dialog then shows each command's exit code so you
   can confirm both succeeded.

## Limits / notes

- **All three surfaces run the exact same flow.** Palette, Context, and Toolbar each just add another
  entry point to the same confirm-before-launch dialog over the same selection — there's no per-surface
  behavior difference to remember.
- **The Toolbar surface has no submenu.** Unlike Context (which tucks bound commands under one
  "Run command ▸" row so it never crowds the menu), every Toolbar-bound command gets its own always-
  visible button. Bind a command to Toolbar sparingly if you have many of them.
- **No sandbox, no allow-list.** The command runs through a full OS shell with the app's own privileges;
  the confirm dialog showing the literal line is the entire safety gate — there's no second warning for
  a command that looks destructive.
- **Not undoable.** Running a user command is opaque to the app (it's an external process) — nothing is
  pushed onto the [Undo](safety-undo) (**Ctrl+Z**) stack, and there is no way to "revert" whatever the
  command actually did. If you're about to run something that might be destructive, take a
  [Checkpoint](16-checkpoints) of the folder first.
- **Output is capped**, not unlimited — a command that produces a lot of stdout/stderr shows "output
  truncated" past roughly a megabyte per stream, rather than the full log.
- **`{ext}`/`{stem}` behave like elsewhere in the app**: `{ext}` never includes the leading dot; a
  filename with no `.` (or a leading-dot name like `.gitignore`) has an empty `{ext}` and `{stem}` equal
  to the whole name.
- Deleting a command (trash icon) only removes the saved template — it has no effect on anything a
  previous run of it already did.
