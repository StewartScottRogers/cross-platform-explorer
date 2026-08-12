---
title: Undo
order: 39
category: Safety & Recovery
categoryOrder: 5
---

# Undo

**Ctrl+Z** reverses your last file operation — a rename, a move, or a delete you sent to the Recycle
Bin/Trash. It is a safety net for the everyday "wrong click" (renamed the wrong file, dragged something
into the wrong folder, deleted something you needed) without you having to dig through the Recycle Bin
by hand.

## When to use it (and when it can't help)

Undo only exists for operations that are safe to reverse automatically:

| Operation | Undoable? | Why |
|---|---|---|
| **Rename** (F2, or Batch Rename) | Yes | Reversed by renaming back — nothing is destroyed. |
| **Move** (cut+paste, drag-and-drop, dual-pane Commander move) | Yes | Reversed by moving back to where it came from. |
| **Delete to the Recycle Bin/Trash** | Yes, *where the OS supports restoring from trash* | See *Platform limits* below — not guaranteed everywhere. |
| **Copy** (Ctrl+C/Ctrl+V, Duplicate/Ctrl+D) | **No** | "Undoing" a copy would mean deleting the file it just created — but if the destination already had a same-named file, or you've since edited the copy, that would destroy real data to reverse a harmless action. The app refuses rather than guess. |
| **Permanent delete** (Shift+Delete, "Securely delete…") | **No** | Bytes are gone from disk on purpose — there is nothing left to restore. |

If you need to reverse a copy or a permanent delete, Undo is the wrong tool — see [Checkpoints &
Rollback](16-checkpoints) for a way to protect a folder *before* a risky operation instead.

### The permanent-delete confirmation is enforced end-to-end (CPE-1651)

Because a permanent delete can't be undone, the confirmation isn't just a dialog: the backend **refuses
to delete anything** unless it is explicitly told that confirmation happened. Exactly three things in the
app can say so, and each of them is you making a decision: the "Delete permanently?" dialog's button, the
Repair Link dialog's replace confirmation, and pressing **Undo** on a folder-watch rule (which removes
only the copies that rule just made). The same rule already
covers [secure delete / shred](20-vaults), creating a vault with "securely delete the original"
checked, [emptying the Trash](38-trash), and [batch-media overwrites](explorer-batch-media) — every
operation that destroys bytes with no way back asks the same question, and none of them will act on an
answer they weren't given. You won't notice this in normal use; it exists so nothing else that can
reach the app's internals — a script, an automation, a browser console — can quietly skip the question
you would have been asked.

### Three more operations now work the same way (CPE-1662, CPE-1664, CPE-1665)

The same enforcement was extended to the other three places where the app destroys something, or starts
something, that no Undo can walk back:

| Operation | What it can destroy | Who is allowed to answer for you |
|---|---|---|
| **Copy/move with "Replace"** | Whatever already sits at the destination — recursively, for a folder. Not sent to the Recycle Bin. | Only the **Replace** button in the copy-conflict dialog ("Some items already exist"). |
| **Running a backup job** | A *mirror* job deletes files under the destination that are no longer in the source. | Only the Backup jobs dashboard's **Run** / **Restore** buttons, or a job you ticked **auto-run on connect** for. |
| **[User commands](organizing-user-commands)** | Anything — the command line runs as a real external process on your machine. | Only the **Run** button in the confirm dialog, after it has shown you the exact command line. |

Two details are worth knowing, because they are deliberate:

- **Only the destructive choice asks.** A copy that keeps both files, or skips the ones that collide,
  destroys nothing and is not gated — nothing new to click. A prompt on every copy would just teach you
  to click past it, which is worse than no prompt at all.
- **What this is, honestly.** It is the app's own discipline written into the backend rather than
  promised in the interface: a call that skips the dialog is refused, and an old script written against
  the previous version is refused outright because it doesn't carry the answer at all. It is *not* a
  security boundary against something that has already taken over the app — that would need a different
  mechanism, and the app doesn't claim to have one here. What it reliably prevents is any part of the
  app — now or in a future version — quietly performing one of these operations without asking you.

## How to open it

- **Keyboard: Ctrl+Z**, from anywhere in the file list. The chord can be rebound in **Settings →
  Keyboard shortcuts** like any other action — see [Keyboard shortcuts](36-keyboard-shortcuts).
- **Right-click an empty area** of the file list (not on a file/folder) and choose **Undo**. The menu
  item shows what it will undo — e.g. *"Undo Rename to "report.txt""* — and is greyed out when there is
  nothing to undo.
- There is currently **no command-palette entry** for Undo — the context menu and Ctrl+Z are the only
  two openers.

## What happens when you undo

Pressing Ctrl+Z (or clicking **Undo**) pops the most recent entry off the undo stack and reverses it:

- A **rename/move** entry is reversed by moving every affected file back to where it came from, in one
  step (a batch rename of 12 files undoes as a single Ctrl+Z, not 12).
- A **delete** entry is reversed by asking the OS to restore the deleted files from the Recycle
  Bin/Trash.

A toast confirms what was undone (*"Undone: Rename to "report.txt""*), or reports *"Nothing to undo"*
if the stack is empty. If the reversal itself fails partway (e.g. something now occupies the original
path), the app reports the failure and leaves that entry **on the stack** rather than silently dropping
it, so you can clear the obstruction and try Ctrl+Z again.

## Worked example

1. You select 5 photos and rename them with [Batch Rename](organizing-batch-rename) — say, adding a
   `-2024` suffix to each.
2. You notice one of the 5 shouldn't have been included.
3. Press **Ctrl+Z**. All 5 files are renamed back to their original names in one step — you don't have
   to manually rename any of them, and you don't need to remember what the "before" names were.

## The stack

Every undoable operation is pushed onto a single, app-wide stack (not per-folder or per-pane) — the
same **Ctrl+Z** always undoes whatever you did most recently, wherever it happened. The stack holds up
to **25** entries; once full, the oldest entry is dropped to make room for a new one. There is currently
no **redo** (no way to re-apply an operation after undoing it) and no way to browse or jump to an
arbitrary entry in the stack — only "undo the most recent one" is offered.

## Limits / notes

- **Not available everywhere.** Undo is blocked while you're inside an archive preview, a smart folder
  (saved-search view), a saved search, or Replay — these are read-only or virtual views with nothing
  real to reverse; the app tells you why rather than silently doing nothing.
- **Platform limits on delete.** Restoring from the Recycle Bin/Trash is only implemented on **Windows
  and Linux**. On **macOS**, the app cannot programmatically restore a trashed file, so a delete-to-trash
  there is **never pushed onto the undo stack** — Ctrl+Z simply moves on to whatever came before it,
  instead of offering an Undo that would do nothing. The file is still safely in the Trash; you'd
  restore it from the Trash yourself, same as before this feature existed.
  - Delete-to-trash from anywhere — the file list, the Disk usage/Space analyzer treemap, or a Home
    pinned/favorite item — shares this same stack and the same platform limit.
- **Copies and permanent deletes are deliberately excluded**, not a missing feature — see the table
  above. If you want to protect a folder before an operation that Undo can't reverse, take a
  [checkpoint](16-checkpoints) first.
- **25-entry cap, no redo, no history browser.** A long editing session can push earlier undoable
  operations out of reach; there is no list of "everything undoable right now" to check before you
  press Ctrl+Z, only the toast's one-line label.
- If an undo itself fails (e.g. the original path is now occupied by something else), the entry stays on
  the stack for a retry rather than being discarded.
