---
id: CPE-242
title: Browse into .zip archives like a folder (expand/navigate the contents)
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Opening a `.zip` should let you navigate INTO it as if it were part of the file
system — list its entries, descend into folders within the archive, and open
files from it — rather than launching an external app. The app already lists zip
contents for the preview pane (`read_archive_entries`); this extends that into a
navigable virtual filesystem for zip (and ideally the other supported archive
kinds: tar/7z).

## Acceptance Criteria
- [ ] Double-clicking / opening a `.zip` enters a virtual view of its contents.
- [ ] Folders inside the archive can be navigated (breadcrumb reflects zip path).
- [ ] Files inside can be opened (extract-on-open to temp, then open_external).
- [ ] Going "up" out of the archive returns to the real folder.
- [ ] The plain explorer stays fast when no archive is open (PURPOSE.md).
- [ ] `npm run check` + `cargo build` + tests pass.

## Notes
Substantial: introduces a virtual-path scheme (e.g. `archive://<zip>!/<inner>`),
backend commands to list an inner directory and extract a single entry, and
history/breadcrumb handling for in-archive paths. Build on the existing zip
reader (CPE-109/112/217) and preview registry. Likely staged. Read-only first
(no writing back into the archive).

### filled
Double-clicking a zip-based archive (.zip/.jar/.apk/.war/.ear/.ipa/.xpi/.whl/
.nupkg/.vsix) now enters a read-only virtual view instead of launching an app.
App holds an ArchiveView { zipPath, entries, inner }; archiveChildren() derives
each level's folders/files from the flat entry list (folders inferred from deeper
paths). Descend by opening a folder; breadcrumb shows Home › real path › zip ›
inner segments (all clickable, ARCH sentinel), Up/Backspace steps out and exits
at the root; sidebar/real navigation exits the archive. Opening a file extracts
it to %TEMP%\cpe-archive via new backend extract_archive_entry then open_external.
File-mutating actions (new/paste/delete/rename/cut/copy/duplicate) are guarded
read-only inside the archive. check 0/0; cargo build ok; 237 tests. Ships 0.10.4.
