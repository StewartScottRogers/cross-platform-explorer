---
id: CPE-1737
title: An S3 object and prefix sharing a name produce two rows with an identical path, which the keyed each cannot render
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-15
---

Filed from the CPE-1727 (PR #903) UAT round 3, which measured the listing and found the duplicate. The
UAT flagged it as "a hazard for any UI keyed on name"; tracing it through the pipeline turned out to make
it sharper than that.

## What was measured

An S3 bucket can hold an object named `photos` **and** objects under `photos/` — keys are just strings.
`ListObjectsV2` returns them independently (`<Contents>` and `<CommonPrefixes>`), so `list("/")` yields
two entries with the same name:

```
listing of "/" = ["name=photos is_dir=false size=4",
                  "name=photos is_dir=true  size=0"]
```

Pinned by `cpe_s3::provider::tests::a_name_collision_lists_as_two_rows_so_the_user_has_already_said_which_one_they_meant`.

## Why it is worse than a display oddity

Read forward from that listing (this is the part the UAT did not have to check, and it is what raises the
priority):

- `crates/vfs/src/connect.rs:244` — `dir_entry_from_provider` builds `path: child_uri(uri, loc, &e.name)`.
  The path is derived from the **name alone**, so both rows get the **identical** `path`.
- **Three components key directly on it**, so this is not confined to the main pane (all three verified in
  the tree by the PR #903 UAT, round 4):
  - `src/lib/components/FileList.svelte:720` — `{#each windowed as { entry, i } (entry.path)}`
  - `src/lib/components/FolderBrowser.svelte:127` — `{#each sorted as entry (entry.path)}`
  - `src/lib/components/DropStackPanel.svelte:91` — `{#each $dropStackEntries as entry (entry.path)}`

This repo is on `"svelte": "^4"` (`package.json`), where a duplicate key in a keyed `{#each}` **throws**
rather than rendering the second row. So the expected symptom is not two confusing rows, and not merely
"the folder fails to render" — it is the **whole list failing to render**, every unrelated file in it
included, on whichever of the three surfaces hits the collision first.

A repo-wide sweep finds ~29 keyed `{#each}` blocks keyed on `.path`, of which at least two more list
*directory children* (`SidebarNode.svelte:55`, `Sidebar.svelte:808`); the UAT did not trace whether those
are fed by `remote_dir_entries`, so they are named as candidates rather than counted. The point either way
is that keying on `path` is the house convention, which is the argument for fixing the path at its source
(option 1 below) rather than re-keying one component.

## Not verified

Whether this reproduces in the running app. That needs a real bucket carrying the collision (no fixture
in the repo drives the frontend end to end), and no real S3/MinIO/Ceph endpoint has been exercised by any
ticket in this family — see CPE-1518. The two code facts above are read from the source, not observed at
runtime, and the S3 provider is not yet user-reachable (CPE-1685), so this cannot bite a user today.

## What a fix probably needs

The duplicate `path` is the root, not the keyed each — two distinct objects sharing an identity string is
wrong regardless of who consumes it. Options, cheapest first:

1. Make the remote child URI carry the `is_dir` bit (a trailing `/` for the prefix row, which is also how
   S3 spells it), so the two rows differ in `path` as they differ in reality.
2. Key the each on something already unique (index, or `path` + `is_dir`) — treats the symptom, leaves
   two rows claiming the same identity to `stat`/`delete`/drag-and-drop.

Option 1 also supplies the bit that **CPE-1735 item 2** is missing: with distinct paths, `delete(path)`
finally knows whether the user clicked the object or the folder. Worth doing together.

## Acceptance criteria

- [ ] Two entries in one listing can never share a `path`, with a test over the colliding keyspace.
- [ ] A folder holding the collision renders, and the two rows are separately selectable.
- [ ] The guard is broken on its own and shown to red a distinct test, per the Evidence Rules in
      `Ticketing/wiki.md`.
- [ ] If option 1 is taken, CPE-1735 item 2 is re-read — it may close with it.

## Notes

Related: **CPE-1727** (which measured the two rows), **CPE-1735** (the delete-side half of the same
missing bit), **CPE-1704** (leaf safety in remote listings), **CPE-1685** (which makes S3 user-reachable),
**CPE-1518** (the first real endpoint).
