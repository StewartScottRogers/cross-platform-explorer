---
id: CPE-1252
title: "Vault: sweep orphaned vault-sessions/* dirs (startup/periodic cleanup)"
type: Task
priority: Low
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
Follow-up from the CPE-1249 review (finding #5). Unlocking a vault decrypts its plaintext into an
app-private session dir under `appCacheDir()/vault-sessions/<uuid>` (the v1 "mount" tradeoff). `vaultLock`
wipes it, but a session dir can be orphaned if the app is killed while a vault is unlocked (and, before
CPE-1249's #1/#2 fixes, via some UI paths). Orphaned dirs leave decrypted plaintext on disk indefinitely.

## What to build
A cleanup pass that removes stale `vault-sessions/*` dirs — e.g. on app startup (no vault can legitimately
be "unlocked" across a restart in v1, since the in-memory VaultRegistry is empty at boot, so every
`vault-sessions/*` present at startup is by definition orphaned) — securely (reuse the shred wiper, not a
plain delete, since the contents are plaintext). Optionally a periodic sweep of dirs older than the
session's known-unlocked set.

## Acceptance criteria
- On startup, any `vault-sessions/*` dir not in the live registry is securely wiped + removed.
- Wipe uses the secure-shred path (plaintext must not be left recoverable by a plain unlink).
- Headless-testable (seed a fake orphan dir → sweep → gone).
- No impact when there are no orphans; no interference with a vault unlocked in the current session.

## Notes
Coordinate with the honest security-review doc (CPE-1251) — this closes the "plaintext lingers after a
crash" gap it will document.
