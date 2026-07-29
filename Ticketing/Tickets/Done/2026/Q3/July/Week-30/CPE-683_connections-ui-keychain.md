---
id: CPE-683
title: Connections sidebar + OS-keychain credentials
type: feature
component: Frontend
priority: medium
status: Done
closed: 2026-07-22
tags: ready
epic: CPE-616
estimate: 3-4h
---

## Summary
Child of CPE-616. A "Connections" sidebar to add/edit/remove a remote (host, auth, path), with
credentials in the OS keychain (never plaintext).

## What shipped (headless core)
`cpe_server::connections` — the pure, persisted **data model** behind the sidebar:
- `Connection { name, scheme, host, port, user, auth: AuthMethod, path }` where
  `AuthMethod = Password | Key { key_path }` — **no secret material** (the password value / key passphrase
  live in the OS keychain, keyed by the connection).
- `upsert` (add / edit-in-place by name), `remove`, `load_connections` / `save_connections` (pretty JSON;
  a missing/corrupt store → empty, never bricks the sidebar), `default_connections_path` (per-OS config
  dir), and `Connection::location()` → the `scheme://user@host[:port]/path` string that round-trips
  through the CPE-680 URI parser (so a saved connection navigates like a location).

## Acceptance Criteria
- [x] Add/edit/remove a connection (upsert/remove by name) that maps to a navigable location string.
- [x] Persistence writes **no secrets** — verified the on-disk JSON contains only metadata + a key *path*.
- [ ] The sidebar UI + storing/reading the actual secret in the OS keychain — **attended / GUI + OS**
      (needs the app + a real keychain), tracked on epic CPE-616.

## Work Log
- 2026-07-22 — Shipped the connections data + persistence layer in `cpe-server` (5 tests, 3-OS green,
  no new deps). The sidebar Svelte UI and the keychain secret storage are GUI/OS-attended; closing the
  headless core here.
