# Vault security review (CPE-738)

Security design + review record for the **encrypted vaults** feature: per-folder authenticated
encryption that locks/unlocks with a passphrase and mounts transparently for browsing.

> **Status — honest bottom line.** The vault uses well-established, audited cryptographic primitives (it
> does **not** hand-roll crypto) and has undergone a thorough **internal adversarial review** by an
> independent reviewer across every slice (see "Adversarial review record"). That materially de-risks it,
> but it is **not a substitute for a professional external cryptography audit**, which is **recommended
> before shipping vaults to real users at GA**. Until then, treat vaults as "carefully built, internally
> reviewed, not externally audited."

---

## 1. What a vault is

A vault is a folder that has been sealed into a single encrypted file, `<name>.cpevault`. Sealing encrypts
the whole tree; unlocking decrypts it (with the passphrase) into a session directory the explorer browses
as a normal location; locking wipes that session directory. The `.cpevault` blob is the only at-rest
artifact once sealed.

## 2. Threat model

**Protects against (at rest, vault locked):**
- Disclosure of the folder's *contents*, *file names*, *file sizes*, and *file count* to anyone who can
  read the `.cpevault` file but does not know the passphrase (a stolen/backed-up/synced file, another
  user on the machine, cloud-sync exposure). The single-blob format hides structure, not just bytes.
- Silent tampering: any modification of the ciphertext is detected on decrypt (authenticated encryption).

**Does NOT protect against (out of scope for v1 — stated plainly):**
- An attacker with **read access while the vault is UNLOCKED** — the decrypted plaintext lives in a
  session directory on disk during that window (see §5, the "mount tradeoff").
- A compromised OS / malware / a keylogger / memory scraping on the live machine (the passphrase and
  decrypted data necessarily exist in memory when in use).
- A weak user passphrase (scrypt raises the cost of guessing but cannot save a trivially guessable
  passphrase — see §4).
- Rubber-hose / coercion, cold-boot memory attacks, or side channels beyond what the underlying `age`
  primitives address.

## 3. Cryptographic design

- **Library:** the [`age`](https://crates.io/crates/age) crate, **v0.12.1**, in **passphrase mode**. This
  is a deliberate choice to avoid hand-rolling AEAD/KDF/nonce management. `age` provides:
  - **AEAD:** ChaCha20-Poly1305 (authenticated encryption — confidentiality + integrity).
  - **Passphrase KDF:** scrypt, with a work factor (see §4).
  - Chunked STREAM AEAD framing with correct per-chunk nonce handling (no nonce-reuse footgun). Note:
    `age` streams *internally*, but the v1 cpe wrapper **buffers the whole tree in memory** when sealing/
    opening (each file is read fully, packed into one plaintext stream, then encrypted; opening reads the
    whole blob + plaintext together) — so peak memory scales with total vault size (~2-3×).
    Streaming-from-disk is future work. See §5.
- **Blob format (`.cpevault`):** `MAGIC (b"CPEVLT1") || u16 schema_version (LE, =1) || age-ciphertext`.
  The magic + version are checked before any crypto; a bad magic / unsupported version yields a distinct,
  non-crypto error (never a panic).
- **Tree serialization:** the folder tree is serialized to one plaintext stream by a deterministic,
  dependency-light internal framing (`kind || path_len || path || data_len || data`, sorted by path) —
  chosen over `tar` for determinism (no mtime/uid/gid/ordering noise) and a trivially auditable,
  panic-free parser. Symlinks/devices/FIFOs are skipped (documented). Non-UTF-8 filenames are rejected
  at encrypt time (v1).
- **Path safety (seal⟺extract symmetry — a hard invariant):** every entry path is validated with the
  **same** `sanitize_rel_path` at **encrypt** time that extraction enforces — rejecting `..` traversal,
  absolute paths, empty/`.` components, backslash/UNC components, and Windows drive-letter components
  (`^[A-Za-z]:`). This guarantees **anything that can be sealed can be extracted** (no "seal-but-never-
  open" data-loss trap), and extraction sanitizes again (defense-in-depth) so a crafted blob cannot write
  outside the output directory (zip-slip defense).
- **Extraction atomicity:** decrypt fully authenticates in memory, then writes into a sibling temp dir and
  renames into place on success; any error removes the staging dir — a failed decrypt never leaves a
  half-populated output. A non-empty target is refused rather than clobbered.
- **DoS hardening:** the scrypt work factor is capped on decrypt (`max_work_factor`) so a crafted blob
  cannot force an unbounded KDF; the framing parser bounds-checks every length (no huge-allocation or
  slice-panic on hostile input).

## 4. Passphrase & key handling

- The passphrase is wrapped in `age`'s `SecretString` at the IPC boundary and flows only to the crypto
  core; it is **never written to a plaintext file or a log**, and never placed in an error message.
- **OS keychain (optional):** if the user opts in ("Remember passphrase in this device's keychain"), the
  passphrase is stored via the [`keyring`](https://crates.io/crates/keyring) v3 crate — Windows Credential
  Manager / macOS Keychain / Linux Secret Service — under service `cpe.vault`, account = SHA-256 of the
  blob path. This is the **only** place a passphrase persists. It is off by a Settings preference by
  default. (Consequence, documented in the user docs: moving/renaming a `.cpevault` orphans its stored
  passphrase under the old path's account.)
- **KDF cost:** scrypt at `age`'s calibrated work factor (~1s on typical hardware). This is a deliberate
  brute-force speed-bump; it does **not** rescue a weak passphrase. The UI warns that a forgotten
  passphrase makes the data **unrecoverable** (there is no backdoor / recovery key by design).

## 5. Known limits & honest caveats

- **Plaintext on disk while UNLOCKED (the "mount tradeoff").** Unlocking decrypts into an app-private
  session directory (`appCacheDir()/vault-sessions/<uuid>`) so the explorer can browse it as a real
  location. While unlocked, that plaintext is on disk. Locking securely wipes it. A future in-memory or
  OS-level (FUSE/dokan) mount could avoid on-disk plaintext but was out of scope for v1.
- **Orphaned sessions on abnormal exit.** If the app is killed while a vault is unlocked, the session dir
  can linger. **CPE-1252** (filed) adds a startup sweep that securely wipes any `vault-sessions/*` not in
  the live registry (the registry is empty at boot, so all are orphans).
- **The session dir is contained, not caller-chosen (CPE-1647, closed).** `vault_unlock`'s `session_dir` is
  untrusted IPC input, and locking *securely shreds* whatever it names — so an unvalidated one was a
  "shred any directory" primitive (`vault_unlock(blob, pass, "…/Documents")` then `vault_lock(blob)`).
  `unlock_to_session` now refuses any path that does not resolve **strictly inside**
  `appCacheDir()/vault-sessions` (`ensure_session_dir_contained`, the same guard shape `create_vault`'s
  `resolves_inside` uses). Both sides are canonicalized before a component-wise comparison, so `..`,
  symlinks/junctions and the `vault-sessions` / `vault-sessions-evil` prefix trap are all caught; the root
  itself is refused (wiping it would destroy every live session); and every resolution failure fails
  **closed**. A refused unlock records no mapping, so a later `lock` has nothing to act on.
- **Secure-delete of the original is best-effort.** The optional "securely delete the original after
  sealing" overwrites then removes, but on SSDs, copy-on-write, wear-levelled, or journalled filesystems
  the OS may retain remnants — the UI states this honestly. It only runs **after** the vault is verified
  decryptable (verify-before-shred), and never when the destination blob is inside the folder being
  shredded.
- **Session-dir wipe pass.** The transient session dir is wiped with a single Zero pass (it's short-lived
  extracted plaintext); the destructive original-shred defaults to a stronger scheme. Both are honest,
  documented tradeoffs.
- **Whole-vault in-memory buffering (v1).** Sealing reads every file fully into memory and packs the whole
  tree into one plaintext stream before encrypting; opening holds the whole blob and decrypted plaintext
  together. Peak RAM scales with total vault size (~2-3×), so a very large (multi-GB) vault can exhaust
  memory — a scalability limit (and minor availability consideration). Streaming-from-disk is future work.
- **Dependency weight.** `age` (with `default-features=false`) adds ~90 transitive crates. Accepted as the
  cost of not hand-rolling crypto for a security feature; vaults are an additive mode (zero cost unused).

## 6. Adversarial review record (internal)

An **independent** reviewer (not the implementer) adversarially reviewed each slice and drove fixes:

- **Crypto core (CPE-1247):** found + fixed a colon-in-filename seal/extract asymmetry (data-availability)
  and partial-extraction-without-rollback; re-probed 11 hostile paths (traversal/UNC/drive/verbatim) — all
  contained; confirmed authenticate-before-write, no plaintext leak, panic-free parsing, correct
  wrong-passphrase → `BadPassphrase` and tamper → `Corrupt` mapping.
- **Lifecycle (CPE-1248):** found + fixed (a) a dest-blob-inside-the-shredded-folder data-loss, (b) a
  verify step that wrote full plaintext to `%TEMP%` during the shred flow (now verified **in memory** via
  `verify_blob`, no disk write), (c) `lock` dropping its registry mapping before wiping (now wipe-first,
  retryable on failure), and (d) a regression where the in-memory verify skipped path sanitization —
  closed by enforcing seal⟺extract symmetry at encrypt time.
- **Mount/browse (CPE-1249):** found + fixed a re-unlock that orphaned decrypted plaintext (frontend
  already-unlocked guard + backend best-effort superseded-dir wipe) and a failed-lock that stranded the
  retry out of UI reach.
- **Session containment (CPE-1647):** found + fixed an uncontained `session_dir` on the IPC boundary — a
  caller with a vault and its passphrase could unlock into any directory and have `lock` shred it. Closed
  by canonicalizing and requiring strict containment under the app's own `vault-sessions` root (§5). Proven
  by tests that read the victim's bytes back **off disk** after both the refused unlock and the follow-up
  lock, cover the `..`/symlink/prefix-sibling/root-itself/unresolvable-root variants, and keep a negative
  control showing a legitimate fresh session still unlocks and is still wiped.
- Verify-before-shred is enforced with an injectable verifier and a falsifiable test proving the original
  survives a failed verify.

## 7. Recommendation

- **Before GA / real-user data:** commission a **professional external cryptography audit** of the format,
  the `age` usage, the key/keychain handling, and the mount/shred lifecycle. The internal review above is
  thorough but is performed by the same project and does not replace an independent expert audit.
- **Before then:** present vaults as internally-reviewed and appropriate for reducing casual/at-rest
  exposure, with the §5 limits stated to users (they are, in `src/docs/20-vaults.md`).
- Complete **CPE-1252** (orphan-session sweep) to close the crash-leaves-plaintext gap.
