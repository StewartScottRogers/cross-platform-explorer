---
title: Folder Templates
order: 15
category: Explorer
categoryOrder: 2
---

# Folder Templates — Scaffold the Same Structure in One Click

Some folder layouts you build again and again: a new project with its `src/`, `tests/`, and `README`; a
blog post with its `index.md` and `images/`; a client folder with the same six subfolders every time.
**Folder Templates** capture that structure once and stamp it back out on demand — no more recreating the
same skeleton by hand.

## Opening it

Open the **command palette** (`Ctrl/Cmd+Shift+P`) and run **Folder templates…**. The dialog works on the
folder you're currently viewing: it's both the folder you **capture from** and the folder you **stamp
into**.

## Capturing a template

1. Browse to a folder whose structure you want to reuse.
2. Open **Folder templates…**, type a name (e.g. `rust-crate`), and click **Capture this folder**.

The capture records the tree of subfolders and files. Small text files keep their contents as boilerplate;
large or binary files are captured as empty placeholders, so a template stays small and shareable.

## Stamping a template

1. In the same dialog, select a template from the list (each shows its folder/file counts).
2. Optionally fill in the **`{name}`** value.
3. Click **Stamp here** — the template is materialized into the current folder.

Stamping **never overwrites an existing file** — if a name collides, the stamp stops and tells you, so you
can't clobber your work.

### Variables

Names and file contents can contain `{token}` placeholders that are filled in when you stamp:

- **`{date}`** — today's date (`YYYY-MM-DD`), filled automatically.
- **`{name}`** — whatever you type in the name field before stamping.

For example, a captured file named `{name}-{date}.md` stamps out as `report-2026-07-24.md`.

## Sharing templates

Each template in the list has an **Export** button that copies its JSON to the clipboard — paste it to a
teammate. To bring one in, click **Import…**, paste the JSON, and **Import**. Import accepts either a single
template or a whole exported catalog, merged by name.

Templates are stored in your app config directory, so they persist across restarts and are private to you.
