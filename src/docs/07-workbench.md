---
title: Workbench
order: 7
---

# Workbench

The Workbench reviews an agent's (or your own) changes without leaving the window. Open it from
**Workbench** in the left sidebar; it reads the current folder's Git changes.

## Diff

The Workbench runs `git diff` (working tree vs HEAD) and shows each changed file with its hunks and
**added / removed / context** lines, plus a summary of `+added −removed · files` and the current branch.

## Edit and browse

- **Edit** on a file opens it in the editor.
- The **address bar** opens a URL (e.g. `localhost:3000`) in a dedicated browser window, so you can view
  your running app beside the diff. Only http/https/localhost URLs are accepted.

## Edge cases

The Workbench tells you clearly when a folder **isn't a Git repository**, when there are **no changes**
("branch matches HEAD"), when **Git isn't installed**, or when **no folder** is open — rather than
showing a raw error.
