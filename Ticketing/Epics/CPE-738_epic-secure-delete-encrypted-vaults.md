---
id: CPE-738
title: "EPIC: Secure delete & encrypted vaults"
type: Task
status: Proposed
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

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Overwrite engine + encrypted-vault (key derivation/mount) + security review unbuilt (only shred-plan model).

## Re-activated 2026-08-01 (workshift) — secure-delete slice only; vaults stay user-gated
PM scouting (grep-first). TRUE state:
- The overwrite ENGINE `secure_shred::shred_file(path, scheme) -> ShredReport` (schemes
  Zero/Random/DoD3/Gutmann, CPE-1012) + the honest-caveat plan `secure_delete::plan_shred` (CPE-941)
  are BUILT + cargo-tested but ORPHANED — `grep shred src-tauri/src/lib.rs` finds only a comment; NO
  command, NO UI. (Orphaned-but-built pattern.)
- The VAULT half is entirely unbuilt + USER-GATED: needs an authenticated-encryption + KDF crate
  (the repo enforces a hard no-new-dep guardrail — a dependency exception is a user call) AND the DoD
  mandates a human security-review gate + OS-keychain key storage. Not crew-buildable alone.

Building the secure-delete DoD bullet only:
- **CPE-1240** — Wire secure delete: thin `shred_paths` command (dispatch to `shred_file`) + a
  "Securely delete…" context-menu action + an explicit confirm dialog stating it is PERMANENT /
  non-recoverable (NOT routed through the recoverable trash — that's the point) AND the honest
  platform caveats (SSD wear-leveling / copy-on-write / journaling can leave remnants; overwrite is
  best-effort). Backend command + frontend. Headless-verifiable (cargo + vitest).

**Deferred (user-gated):** encrypted vaults — needs a crypto-dep exception + a security review + OS
keychain. Revisit with the user.

## Secure-delete slice COMPLETE 2026-08-01 (workshift) — vaults remain user-gated
CPE-1240 (#539) wired the shred engine end-to-end: `shred_paths` command + "Securely delete…" context
action + ShredConfirmDialog (honest permanence + best-effort platform caveat + scheme picker + red
"Shred permanently"). CPE-1241 (#540) pinned it with a gui-smoke spec (Cancel-only, fixture-survives)
+ Visual Critic **VISUAL PASS**. Full gauntlet: Reviewer + UAT + Visual. The secure-delete DoD bullet
is MET. Reverted to Proposed — the ONLY remaining DoD (encrypted vaults) is USER-GATED: needs a crypto
dependency exception (repo enforces no-new-dep) + a human security review + OS-keychain key storage.
Re-activate once the user makes the crypto-dependency call.
