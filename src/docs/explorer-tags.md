---
title: Tags
order: 48
category: Explorer
categoryOrder: 2
---

# Tags

**Tags** let you attach free-text labels and one colour to any file or folder, independent of where it
lives on disk. They're the foundation two other features build on: a **folder-scoped filter** you toggle
from the sidebar, and [Smart Folders](explorer-smart-folders) — a saved tag query that lists every
tagged item across the whole tag store, wherever it actually lives.

## How to open the editor

Select one or more files/folders and right-click → **Tags…**. There is **no command-palette entry and
no keyboard shortcut** — the context menu is the only opener, the same as [Batch Media](explorer-batch-media).

- **One item selected** — the editor seeds itself from that item's current tags and colour label; **Apply**
  overwrites both.
- **Two or more selected (batch add, CPE-656)** — the editor opens empty. Whatever tags you type are
  **added** to every selected item's existing tags (a non-destructive union, not a replace), and a colour
  you pick is applied to all of them. Leave the colour on **none** and every item keeps its own existing
  label untouched — batch mode never clears a label you didn't touch.

## Editing tags

- Type into the field and press **Enter** to add a tag as a chip; each chip has its own **✕** to remove it.
- **Backspace** on an empty field peels off the last chip, like most tag inputs.
- Any half-typed text still in the field is folded in as a tag automatically when you click **Apply** —
  you don't have to press Enter first.
- Duplicate tags are silently ignored (the same tag can't be added twice to one item).

## Colour label

Below the tag chips, a row of swatches sets a single colour label: **none**, red, orange, yellow, green,
blue, purple, or grey — pick one to replace whichever label the item(s) currently carry. A labelled row
elsewhere in the explorer gets a colour dot plus a soft accent bar in that colour; an unlabelled row looks
exactly as it always did.

## Native metadata sync (opt-in)

When **Settings → Native metadata bridge** is turned on (off by default) and exactly **one** item is
selected, a bordered section appears at the bottom of the editor named after the OS-native store (e.g.
"NTFS alternate data streams", "Finder tags") with two buttons:

- **Pull** — reads that path's OS-native tags/label and merges them into the app's own store
  (non-destructive; only adds, never removes), then re-seeds the editor from the merged result.
- **Push** — applies whatever's in the editor right now (folding in any half-typed tag first), saves it,
  then writes it out to the OS-native store.

This section is hidden entirely in batch mode (native metadata is inherently per-path) and hidden
whenever the bridge setting is off. See [Native Metadata Bridge](17-native-metadata) for the full picture
of what each platform's store actually holds and how Properties' own read-only tag chips relate to it.

## The sidebar Tags section

Once at least one item somewhere is tagged, a **Tags** section appears in the sidebar (hidden entirely
when nothing is tagged — it costs nothing to have zero tags). Each row shows a tag's name and how many
items carry it, most-used first, then alphabetically.

- **Click a tag** to toggle a **folder-scoped filter**: the file list narrows to just the items in the
  *current folder* that carry that tag. This is scoped to the folder you're looking at — it does **not**
  reach across the whole tag store the way a smart folder does. Leaving the folder (navigating anywhere
  else) clears the filter automatically; a small bar above the list shows the active tag, a live count,
  and a **✕** to clear it early. Dual-pane mode keeps each pane's tag filter independent.
- **Right-click a tag** to rename it (across every file that carries it, app-wide), delete it (removes it
  from every file, no confirmation dialog — this is instant, matching the rest of the app's "no modal for
  a reversible action" convention; tags can always be re-added), or **Save as smart folder**, which creates
  a new smart folder named after the tag itself, querying that same tag. Note the rename box only commits
  a *non-empty*, *changed* value — clearing the field and confirming just closes the popover rather than
  deleting the tag; use the separate **Delete** button for that.

## Tags follow the file

Renaming or moving a tagged file or folder **inside the app** re-keys its tags to the new path
automatically — tags never fall off a file just because you renamed it. Renaming a tagged **folder**
carries every tagged item nested inside it too, not just the folder's own entry. Moving or renaming a
file **outside the app** (in another program, or on another machine) breaks this link: the tag store still
has an entry for the old path, and it simply won't match anything until you tag the new path again.

## Import & export

Two command-palette entries (**Application** group) round-trip the whole tag store as JSON:

- **Export tags…** writes every path's tags + label to a JSON file you choose.
- **Import tags…** reads a previously-exported file and **merges** it into your current store: for a path
  present in both, its tags are **unioned** (nothing is dropped), but if the imported entry carries a
  non-empty colour label, that label **replaces** whatever label the path currently has — tags are purely
  additive, the label is not. There's no "replace everything" option.

## Worked example

You're triaging a folder of client deliverables and want to flag the ones that still need review,
regardless of which subfolder they end up in later.

1. Select the three files that need another look, right-click → **Tags…**.
2. Type `needs-review`, press Enter, pick the **yellow** swatch, click **Apply**. All three now show a
   `needs-review` chip and a yellow accent bar.
3. Later, from any folder, right-click `needs-review` in the sidebar's Tags section → **Save as smart
   folder**. Now you have a **Needs Review** entry under Smart Folders that lists all three files no
   matter where you've since moved them — see [Smart Folders](explorer-smart-folders) for what that view
   can (and can't) do.

## Limits / notes

- **No keyboard shortcut or palette entry** for the Tags… editor — right-click is the only way in.
- **Deleting or renaming a tag has no confirmation dialog** — it's instant and applies to every file that
  carries it, app-wide. There is no "delete a tag from just this one file" — remove that one chip instead.
- **Tags live in the app's own store, not on disk**, unless you opt into the native metadata bridge and
  explicitly Pull/Push. A file copied outside the app, sent to someone else, or opened on a machine
  without this app carries no tags with it (unless it was pushed to native metadata first).
- **The sidebar tag filter is folder-scoped and single-tag** — there's no "match any of these tags" or
  "match all of these tags" filter here; for that, use [Select By…](organizing-select-by) or a
  [saved search](explorer-saved-searches) instead.
- **Renaming/deleting a tag outside the app breaks nothing gracefully** — a path no longer on disk simply
  stops appearing anywhere tags are surfaced (its stale entry lingers in the store until you next touch
  that tag's data, which is harmless but not automatically cleaned up).
