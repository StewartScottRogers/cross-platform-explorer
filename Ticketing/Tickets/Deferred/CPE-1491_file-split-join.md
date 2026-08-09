---
id: CPE-1491
title: "File split / join — chunk a large file into parts and rejoin (classic commander utility)"
type: Feature
status: Deferred
priority: Low
component: Multiple
tags: [ready]
created: 2026-08-08
---
## What
A small, classic orthodox-commander utility (Total Commander / Multi Commander staple, absent from CPE):
**split** a large file into N fixed-size parts (`.001`, `.002`, … + a small checksum/index), and **join** them
back into the original. Still genuinely useful for FAT32/USB size limits, chunked uploads, and email-attachment
splits. Surfaced by the competitive-landscape GUI survey.

## Honest framing (why it's Low)
Least differentiating item from the survey — a CLI/script can do the same job and the GUI value-add is modest.
Filed as a cheap, well-scoped Low ticket, not an epic. Build if the queue wants an easy backend win; don't
prioritize it over the differentiators (CPE-1487/1488/1489 or activating CPE-661/616).

## How
- Backend (`cpe-server`): a **stream-chunking** module — split reads the source with a **bounded/streamed**
  reader (never load the whole file; follow STREAMING.md + the resource-exhaustion conventions) writing
  fixed-size parts + a tiny manifest (part count, sizes, sha256 of the whole for verify); join concatenates
  parts in order and verifies the checksum. No new Cargo deps (reuse the existing sha256 from CPE-412/737).
- Frontend: one dialog (choose part size / pick parts to join) + context-menu entries (MENUS standard); the
  actual work runs through the transfer/progress surface where it fits.

## Verify (headless half is clean)
`cargo test`: round-trip (split then join == original bytes, checksum matches); odd final part size; a part
missing/corrupt → join errs gracefully; bounded on a large synthetic input (no full-file buffer). `cargo
clippy --all-targets -D warnings`.

## Effort
Small. Backend split/join + fixtures is headless-buildable and a good batch; the dialog is the GUI half.

## Work Log

### 2026-08-08 — Backend module implemented + shipped; GUI dialog split off as CPE-1509

**Backend-first split (mirrors CPE-1478/1485/1490):** this pass builds only the headless split/join
engine. The GUI dialog (part-size chooser + pick-parts-to-join + context-menu entries) is scoped
separately as **CPE-1509** and filed to the Backlog — this ticket stays in `Doing/` with the backend
half done.

**New module (`crates/server/src/split_join.rs`):**
```rust
pub fn split_file(path: &Path, part_size: u64, out_dir: &Path) -> Result<SplitManifest, String>
pub fn join_files(first_part_or_manifest: &Path, out_path: &Path) -> Result<(), String>
```
`SplitManifest { original_name, total_size, part_count, part_size, sha256 }` — deliberately small: the
part sequence width (zero-padding) and each part's expected size are re-derived from
`total_size`/`part_size`/`part_count` rather than stored per-part. Written as
`<original_name>.split-manifest.json` alongside the numbered parts (`<original_name>.001`, `.002`, …,
width 3 unless `part_count` itself needs more).

**Streamed + bounded both ways, single-pass checksum:** both functions read/write through a fixed 1 MiB
buffer (`CHUNK_SIZE`) regardless of file size — a multi-GB file never loads whole into memory. `split_file`
computes the whole-source SHA-256 in the same pass it writes parts (via `sha2::Sha256`, reusing the same
crate `fsutil::sha256_file` already depends on — no new hashing dep, no second pass). `join_files`
recomputes the SHA-256 while it concatenates parts into `out_path`, then compares to the manifest.

**Guardrails:** `part_size == 0` → `Err` before touching the disk. A `part_size`/file-size ratio that
would create more than `MAX_PARTS = 100_000` parts → `Err` before any part is written (protects both a
careless split call and a hostile manifest's claimed `part_count` on join). A missing part on join → `Err`
naming the part; a part whose on-disk size doesn't match what the manifest implies → `Err` (catches
truncation even when the checksum hasn't been reached yet); a same-size but corrupted part (byte flip) →
caught by the final whole-file checksum comparison → `Err`. **Any** join failure past the point `out_path`
was created (missing/short/corrupt part, I/O error, checksum mismatch) removes the partial `out_path`
before returning — factored into one `join_into` inner function with a single cleanup point at the call
site, so a failed join never leaves a truncated file that could be mistaken for a good one. A loaded
manifest is validated (`part_size` nonzero, `part_count` under the cap, `original_name` is a single plain
path component — not `..`/absolute/separator-bearing, `sha256` is 64 hex chars, `total_size` consistent
with `part_size × part_count`) before it's trusted to locate any part file. Manifest reads are capped at
64 KiB before parsing (a hostile stand-in file can't be read whole). **Overwrite policy (decided +
documented in the module doc):** `split_file` refuses if the manifest or any target part already exists in
`out_dir`; `join_files` refuses if `out_path` already exists — both fail loudly rather than silently
clobbering; a caller that wants to replace prior output removes it first.

**0-byte source — decided + documented:** `part_count == 0`, no part files are written at all; the
manifest alone (with `sha256` of the empty string) is enough for `join_files` to recreate a 0-byte
`out_path`.

**Join accepts either entry point:** the manifest path directly (`<name>.split-manifest.json`), or any one
numbered part (`<name>.NNN`, from which the manifest path is derived in the same directory) — matching the
ticket's "locate the ordered parts from the manifest, or by the `.001/.002/…` sequence."

**Tauri commands (`src-tauri/src/lib.rs`):** `async fn split_file(path, part_size, out_dir) ->
Result<SplitManifest, String>` and `async fn join_files(first_part_or_manifest, out_path) -> Result<(),
String>`, both `spawn_blocking`-wrapped one-line dispatchers into `cpe_server::split_join`. Registered in
both `generate_handler![]` and the `export_bindings` `collect_commands![]` list, next to
`verify_all_baselines`. `bindings.gen.ts` regenerated (`cargo run --bin export_bindings --features
"specta-bindings sidecar-platform"`): additive 57-line diff adding `splitFile`, `joinFiles`,
`SplitManifest`; nothing else drifted. No new Cargo dependency — `git diff` on `Cargo.lock` (both
`crates/server` and `src-tauri`) is empty.

**Tests (`crates/server/src/split_join.rs`, 12 new, tempdir fixtures, cleaned up after each test):**
round-trip on a part-size-exact-multiple file (no ragged final part); round-trip with a ragged final part;
`part_size == 0` → `Err`; missing part on join → `Err`, no panic, no output left behind; corrupted
(byte-flipped) part of the correct size → checksum-mismatch `Err`, output removed; 0-byte source → 0
parts, manifest-only, join reproduces a 0-byte file; a ~6 MiB synthetic input with a `part_size` not
aligned to `CHUNK_SIZE` → round-trips correctly (exercises both multi-chunk-per-part and
multi-part-per-chunk boundary handling — the behavioral proxy for "never buffers the whole file", since
`CHUNK_SIZE` stays a fixed 1 MiB read regardless of source size); split refuses to overwrite an existing
manifest/part; join refuses to overwrite an existing `out_path` (and leaves the pre-existing file
untouched); a hostile `part_size` that would exceed the part cap is rejected before writing anything; a
hand-crafted manifest with `part_count` over the cap is rejected on join before touching any part file;
joining from a part path vs. the manifest path produce identical output.

**Verification results:**
- `cargo test` crates/server (default features): 1788 passed, 0 failed (was 1776 before this ticket; +12
  new `split_join` tests, no regressions).
- `cargo clippy --all-targets -D warnings` crates/server: clean, default features and `--features specta`.
- `cargo build` src-tauri: clean.
- `cargo clippy --all-targets -D warnings` src-tauri: clean, default features and `--features
  "specta-bindings sidecar-platform"`.
- `bindings.gen.ts` regenerated as noted above (typed-bindings drift-guard test
  `typed_bindings_are_committed_and_routed_through_busy_cursor` passes); no `Cargo.lock` drift in either
  lockfile.
- `npm run check` NOT run — no frontend code touched (explicitly backend-only this pass); GUI consumer is
  CPE-1509.

**For Reviewer to scrutinize:** the bounded/streamed I/O on both split and join (fixed 1 MiB buffer,
single-pass checksum, no whole-file/whole-part read anywhere); the missing/short/corrupt-part handling and
the single-cleanup-point-on-failure design in `join_into`/`join_files` (never leaves a truncated or bogus
`out_path` behind, never panics); the hostile-manifest validation (`part_count` cap, `original_name`
path-traversal guard, manifest-size cap) before any part file is touched; and the overwrite-refusal policy
for both directions (decided here since the ticket left it open — flagged in case the future GUI dialog
wants an explicit "replace existing" affordance instead of requiring the caller to delete first).

**Status:** backend done; GUI = CPE-1509. Leaving this ticket in `Doing/` for the Foreman to disposition.

## 2026-08-08 (sprint) — BACKEND SHIPPED (PR #727, merged); DEFERRED pending GUI dialog CPE-1509
`crates/server/src/split_join.rs` (`split_file`/`join_files`, streamed 1 MiB buffers, single-pass sha256,
traversal-safe hostile-manifest validation) merged + gauntlet-verified (Reviewer caught + fixed a manifest
overflow-panic; UAT: round-trip byte-identical, corrupt/missing→Err, traversal rejected, 200 MiB → ~5.9 MB RSS).
Remaining scope — the split/join dialog + context-menu — is **CPE-1509**. Deferred (not Done) pending that.
