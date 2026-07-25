---
id: CPE-738
title: "EPIC: Secure delete & encrypted vaults"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Two data-safety primitives: cryptographic shredding for sensitive deletes (with honest, platform-aware
caveats about SSD / copy-on-write limits) and per-folder encrypted vaults that lock/unlock with a passphrase
and mount transparently in the explorer.

## Why
Protecting data at rest and on disposal is a real need the app doesn't address. Done honestly (no false
guarantees) it's a differentiating trust feature.

## Rough scope (areas, not child tickets)
- A shred command with clear, honest UX about what it can and can't guarantee on modern storage.
- An authenticated-encryption vault format (per-folder), lock/unlock lifecycle.
- Transparent mount of an unlocked vault as a browsable location.
- Vault indicators in the tree; passphrase handling via the OS keychain.

## Open questions (resolve at activation)
- Shred honesty on SSD/CoW/wear-levelled media — messaging and scope.
- Vault crypto design and format; security review gate before shipping.
- Mount mechanism (in-app virtual FS vs. OS-level mount) per OS.

## Definition of Done
- Secure delete is available with honest, platform-aware guarantees clearly stated.
- Per-folder encrypted vaults lock/unlock with a passphrase and mount transparently for browsing.
- Crypto passes a security review; keys are stored in the OS keychain, never plaintext.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-941** — `secure_delete::plan_shred` / `passes`:
overwrite-pass schedules (Zero/Random/DoD3/Gutmann) + honest SSD / copy-on-write erasure caveats. Remaining:
the overwrite engine, and the encrypted-vault half (passphrase/key derivation + transparent mount).
