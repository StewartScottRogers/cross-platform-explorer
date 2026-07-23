---
id: CPE-705
title: "EPIC: Archive & compression suite"
type: Task
status: In Progress
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

## Work Log
2026-07-22 (nightshift) — **Activated.** Grep-first: the Rust foundation is **largely already built** —
`archive.rs` has archive reading (CPE-064), `compress_to_zip`, `extract_archive` (here / to, with a
zip-slip guard), and `extract_archive_entry` (CPE-251/252/242). Open questions resolved (best-guess):
**crate choices** = the existing `zip` / `tar` + `flate2` / `sevenz-rust` (all pure-Rust, no sys libs; 7z
create deferred — read only for now); **edit model** = extract-modify-repack only (no in-place); **depth**
= browse + preview already work, this epic adds create/extract/navigate. First slice shipped:
**CPE-908** — tar.gz creation (`compress_to_targz`) + a format-dispatching `compress_archive`.

## Children
- CPE-908 — tar.gz creation + `compress_archive` dispatcher (backend) — **Done**.
- (next) Compress-selection + extract-here/extract-to context actions with conflict handling — **GUI + command wiring**.
- (next) Navigate-into-archive routing (an archive as a browsable location) — **GUI**.
- CPE-909 — Password-protected (AES-256) zip create + extract (backend) — **Done**; the password prompt UI remains.
