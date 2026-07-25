---
id: CPE-1012
title: Secure-delete shred engine (execute overwrite passes + unlink)
type: feature
component: Backend
priority: medium
tags: ready
status: Doing
created: 2026-07-24
epic: CPE-738
estimate: 2-3h
---

## Summary
The CPE-738 (secure delete & encrypted vaults) epic's remaining **secure-delete core**.
`crates/server/src/secure_delete.rs` (CPE-941) is only the **pure pass *planner*** — `passes(scheme)`,
`plan_shred(...) -> ShredPlan` (patterns + honest platform caveats). Nothing actually **executes** the
passes. This ticket adds the disk-backed **shred engine** that overwrites a real file's bytes pass-by-pass
per a `ShredPlan`, then unlinks it. Fully headless — verified by tempfile tests.

## Scope
New module `crates/server/src/secure_shred.rs` (declare `pub mod secure_shred;` in `lib.rs`, near
`secure_delete`), building on the existing planner — do **not** duplicate `passes`/`plan_shred`:
- `shred_file(path, scheme) -> Result<ShredReport, String>` — stat the file for its size, build the plan via
  `secure_delete::plan_shred` (on_ssd/copy_on_write can be `false`/best-effort for v1 — the caveats are the
  planner's job; note the assumption), then for each `PassPattern` in order: open the file for writing, seek
  to start, and overwrite exactly `size` bytes with that pattern, **flushing + `sync_all`** after each pass so
  writes actually reach disk (not just the page cache). After the last pass, remove the file.
- Pattern bytes: `Zeros`→0x00, `Ones`→0xFF, `Byte{value}`→that byte, `Random`→unpredictable bytes.
- Return a `ShredReport { path, passes_run, bytes_written, removed }` (serde-serializable, `specta::Type`
  behind the `specta` feature like the neighbours) so a caller/UI can confirm what happened.

### RNG for the `Random` pass — no new dependency
`getrandom`/`rand` are **not** direct deps (only transitive in the lock) so you may **not** `use` them without
adding a Cargo.toml dependency — which is forbidden by the repo guardrail. For the `Random` pass, fill the
buffer from a **std-only PRNG** (e.g. splitmix64 / xorshift) seeded from `std::time::SystemTime` nanos +
a stack address + thread id. This is **non-cryptographic** — document that clearly in the module doc and leave
a `// NOTE: swap to a CSPRNG if a crypto RNG ever becomes a direct dependency`. It is acceptable for v1: the
security value is the in-place overwrite occurring, and `secure_delete`'s caveats already disclaim hard
guarantees. Write in reasonably-sized chunks (e.g. 64 KiB), not byte-by-byte.

Keep it **std only** (plus the existing `serde`/`specta`). No new dependency in any Cargo.toml.

## Acceptance Criteria
- [ ] `secure_shred` module compiles, declared in `lib.rs`, not feature-gated.
- [ ] `shred_file` on a temp file runs every pass of the scheme, writes `size × passes` bytes total, and the
      file no longer exists afterward; `ShredReport` reflects that.
- [ ] Multi-pass scheme test (e.g. `Dod3`) writes 3× the file size and completes; `Gutmann` runs 7 passes.
- [ ] Zero-length file: shreds (0 bytes written) and is removed without error.
- [ ] Missing/unreadable path returns a clean `Err`, not a panic.
- [ ] `cargo test -p cpe-server secure_shred` green; `cargo clippy --all-targets -D warnings` clean in **both**
      feature modes; no new dependency added.

## Notes
- Grep-first done (Foreman): no shred/overwrite engine exists — `secure_delete.rs` is planner-only; no
  `write_all`-pass loop or `shred` fn anywhere in `crates/` or `src-tauri/`. Safe to build fresh.
- Backend-only. The Tauri command + confirm-dialog UI (destructive → must confirm, per the epic DoD) is a
  later attended slice. This ticket is the engine + tests.
- Domain logic in `cpe-server`, not `lib.rs`. Match neighbouring module style (rich docs, `Result<_,String>`).
