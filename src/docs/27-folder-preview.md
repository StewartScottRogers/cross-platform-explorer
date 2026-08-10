---
title: Folder Preview (Peek)
order: 27
category: Previews & Media
categoryOrder: 6
---

# Folder Preview (Peek)

Highlight a folder in the main file list — with a click or the arrow keys — and the preview pane shows
that folder's contents one level down, without navigating into it. It's a "peek": the main list stays
right where it is, and the preview pane becomes a browsable view of what's inside the highlighted
folder.

## Walking a tree from the preview pane

Click a **subfolder** inside the peek and the main list descends into the folder you had highlighted,
landing with that subfolder selected — and the preview pane immediately peeks one level further, into
it. Click another subfolder there and the list descends again. Repeat, and you can walk an entire
folder tree one click at a time without ever clicking back into the main list — the same column-view
feel as Finder's Miller columns.

Click a **file** inside the peek and the main list descends into the folder the same way, landing with
that file selected — the preview pane then shows the file's normal preview (text, image, hex, whatever
its type opens as), exactly as if you'd clicked it in the main list yourself. Double-click a file in the
peek to open it directly, same as double-clicking it anywhere else.

Back, Forward, the breadcrumb, and the sidebar all update normally as you walk down — this is real
navigation, not a separate mode.

## Behaviour notes

- The peek loads a short moment after you settle on a folder (rather than on every single arrow-key
  step), so scrolling quickly through a long list doesn't fire off a filesystem read per row.
- The peek always shows **everything** one level down — an active file-type filter on the main list
  never hides anything from the peek.
- An empty folder shows a plain "empty" note; a folder that can't be read (permissions, a broken
  link) shows a plain "can't open" note. Neither ever pops an error dialog.
- Selecting a **file** instead of a folder shows that file's normal preview, unchanged — the peek only
  replaces the preview for a highlighted directory.
