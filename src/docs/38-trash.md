---
title: Trash
order: 38
category: Safety & Recovery
categoryOrder: 5
---

# Trash

The **Trash** section in the left-hand sidebar lets you browse what's actually sitting in your
operating system's Recycle Bin (Windows) or Trash (Linux) — without leaving the app — and restore
or permanently delete items from it.

## Opening it

Click **Trash** in the sidebar to open the Trash view. It lists every item currently in the OS
Trash with three columns:

- **Name** — the item's file/folder name (plus its size, when known).
- **Original location** — the full path it was deleted from.
- **Deleted** — when it was moved to the Trash.

A large Trash paints progressively as it loads, so opening it never blocks on a big listing.

If the listing shows **"Trash couldn't be fully read — it may not be empty,"** the OS trash itself
couldn't be read this time (for example, one malformed entry on disk) — it does not mean your Trash
is actually empty, and it's a different message from "Trash is empty" on purpose. Nothing in this
view can fix that condition; try **Refresh**, since it can clear on its own once the underlying issue
goes away, and any items still visible in the list remain safe to restore.

A second message, **"3 items in the Trash couldn't be read and aren't shown,"** means individual
items were dropped from this listing because their name or original location isn't text this app can
represent — the count tells you exactly how many are missing. It appears **above the list**, not
instead of it, so the items that could be read are still shown and still restorable; the Trash simply
holds more than you can see here. If every item in the Trash is affected the list will be empty, but
you will get this message rather than "Trash is empty" — an unreadable name never causes the app to
report your Trash as empty when it isn't. The item count in the title bar is hidden whenever either
message is showing, since the real total isn't known.

## Restoring items

Check the box next to one or more items (or use **Select all**), then click **Restore selected**.
Each item is restored back to its original location individually — if something else now occupies
that spot, or the item is otherwise no longer restorable, that single item is reported as failed
without affecting the rest of your selection.

## Emptying the Trash

- **Delete selected permanently** removes just the checked items.
- **Empty Trash** removes everything.

Both are **permanent and cannot be undone**, so either one opens a confirmation dialog before
anything is deleted.

That confirmation is enforced by the backend too, not only by the dialog (CPE-1651): the purge refuses
to run at all unless it is explicitly told you confirmed, and this dialog is the only thing in the app
that says so. The rule covers **both** buttons — purging a few checked items destroys those items just
as irrecoverably as emptying the lot. See [Undo](safety-undo) for the same guarantee across every other
operation that can't be reversed.

## Platform support

Browsing the Trash from within the app is available on **Windows and Linux**. On **macOS**, the
sidebar shows a row pointing you to Finder's own Trash instead — macOS doesn't expose the listing
API this feature relies on, so rather than show a broken or permanently empty view, the app tells
you where to go instead.

This is separate from the app's regular delete-to-Trash behavior, which works everywhere: deleting
a file normally sends it to the OS Trash first (recoverable), and only Shift-Delete / a permanent
delete bypasses it. See [Undo](safety-undo) for how that fits into the undo stack.
