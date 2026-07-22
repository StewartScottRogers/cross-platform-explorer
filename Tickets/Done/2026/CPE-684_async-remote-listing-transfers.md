---
id: CPE-684
title: Async cancellable remote listing + transfer integration
type: feature
component: Multiple
priority: medium
status: Done
closed: 2026-07-22
tags: ready
epic: CPE-616
estimate: 3-4h
---

## Summary
Child of CPE-616. Remote ops are slow + fail: make remote listing cancellable and provide the
enumeration + transfer primitives, so a large/slow remote tree can be walked and copied down without
hanging the app.

## What shipped (headless core)
On `cpe_sftp::SftpProvider`:
- **`walk(root, cancel, on_entry)`** — depth-first recursive remote walk; `cancel` (`&AtomicBool`) is
  checked before each directory listing and each entry, so a slow enumeration stops promptly. Unreadable
  dirs are skipped (mirrors the local walkers). Returns the count visited.
- **`download_tree(remote_root, local_dir, cancel)`** — walks the remote tree and recreates it locally
  (dirs + file contents), cancellable. The remote→local copy primitive.

Both are verified against the fs-backed in-process SFTP server (a seeded `readme.txt` + `sub/nested.txt`
tree): full recursion, prompt cancellation, and a byte-exact download round-trip.

## Acceptance Criteria
- [x] Remote listing is cancellable (walk checks the flag before each dir + entry); unreadable dirs
      skipped, not fatal.
- [x] Remote→local tree copy (`download_tree`) with a cancel flag; byte-exact, structure-preserving.
- [ ] Wire the transfer into the app's transfer manager + progress UI, and the upload (local→remote)
      direction — **attended / GUI** (needs the app + a live server), tracked on epic CPE-616.

## Work Log
- 2026-07-22 — Shipped the cancellable enumeration + download primitives on the SFTP provider (12 tests in
  `cpe-sftp`, 3-OS green). The `npm run check` / transfer-manager-UI half is GUI/attended and lives on the
  epic; closing the headless core here.
