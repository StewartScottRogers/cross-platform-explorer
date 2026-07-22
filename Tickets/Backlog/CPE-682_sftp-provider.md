---
id: CPE-682
title: SFTP filesystem provider
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-616
estimate: 4h+
---

## Summary
Child of CPE-616. An SFTP `FileSystemProvider` (connect/list/stat/read via an ssh/sftp crate) with
host-key verification. Needs network + attended testing against a real server. Prereq: CPE-681.

## Acceptance Criteria
- [ ] Connect (password + SSH key), list, stat, read a remote path over SFTP.
- [ ] Host key verified before any op; failures surface clear errors.
- [ ] cargo-tested where possible (parsing/error paths); clippy clean both modes.

## Work Log
- 2026-07-22 (nightshift) — **Host-key verification core landed headlessly (CPE-897).** New
  `cpe-server::known_hosts` parses a `known_hosts` file and decides `Trusted`/`Unknown`/`Changed` for a
  presented host key (TOFU + loud changed-key refusal — AC #2's security heart), decoupled from any ssh
  crate. Remaining here (network + attended): the actual SFTP `FileSystemProvider` (connect with
  password/SSH-key, list/stat/read) that calls `verify_host_key` at connect time against a real server.
