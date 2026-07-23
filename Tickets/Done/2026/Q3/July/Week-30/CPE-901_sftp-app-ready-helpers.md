---
id: CPE-901
title: SFTP app-ready helpers — known_hosts file loading + location→provider bridge
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The glue that makes the SFTP provider (CPE-899/900) usable from a user-typed URL + the user's existing
SSH config, all headless-testable.

- **`cpe-server::known_hosts`:** `default_known_hosts_path()` (→ `~/.ssh/known_hosts` from `$HOME` /
  `%USERPROFILE%`, no new deps) + `load_known_hosts(path)` (read + parse; missing/unreadable file → empty
  list = first-use TOFU, never an error).
- **`cpe-sftp`:** `connect_location(&Location, auth, known, policy)` — bridges a parsed
  `cpe_server::location::Location` (`sftp://user@host[:port]/path`, CPE-680 parser) to a live
  `SftpProvider`. Port defaults to 22; a username is required (SFTP has no anonymous mode); a non-SFTP or
  userless location is refused before any connection.

Net effect: the app can now do `parse(url) → load_known_hosts(default_path) → connect_location(...)` end
to end.

## Acceptance Criteria
- [x] `default_known_hosts_path` resolves `~/.ssh/known_hosts`; `load_known_hosts` parses a file and
      returns empty for a missing one.
- [x] `connect_location` connects an `sftp://` `Location` to an `SftpProvider` (tested against the
      in-process server) and rejects non-SFTP / userless locations with clear errors.
- [x] `cargo test` (cpe-server known_hosts 11, cpe-sftp 8) + clippy `-D warnings` clean on the 3-OS matrix.

## Work Log
- 2026-07-22 — Small integration slice tying `location` + `known_hosts` + `cpe-sftp` together. The provider
  is now app-ready; what remains is the actual command wiring + the connections UI/keychain (CPE-683) and
  streaming transfers (CPE-684), which are GUI/attended.
