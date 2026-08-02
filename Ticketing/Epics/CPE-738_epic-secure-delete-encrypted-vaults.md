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

## Re-activated 2026-08-01 (workshift) — VAULTS half, user said "do the vaults"
User authorized the crypto-dependency call. Grep-first reuse found: `keyring` v3 is ALREADY a workspace
dep (`sidecar/host`, per-OS native backends) with a proven `KeyringBackend` seam
(`sidecar/host/src/providers/secrets.rs`) + a `SecretAccess`-style trait in `sidecar/ai-console/src/vault.rs`
(a secret-REFERENCE vault — not file crypto, but the seam pattern to mirror). NO file-encryption crate
exists yet.

**Crypto decision (Foreman, per the mandate to use audited primitives, never hand-roll):** the vault
format uses the **`age`** crate (passphrase mode: ChaCha20-Poly1305 AEAD + scrypt KDF, pure-Rust,
streaming, audited) — no nonce/KDF footguns. This is the NEW dep the user approved. Keychain via the
already-approved `keyring` v3. **Format:** a folder → a single `.cpevault` blob (tar of the tree, age-
encrypted) — atomic lock/unlock, and it hides file names/count/sizes (more private than per-file `.age`).

Decomposition (SEQUENTIAL — each needs the prior; core-first, heaviest review on the crypto core):
- **CPE-1247** — Vault crypto core (`cpe-server/src/vault_crypto.rs`): pure `age`-passphrase encrypt/
  decrypt of a folder tree ↔ a `.cpevault` blob (magic + schema version); cargo tests: round-trip,
  tamper-detection (flip a byte → fail), wrong-passphrase-rejected, empty/nested/large. NEW dep `age`.
- **CPE-1248** — Vault lifecycle + keychain seam + thin async commands (create shreds plaintext via the
  built secure-shred engine; lock/unlock/status/is-vault; `SecretAccess` mirror of the keyring backend).
- **CPE-1249** — Transparent mount/browse of an unlocked vault as a location + tree lock/unlock indicator.
- **CPE-1250** — Vault UI (create/unlock/lock dialogs w/ visible border + path picker; indicators;
  Settings for keychain caching); gui-smoke + Visual Critic.
- **CPE-1251** — Security-review doc (threat model + crypto choices + honest guarantees) + crew adversarial
  security review + an explicit "professional external audit recommended before GA" flag (DoD's review gate,
  done honestly — a crew review de-risks but does not substitute for a professional crypto audit).

### Dependency-weight ACK (Foreman, from CPE-1247 security review, finding #4)
`age =0.12.1` with `default-features=false` adds exactly two DIRECT deps (`age` + `zeroize`) but ~90
TRANSITIVE crates (curve25519-dalek/x25519-dalek, p256, ml-kem, hpke, i18n-embed/fluent for age's
localized errors). This is a real, conscious cost against PURPOSE's "small" tiebreaker. Accepted rationale:
the user explicitly authorized the crypto dependency; `age` is the audited, footgun-free choice and NOT
hand-rolling AEAD/KDF/nonces is worth the weight for a security feature; and vaults are an ADDITIVE mode
(zero cost when unused). Possible future trim (follow-up, not now): investigate whether age's i18n/fluent
pull can be dropped. Recorded so the weight is a deliberate decision, not an accident.

## Secure-delete slice COMPLETE 2026-08-01 (workshift) — vaults remain user-gated
CPE-1240 (#539) wired the shred engine end-to-end: `shred_paths` command + "Securely delete…" context
action + ShredConfirmDialog (honest permanence + best-effort platform caveat + scheme picker + red
"Shred permanently"). CPE-1241 (#540) pinned it with a gui-smoke spec (Cancel-only, fixture-survives)
+ Visual Critic **VISUAL PASS**. Full gauntlet: Reviewer + UAT + Visual. The secure-delete DoD bullet
is MET. Reverted to Proposed — the ONLY remaining DoD (encrypted vaults) is USER-GATED: needs a crypto
dependency exception (repo enforces no-new-dep) + a human security review + OS-keychain key storage.
Re-activate once the user makes the crypto-dependency call.
