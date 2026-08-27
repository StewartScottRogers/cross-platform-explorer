---
title: Macros
order: 41
category: Organizing & Tagging
categoryOrder: 3
---

# Macros

A **macro** chains **Rename → Move → Tag → Convert** steps into one named, reusable action you run
over whatever's selected, instead of performing each step by hand every time. It's built entirely on
the app's own primitives (the same rename/move/tag/convert the rest of the explorer uses), so a run is
previewed before anything happens and can be undone as a single step afterward.

## When to use it

- **A repeated multi-step chore** — "rename these, then move them here, then tag them" — is exactly
  what a macro is for: define it once, run it any time with one click/hotkey.
- **A single step, run once** — just use the ordinary tool directly ([Batch Rename](organizing-batch-rename),
  drag-and-drop move, the tag editor) instead of building a one-off macro.
- **Running an actual external program** (a script, a CLI tool, `git`, `ffmpeg` by hand) is a different
  feature — see [User-Defined Commands](organizing-user-commands). Prefer a macro when a built-in step
  covers what you need: it stays inside the app's own dry-run + Undo safety net, where a user command
  hands a literal line to your OS shell instead.

## How to open it

- **Command palette** (**Ctrl+Shift+P**) → **"Manage macros…"** — the only way to open the macro
  library itself. There's no menu item and no dedicated shortcut for the library.
- **Running** a saved macro is separate from opening the library, and only happens on a surface you
  explicitly turned on for that macro (see *Binding a macro to a surface* below):
  - Right-click a file/folder → **Run macro ▸** submenu — appears only when at least one saved macro is
    bound to the **Menu** surface; it lists just those macros.
  - Command palette → **"Run macro: `<name>`"** — appears only for macros bound to the **Palette** surface.
  - A **hotkey** typed into the macro's row in the library — any combo that includes **Ctrl** or **Alt**
    (a Shift-only or bare-letter combo is rejected, since it would collide with ordinary typing or
    type-ahead find). A macro hotkey is checked **last**, after every built-in shortcut, so it can never
    shadow one. Macro hotkeys follow the same Ctrl/Alt rule as the built-in shortcuts (see
    [Keyboard shortcuts](36-keyboard-shortcuts)).
  - A **brand-new macro isn't exposed anywhere** until you check at least one of Menu/Palette or give it
    a hotkey — saving it alone doesn't make it runnable from any surface.

## The macro library

- **+ New macro** — starts an empty macro (no steps yet).
- **Name** — the macro's identity; it's also the join key everything (bindings, running, export) keys
  off of.
- **Step editor** — pick a kind from the dropdown (**Rename** / **Move** / **Tag** / **Convert**) and
  click **+ Add step**; each step row has:
  - A text field, whose placeholder tells you what it expects:
    - **Rename** — a template using **`{name}`** (full filename), **`{stem}`** (name without
      extension), **`{ext}`** (extension, **no dot**), and **`{n}`** (the item's 1-based position in
      the current selection). So `{stem}-edited.{ext}` on `report.pdf` becomes `report-edited.pdf`.
    - **Move** — the destination folder, typed by hand (no folder picker in this field).
    - **Tag** — the label to attach, using the app's own tag store (see [Native Metadata
      Bridge](17-native-metadata) for how that store relates to OS-native tags).
    - **Convert** — the target extension, **no leading dot**.
    - Any of the four fields may also contain **`{ask:label}`** — instead of baking a fixed value into
      the saved macro, this prompts you for `label`'s value every time the macro runs (see *Run flow*
      below). A macro can mix fixed text and one or more distinct `{ask:...}` labels freely.
  - **↑ / ↓** — reorder the step (steps run top-to-bottom, for every selected item in turn).
  - The trash icon — remove the step.
  - **Save** / **Cancel** for the editor.
- Per-macro row actions: **✎** (edit), **Export** (copies that macro's JSON to the clipboard), and the
  trash icon (delete — immediate, no confirmation).
- **Import…** reveals a paste box and an **Import JSON** button — accepts either one macro's exported
  JSON or a whole exported catalog.

### Binding a macro to a surface

Each row also carries its own binding controls, independent of every other macro:

| Control | Effect |
|---|---|
| **Menu** checkbox | Adds the macro to the right-click **Run macro ▸** submenu. |
| **Palette** checkbox | Adds a **"Run macro: `<name>`"** entry to the command palette. |
| **Hotkey** field | A typed combo (e.g. `Ctrl+Alt+1`); normalized on save. Empty = no hotkey. |

## Run flow

Starting a macro (from any of the three run surfaces above) always goes through the same sequence:

1. **Macro param prompt** — if the macro contains any `{ask:label}` tokens, a small dialog lists one
   labelled text field per distinct label (in first-appearance order). **Cancel** aborts the run;
   **Continue** resolves them (a field left blank resolves to an empty string, never a literal
   `{ask:...}` leaking through). A macro with no `{ask:...}` tokens skips this step entirely.
2. **Dry-run confirm** — lists every planned operation (its kind, the input, and the resolved detail)
   before anything touches disk. Alongside that preview, a read-only scan checks whether any planned
   Rename/Move/Convert destination is already occupied on disk (see *Collisions* below). **Cancel**
   closes without effect; **Run** applies every op, over every selected item, in step order.
3. **Result + Undo** — after a successful run, the dialog shows *"Applied N steps to `<count>`
   item(s)"* and an **Undo** button. Clicking it reverses the **entire run** in one action — this is a
   **separate** undo from the app-wide **Ctrl+Z** stack (see [Undo](safety-undo)): a macro run is applied
   as one backend operation and is **never** placed on the Ctrl+Z stack. So once you close this dialog,
   that one-click "undo this run" option is gone and there is **no fallback** — Ctrl+Z will not reverse
   it. If you want a safety net, take a **Checkpoint** first (see *Limits* below). **Close** dismisses the
   dialog either way.

## Worked example

You want a one-click way to file screenshots into a per-project archive folder:

1. Command palette → **"Manage macros…"** → **+ New macro**. Name it `Archive screenshots`.
2. Add a **Rename** step with template `{ask:project}-{stem}.{ext}`.
3. Add a **Move** step with destination `Screenshots\Archive`.
4. **Save**, then check the **Menu** box on the new row so it appears in the right-click submenu.
5. Select a batch of PNGs, right-click → **Run macro ▸ Archive screenshots**.
6. A "project" prompt appears — type `Q3`, click **Continue**.
7. The dry-run plan shows a rename then a move for each file; click **Run**.
8. Each screenshot is renamed `Q3-<original-name>.png` and moved into `Screenshots\Archive`. Click
   **Undo** right away to put everything back, or **Close** to keep it.

## Collisions — an occupied name doesn't have to abort the whole run

A Rename/Move/Convert step refuses to write over a destination that's already occupied, rather than
silently clobbering it — same as the rest of the app. Left alone, one collision partway through a large
batch would abort and roll back *everything already applied*, with no way to proceed short of finding
and renaming the colliding file by hand. The dry-run confirm avoids that:

- Every colliding destination the macro would hit is listed **before Run is even clickable** — not
  discovered one at a time by running, failing, and retrying.
- A collision with a **plain, ordinary file** already at that name is **confirmable**: check the
  **"Overwrite these files"** box (an inline checkbox, not a separate dialog) and the button becomes
  **"Overwrite N and Run"**. Confirming re-runs with permission to replace those specific files' bytes.
- A collision with a **link** (a shortcut, symlink, or similar) is listed the same way but is **never**
  confirmable — no checkbox unblocks it, and Run stays disabled while one is present. Writing through a
  link would put the bytes somewhere other than the name you picked, so this refusal doesn't have an
  override; remove or rename the link first if that's really what you meant.
- **Copy all N names** copies every colliding destination to the clipboard, one per line, for a batch
  larger than the on-screen preview shows.

## Limits / notes

- **Convert really re-encodes the file** (the same conversion the app uses elsewhere) — it isn't a bare
  extension rename. The step trashes the pre-convert original (rather than deleting it outright) so its
  inverse can restore those exact original bytes from the OS trash instead of re-encoding backward — a
  genuine undo, not a lossy round-trip.
- **Scope guard.** Every resolved rename/move/convert destination must land inside the folder the macro
  was run from — a maliciously- or accidentally-crafted rename template containing `..`/path separators
  can't write outside it. This is checked before anything runs.
- **No pickers.** The Move destination and any `{ask:label}` answer are plain text fields — there's no
  Browse dialog for either.
- **The run-flow Undo is separate from Ctrl+Z.** It only exists in the confirm dialog right after a run;
  it is not a new entry on the app-wide undo stack described in [Undo](safety-undo).
- A macro's Menu/Palette checkboxes and hotkey are all **off by default** for a new macro — nothing is
  exposed until you turn at least one on.
- See [Batch Rename](organizing-batch-rename) for renaming alone (richer find/replace, numbering, case
  modes) and [User-Defined Commands](organizing-user-commands) for running an actual external program
  instead of an in-app step.
