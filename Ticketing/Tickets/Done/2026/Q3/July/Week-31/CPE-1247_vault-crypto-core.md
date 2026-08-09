---
id: CPE-1247
title: "Vault crypto core: age-passphrase encrypt/decrypt of a folder ↔ .cpevault blob"
type: Task
priority: Medium
component: cpe-server
tags: [ready, security-sensitive]
estimate: 3h
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
First slice of the encrypted-vaults half of CPE-738 (user authorized the crypto dependency: "do the
vaults"). The security heart of the feature — it gets the heaviest, adversarial review, so it is a **pure,
Tauri-free, fully cargo-testable** core in `crates/server` with no I/O side-effects beyond reading/writing
the two byte streams it is handed. Lifecycle/keychain/mount/UI are later slices (CPE-1248..1251).

**Crypto decision (already made — do NOT hand-roll crypto):** use the **`age`** crate in **passphrase
mode** (ChaCha20-Poly1305 AEAD + scrypt KDF, pure-Rust, streaming, audited). This removes nonce/KDF
footguns. Confirm the exact current `age` API via context7 before coding (the passphrase encrypt/decrypt
entry points moved across 0.9→0.11); pin a specific version.

## What to build
`crates/server/src/vault_crypto.rs` — a pure module:

1. **Serialize a folder tree → one plaintext byte stream.** A deterministic `tar` of the tree (relative
   paths, file bytes; store regular files + dir structure; skip/param symlinks — document the choice).
   Use a small, already-vendored-or-lightweight tar writer if one is present; otherwise a minimal internal
   framing is acceptable (length-prefixed `path\0mode\0len\0bytes` records) — whichever is simpler and
   fully testable. Keep it dependency-light.
2. **Encrypt** that stream with `age` passphrase mode → a `.cpevault` blob = `MAGIC (b"CPEVLT1") ||
   u16 schema_version || age-ciphertext`. `fn encrypt_tree(root: &Path, passphrase: &Secret<String>) ->
   Result<Vec<u8>, VaultError>` (or take an in-memory tree for pure testability + a thin fs-walking
   wrapper — prefer a pure `encrypt_bytes`/`decrypt_bytes` core + a fs helper so the crypto is testable
   without touching disk).
3. **Decrypt + verify**: parse+check magic/version, `age`-decrypt with the passphrase, un-tar back to a
   tree (returned as an in-memory structure and/or written under a caller-provided dir). Wrong passphrase
   → a distinct `VaultError::BadPassphrase`; any AEAD/tamper failure → `VaultError::Corrupt` (age's
   authenticated decryption already rejects tampering — surface it, don't swallow it).
4. Use **`zeroize`** (transitive via age, or add it) to wipe the passphrase/derived material where
   practical; document what can and cannot be guaranteed (heap copies inside `age`).

## Acceptance criteria (cargo tests — the falsifiable core)
- **Round-trip**: encrypt a nested tree (dirs + several files, incl. an empty file + a binary file) →
  decrypt with the right passphrase → byte-identical tree back.
- **Wrong passphrase** → `Err(BadPassphrase)`, never partial/plaintext output.
- **Tamper-detection**: flip one byte anywhere in the ciphertext body → decrypt returns `Err(Corrupt)`
  (proves the AEAD is actually authenticating).
- **Bad magic / wrong version** → a clear distinct error (not a panic, not a crypto error).
- **Empty tree** and a **reasonably large** file (e.g. a few MB) both round-trip (streaming, no OOM).
- No `unwrap`/`expect` on attacker-controlled input (a crafted blob must never panic — add a
  "garbage bytes in → clean Err, no panic" test).

## Guardrails / conventions
- Pure `cpe-server` module; NO `#[tauri::command]` here (commands come in CPE-1248). Behind the crate's
  normal seams; no global state.
- Add `age` (pin version) to `crates/server/Cargo.toml` — this is the approved new dep; keep it to `age`
  itself (avoid pulling `age/armor`/CLI features you don't need — use `default-features` review).
- `cargo test -p cpe-server`, `cargo clippy --all-targets -D warnings` in BOTH feature modes, and
  `cargo build` must all pass. Follow the delete-test rule (no hollow tests). No `println!`.
- Do NOT expose any specta `Type` struct yet (no bindings drift this slice). If you must, regenerate
  `bindings.gen.ts` per [[regen-specta-bindings-on-struct-change]].

## Out of scope (later slices)
Keychain storage, Tauri commands, create/lock/unlock lifecycle, mount/browse, UI, the security-review doc.

## Done 2026-08-01 (sprint) — merged #550 @ d6f09ed3
Pure `age`-passphrase (=0.12.1, ChaCha20-Poly1305 + scrypt) encrypt/decrypt of a folder tree ↔ a
`.cpevault` blob (MAGIC+version+age-ciphertext), deterministic length-prefixed framing (not tar),
symlink-skip, precise path sanitizer (drive-letter blocked, `:` allowed in normal components),
atomic all-or-nothing extraction, scrypt work-factor cap. Full gauntlet: independent SECURITY review
(APPROVE after a fix round — it caught a seal-but-can't-open data-availability bug #1 + partial-extraction
#2 + a doc inaccuracy #3, all fixed + re-reviewed with 11 hostile-path Windows probes) + UAT PASS +
Foreman-local verify (19 vault tests, clippy both modes). CI runners were stalled at merge; verified
locally 4x — the one unix-gated test (colon-filename roundtrip) will confirm when CI processes.
