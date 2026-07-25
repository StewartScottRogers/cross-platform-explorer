---
id: CPE-900
title: SFTP provider — SSH-key authentication (public-key auth)
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
Adds SSH public-key authentication to the SFTP provider (CPE-899 shipped password auth). Completes
CPE-682's auth story.

- New `SftpAuth` enum: `Password(String)` | `PrivateKey { pem, passphrase: Option<String> }`.
  `SftpConfig::password(…)` / `SftpConfig::key(…)` constructors.
- `connect` branches on the auth method: `authenticate_password` vs `authenticate_publickey`
  (`PrivateKeyWithHashAlg`, negotiating the RSA hash where relevant; `None` for Ed25519).
- `decode_private_key` parses an OpenSSH private key and **decrypts it with the passphrase if encrypted**
  (clear error if the key is encrypted and no passphrase was supplied, or the passphrase is wrong).

## Acceptance Criteria
- [x] `SftpConfig::key(host, port, user, pem, passphrase)` authenticates with an OpenSSH private key.
- [x] Correct key → auth succeeds and the session works (list); wrong key → auth fails; malformed key →
      clear "invalid private key" error; encrypted key without passphrase → clear error.
- [x] The in-process test server accepts only a configured public key (proves the client sends the right
      key). 6/6 tests pass on the 3-OS matrix; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Extended the CPE-899 crate + fixture (server now verifies a specific client public key via
  `auth_publickey`). Ed25519 keypairs generated in-test. CPE-682's provider is now feature-complete for
  connect/auth/list/stat/read; what remains is app wiring (connections UI/keychain — CPE-683) and
  streaming/transfers (CPE-684).
