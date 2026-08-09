---
id: CPE-1515
title: "FileSystemProvider capability descriptor + auth-model growth (Anonymous/Token/AccessKey) — unblocks S3/cloud"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1501
created: 2026-08-09
---
## What (CPE-1501 F5 — enabling; headless)
Three additive extensions to the provider layer so protocols beyond SFTP/WebDAV/FTP fit:
1. **Capability descriptor** on `FileSystemProvider` (or a companion): `supports_write/rename/random_read/watch`,
   `has_real_dirs` — so the UI + router can adapt to read-only shares, S3 (no real directories), FTP (weak
   rename). Existing providers return sensible defaults (Local/SFTP/WebDAV/FTP: has_real_dirs=true, writable=true).
2. **Auth-model growth**: today `connections.rs` `AuthMethod = Password | Key`; add `Anonymous`,
   `Token{token}` (OAuth/bearer, for later cloud), `AccessKey{id, secret_ref}` (S3 SigV4). Keep it
   backward-compatible (serde — existing connections.json still parses). Note CPE-1514 currently does anonymous
   as a cpe-vfs heuristic; this is where Anonymous becomes first-class (migrate the heuristic to the enum if clean).
3. **Streaming read** already exists per-provider (sftp/webdav/ftp stream); ensure the trait/capability documents
   it (a `random_read` capability flag) — no behavior change required here.

## How
- Add the capability struct + a `capabilities(&self) -> ProviderCapabilities` trait method (default impl =
  full-POSIX so existing providers need no change; override where they differ). Well-tested via `FakeProvider`.
- Extend `AuthMethod` in `crates/server/src/connections.rs` with the new variants; update the `vfs::open` auth
  mapping to handle them (AccessKey → pass id+secret to a future S3 provider; Token → future cloud; Anonymous →
  first-class). Regenerate `bindings.gen.ts` if `AuthMethod`/`Connection` (specta) change (they will — additive).
- NO new Cargo dep. Keep the diff additive + backward-compatible (a v1 connections.json with Password/Key still loads).

## Verify (HEADLESS)
`cargo test` (crates/server + crates/vfs): capability defaults for Local/Fake; a provider reporting
has_real_dirs=false / read-only behaves; AuthMethod round-trips through serde incl. the new variants AND an old
Password/Key connections.json still deserializes (back-compat); vfs::open maps each auth variant correctly (via
FakeProvider). `cargo clippy --all-targets -D warnings` both feature modes. Regenerate + commit bindings +
`src-tauri/Cargo.lock` if touched.

## Ship
Move CPE-1515 Doing→Done, Work Log (the capability struct, the AuthMethod variants + back-compat approach, the
vfs mapping). Note S3 (CPE-1503) now buildable on this. Effort M, mostly pure/backend.

## Work Log (2026-08-09)

**Capability descriptor** (`crates/server/src/provider.rs`): a new `ProviderCapabilities` struct —
`supports_write`, `supports_rename`, `random_read`, `supports_watch`, `has_real_dirs` (all `bool`) — with
`impl Default` returning full-POSIX (every field `true`). `FileSystemProvider::capabilities(&self)` gained
a default trait-method impl that returns `ProviderCapabilities::default()`, so `LocalProvider` and every
existing consumer need zero changes. `FakeProvider` grew a test-only `with_capabilities(...)` builder +
`caps: Option<ProviderCapabilities>` field, overriding the trait method only when set — used to prove a
provider CAN report `has_real_dirs = false` / `supports_rename = false` (modelling the future S3 shape)
while everything else still defaults true. Doc comments flag S3 (CPE-1503) as the first expected override.

**AuthMethod growth** (`crates/server/src/connections.rs`): three new variants alongside the existing
`Password` / `Key { key_path }`:
- `Anonymous` — no credentials.
- `Token { token_ref: String }` — `token_ref` is a non-secret reference/label, NOT the token itself; the
  real bearer/OAuth token lives in the OS keychain (CPE-1510), fetched at connect time the same way a
  password is (keyed by the connection's `name`).
- `AccessKey { id: String, secret_ref: String }` — `id` is the S3 access-key ID (non-secret, stored like
  `user`); `secret_ref` is a non-secret reference/label, the actual secret access key stays in the
  keychain. No plaintext secret material was added to the enum or to `connections.json` anywhere.

**Back-compat proof:** `AuthMethod` is internally-tagged (`#[serde(tag = "kind", rename_all =
"snake_case")]`), so adding variants doesn't change how old tags parse — additive by construction. Added
`auth_method_round_trips_every_variant_through_serde` (serde round-trip for all 5 variants) and, as the
explicit back-compat gate, `an_old_connections_json_with_only_password_and_key_still_deserializes`: writes
a frozen v1-shaped JSON file (`{"kind":"password"}` / `{"kind":"key","key_path":...}` only) to disk and
loads it through the real `load_connections(path)` path, asserting both connections parse with their
original auth intact.

**vfs auth mapping** (`crates/vfs/src/lib.rs`): `sftp_auth_from`/`ftp_auth_from` extended, plus a new
`webdav_auth_from` (webdav previously inlined its auth logic in `open()`; factored out for the same
per-variant match + testability as sftp/ftp):
- `Anonymous` → SFTP: password login with an empty password (no true anonymous mechanism in SFTP/SSH);
  WebDAV: no `Authorization` header sent; FTP: `FtpAuth::Anonymous` directly.
- `Token` / `AccessKey` on sftp/webdav/ftp → a clear `Err` ("... auth is not supported by this provider —
  reserved for a future cloud provider"), not a silent wrong-auth attempt — no S3/cloud provider exists
  yet to consume them.
- `Password` / `Key` unchanged in behaviour.

**CPE-1514 anonymous heuristic:** migrated to be first-class but the heuristic itself was KEPT as a
fallback, not removed — `ftp_auth_from` now checks `AuthMethod::Anonymous` first (wins even with a
non-blank username), then falls back to the pre-existing blank/`"anonymous"`-username heuristic for
`Password`/`Key`-auth connections saved before this ticket, so an old saved FTP profile keeps working
unchanged. New connections should set `Anonymous` explicitly going forward.

**S3 (CPE-1503):** now buildable on top of this — `AccessKey`/`Token` variants + `has_real_dirs`/
`supports_rename` capability overrides are plumbed and tested; only the S3 provider crate + its `open()`
scheme arm are missing.

**Tests / lint:** `cpe-server` lib: 1812/1812 (+4 new: 2 capability tests, 2 AuthMethod serde tests).
`cpe-vfs` lib: 20/20 (+6 new: anonymous-variant mapping for ftp/sftp/webdav, cloud-only-auth rejection for
all three). `cargo clippy --all-targets -D warnings` clean on `cpe-server` (default, `--features specta`,
`--features index`) and `cpe-vfs` (default). `src-tauri cargo build` (default features) and `cargo run
--bin export_bindings --features "specta-bindings sidecar-platform"` both succeed.

**Bindings / Cargo.lock:** `src/lib/bindings.gen.ts` regenerated (additive `AuthMethod` union: `anonymous`,
`token`+`token_ref`, `access_key`+`id`+`secret_ref` added alongside `password`/`key`). No new Cargo
dependency, so no `Cargo.lock` (crate-level or `src-tauri/Cargo.lock`) changed.

**For the Reviewer to scrutinize:** (1) serde back-compat — the internally-tagged-enum argument is sound,
but double-check the explicit old-JSON-on-disk test actually exercises it, not just the in-memory
round-trip; (2) no plaintext secret in the enum — `token_ref`/`secret_ref` are references/labels only,
never fetched-from-keychain values; (3) `connected_provider`'s existing "no stored secret" guard in
`crates/vfs/src/connect.rs` still only special-cases `AuthMethod::Password` — `Token`/`AccessKey` aren't
given the same "fail before attempting a blank connect" treatment yet (deferred: no provider consumes them
today, so it's currently unreachable, but worth a follow-up once S3/CPE-1503 lands); (4) SFTP's
`Anonymous → empty password` mapping is a best-effort convention, not a verified behaviour against a real
anonymous SFTP server (they're rare) — flagging in case a Reviewer knows of a better convention.
