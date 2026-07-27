---
title: Checkpoints & Rollback
order: 16
category: Explorer
categoryOrder: 2
---

# Checkpoints & Rollback

**Checkpoints** capture the state of a folder tree so you can revert to it later — a manual save point
you take before an agent, a script, or your own risky edit touches a folder, so you always have a way
back.

Open it from the **command palette** (Ctrl/Cmd+K → *Checkpoint & rollback…*).

## Creating a checkpoint

1. Point the dialog at the folder to protect (it starts on the folder you're currently viewing).
2. Optionally give it a label (`"before refactor"`, `"clean state"`, …) — blank is fine too.
3. Click **Create checkpoint**. The tree is captured into your per-folder checkpoint store; identical
   file content already stored by an earlier checkpoint is reused rather than duplicated, so a series of
   checkpoints on the same folder stays cheap. Any file skipped (too large, or the capture budget would be
   exceeded) is reported by name rather than silently dropped.

## Previewing a revert

Every checkpoint in the list has a **Preview** button. It shows what reverting to that checkpoint would
do, *without changing anything*:

- **creates / overwrites / deletes** — how many files would be recreated, overwritten, or removed, and
  how many bytes would be written back.
- **drift** — paths that have changed since the checkpoint but can't be attributed to a specific agent
  session. Drift is called out separately because it's the case most likely to surprise you: reverting
  will overwrite or remove those files too.

Always preview before reverting a folder you've kept working in — the drift count is exactly the number
of files whose current state you'd lose.

## Reverting

Two ways to revert, both from the checkpoint's row in the list:

- **Revert…** — reverts the **whole tree** under the root back to that checkpoint.
- **Revert this path…** — type a path under the root and revert **only that path**, leaving the rest of
  the tree alone. Useful when preview shows drift you want to keep everywhere except one file.

Both are destructive — files are overwritten or deleted on disk directly, not moved to the Recycle Bin —
so both arm a confirmation panel first, restating what will happen. Nothing reverts on a single click.
After a revert, the dialog reports how many changes were applied and how many were skipped (e.g. a locked
or missing file); a skip never fails the rest of the revert.

## What this is (and isn't)

This is the palette-driven, headless-friendly way to create and use checkpoints. A richer visual restore
panel — with timeline markers showing checkpoints alongside an agent's activity — is a separate, deferred
feature; this dialog covers the same underlying commands with a minimal UI.
