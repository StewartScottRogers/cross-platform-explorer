---
id: CPE-682
title: SFTP filesystem provider
type: feature
component: Backend
priority: medium
status: Done
closed: 2026-07-22
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
- 2026-07-22 — **Provider built + auth complete (CPE-899 + CPE-900).** `crates/sftp` (`cpe-sftp`): SFTP
  `FileSystemProvider` over russh (ring backend), host-key verification via `known_hosts` at connect,
  **password + SSH-key auth**, list/stat/read/write/mkdir/delete — all proven against an in-process
  russh-sftp test server (no Docker; 3-OS CI green). AC #1/#2/#3 met. Remaining is app-facing: the
  connections UI + keychain (CPE-683) and streaming/async transfers (CPE-684).
- 2026-07-22 (nightshift) — **Host-key verification core landed headlessly (CPE-897).** New
  `cpe-server::known_hosts` parses a `known_hosts` file and decides `Trusted`/`Unknown`/`Changed` for a
  presented host key (TOFU + loud changed-key refusal — AC #2's security heart), decoupled from any ssh
  crate. Remaining here (network + attended): the actual SFTP `FileSystemProvider` (connect with
  password/SSH-key, list/stat/read) that calls `verify_host_key` at connect time against a real server.

## Closure (2026-07-22)
All ACs met headlessly: connect (password + SSH key), list/stat/read (+write/mkdir/delete), and host-key
verification with clear errors — built as `crates/sftp` (`cpe-sftp`) over russh, verified against an
in-process russh-sftp server on the 3-OS matrix (CPE-899/900/901/902/684). App-facing wiring (route the
app's commands through the provider) + the connections UI/keychain live on CPE-683 and epic CPE-616.
