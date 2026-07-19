---
id: CPE-705
title: "EPIC: Archive & compression suite"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Create, extract, and browse *into* zip/7z/tar/gz archives as if they were folders — add-to-archive,
extract-here, extract-to, and password support — so users never leave the explorer to pack or unpack.

## Why
The preview pane already lists archive contents (CPE-064). This closes the loop: navigating into an archive,
compressing a selection, and extracting are core file-manager expectations the app doesn't yet meet.

## Rough scope (areas, not child tickets)
- Rust archive backends (read + write) for zip/tar/gz, then 7z; streamed extraction wired to the transfer manager.
- Navigate-into-archive routing in the listing view (an archive is a browsable location).
- Compress-selection and extract-here/extract-to context actions with conflict handling.
- Password-protected archive support (prompt on read, optional on create).

## Open questions (resolve at activation)
- Crate choices per format and cross-platform build cost (7z especially).
- In-place edit inside an archive, or extract-modify-repack only?
- How deep does "archive as a location" reach — search, preview, DnD in/out of archives?

## Definition of Done
- A user can open an archive, browse its tree, and preview files inside it.
- Compressing a selection and extracting (here / to) both work through the transfer queue with progress.
- Password-protected archives can be read and created; no regression to the existing archive preview.
