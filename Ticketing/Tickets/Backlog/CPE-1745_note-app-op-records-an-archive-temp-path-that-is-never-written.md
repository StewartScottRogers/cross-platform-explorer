---
id: CPE-1745
title: note_app_op records an archive temp path that is never written (drifted from CPE-1195)
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## The defect

Three `#[tauri::command]` wrappers in `src-tauri/src/lib.rs` — `extract_archive_entry` (~8206),
`extract_archive_entry_any` (~8232) and `extract_rar_entry` (~8258) — call `note_app_op` with a path each
comment describes as *"the exact temp-file target `cpe_server::archive::…` will write"*, built as:

```rust
std::env::temp_dir().join("cpe-archive").join(base)
```

The server has not written that path since **CPE-1195**. `cpe_server::archive::temp_extract_target` writes
`%TEMP%/cpe-archive/<pid>-<seq>/<base>` — the per-extraction subdirectory added to stop two concurrent
extractions of same-named entries racing each other. So all three records name a path that **does not
exist and never will**, and each does it under a comment asserting the opposite.

Found by the PR #906 reviewer while checking CPE-1733's enumeration of the same temp target.

## Why it matters

`note_app_op` feeds Agent Watch's record of what the app touched. A recorded path that was never written
is worse than no record: it is a confident false statement, and the whole point of Agent Watch is showing
what actually happened on disk. It is the same failure shape as CPE-1687 — a message sending someone to
look at the wrong path.

CPE-1733 has since hardened `temp_extract_target` further (exclusive `fs::create_dir`, retrying the next
sequence number when a name is taken), so the `<pid>-<seq>` component is now **not** predictable from
outside the call: on a retry the sequence number actually used is not the one the caller would guess. That
makes "mirror the derivation in the adapter" the wrong fix.

## What to do

- [ ] **Do not re-derive it a fourth time.** The adapter has been mirroring a server-side derivation by
      hand, which is exactly how it drifted, and the retry loop means the mirror can no longer be correct
      even in principle. Have the server hand the real path back instead — the commands already return
      the written path as their `Ok` value, so recording *after* the call (or having the server expose the
      target) is both simpler and correct.
- [ ] Note the ordering consequence and decide it deliberately: recording after the call means a failed
      extraction records nothing. Check what the other `note_app_op` sites do before assuming that is a
      loss — for a path that was never written, recording nothing is the accurate answer.
- [ ] A test that would have caught this: assert the recorded path **equals** the path the command
      returned. Assert on equality with the real value, not on the shape of the string.
- [ ] Check the other `note_app_op` call sites for the same mirror-by-hand pattern while in there, and
      record what was checked.

## Notes

Filed by the CPE-1733 worker from the PR #906 review, 2026-08-14 — flagged by the reviewer as an aside
outside that ticket's scope. Related: **CPE-1195** (the `<pid>-<seq>` subdirectory this drifted from),
**CPE-1102** (the `note_app_op` records), **CPE-1733** (the enumeration and the exclusive-create
hardening), **CPE-1687** (a message that named the wrong path).

## Work Log — 2026-08-18

**Fix.** `extract_archive_entry`, `extract_archive_entry_any`, `extract_rar_entry`
(`src-tauri/src/lib.rs`) no longer call `note_app_op` before extracting with a hand-derived
`temp_dir().join("cpe-archive").join(base)` guess. Each now runs the `spawn_blocking` extraction first,
then calls a new shared helper, `archive_extract_op_paths(&result) -> Vec<String>`, which returns
`vec![path.clone()]` from the real `Ok` value or nothing on `Err`, and feeds that into `note_app_op`
*after* the call. No fourth re-derivation of `temp_extract_target`'s private `<pid>-<seq>` subdirectory
anywhere — the path recorded is always the literal value the command already returns to the frontend.

**Record-before vs record-after — decided at the call site.** Chose record-after, success-only. Recorded
in the `archive_extract_op_paths` doc comment (`src-tauri/src/lib.rs`, right above the three commands):
a failed extraction wrote nothing, so recording nothing is the accurate ledger entry, not a loss. This
matches the existing shape of `organize_apply` and `template_stamp` elsewhere in `lib.rs`, both of which
already record only after their `spawn_blocking` result is known, and both already success-gated
(`organize_apply` filters `outcome.results` to `r.ok`; `template_stamp` only calls `note_app_op` inside
`if let Ok(created) = &result`). Record-after is not a novel shape in this file — the archive-extraction
sites were the outliers by recording *before*, on a guess.

**Other `note_app_op` call sites checked — none had the same mirror-by-hand pattern.** Read every call
site in `src-tauri/src/lib.rs` (grep `note_app_op`, ~20 sites): `create_dir`/`create_file`/
`create_file_with_content`/`create_empty_zip` join caller-supplied `path`+`name` directly (deterministic,
no hidden server-side randomness or retry); `copy_entries`/`move_entries`/`start_transfer` predict
`dest/<name>` and are explicitly commented "best-effort... may differ" (an auto-rename on collision can
legitimately diverge, and the comments already say so, unlike the archive sites' false "the exact...
will write" claim); `delete_to_trash`/`delete_permanent`/`shred_paths`/`move_exact`/`macro_run`/
`macro_undo` record caller-supplied paths, not derived ones; `run_watch_actions` best-effort-simulates
its own planner and is commented as such; `organize_apply`/`template_stamp` already record after the
call (see above — used as this fix's precedent, not a bug). The only sites deriving a *hidden,
server-internal, retry-affected* path from scratch and asserting it was exact were the three archive
extraction commands. None of the others need the same fix.

**Tests** (`src-tauri/src/lib.rs`, in `mod tests`, next to `create_empty_zip_makes_a_valid_openable_archive`):
- `archive_extract_op_paths_records_the_real_written_path_on_success` — builds a REAL zip via
  `cpe_server::archive::compress_to_zip`, extracts it via the REAL
  `cpe_server::archive::extract_archive_entry`, and asserts `archive_extract_op_paths(&result) ==
  vec![real_path.clone()]` — equality with the actual returned value (which includes the real
  `<pid>-<seq>` subdirectory), not a shape/prefix check. Also asserts the recorded path is a real file on
  disk with the real extracted bytes, and asserts it does NOT equal the old flat
  `temp_dir()/cpe-archive/<base>` guess.
- `archive_extract_op_paths_records_nothing_on_a_real_failure` — extracts a missing entry from a real
  (empty) zip, a genuine `Err`, and asserts `archive_extract_op_paths` returns `vec![]`.
- Cleanup: both tests use the existing `scratch()` helper for their own directory; the success test also
  arms a second `Drop` guard (`Cpe1745ExtractGuard`), armed immediately after the extraction call
  returns and before any assertion, to remove the real `%TEMP%/cpe-archive/<pid>-<seq>/` directory the
  extraction wrote (outside `scratch()`'s tree) — mirroring this file's `Cpe1715Scratch` convention and
  `cpe_server`'s `split_join`/`dispatch` `Restore` guards.

**Red-proof.** Committed the fix + tests first (commit `d10f4451`). Then temporarily rewrote
`archive_extract_op_paths` to reproduce the pre-fix behavior — deriving the flat
`temp_dir().join("cpe-archive").join(base)` guess from the real returned path's file name instead of
using the real path — and ran `cargo test --lib archive_extract_op_paths`:

```
thread 'tests::archive_extract_op_paths_records_the_real_written_path_on_success' panicked at src\lib.rs:15272:9:
assertion `left == right` failed: note_app_op must be fed the exact path the extraction actually returned
  left: ["C:\\Users\\Stewart Rogers\\AppData\\Local\\Temp\\cpe-archive\\note.txt"]
 right: ["C:\\Users\\Stewart Rogers\\AppData\\Local\\Temp\\cpe-archive\\24332-0\\note.txt"]
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 188 filtered out; finished in 0.01s
```

The `left` value is exactly the old bug's flat guess; `right` is the real `<pid>-<seq>` path the
extraction actually wrote — proof the equality assertion bites on the real defect shape. Restored the
committed good state with `git checkout -- src-tauri/src/lib.rs` (safe, already committed) and reran:
both tests green again.

**Verification.**
- `cargo test --lib archive_extract_op_paths` (src-tauri): 2 passed.
- `cargo test` (src-tauri, default features): 190 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings` (src-tauri, default features): clean.
- `cargo clippy --all-targets --features sidecar-platform -- -D warnings` (src-tauri): clean.
- `cargo test` (crates/server): 1 pre-existing, unrelated flake
  (`zip_lists_real_tree_and_extracts_inner_file`, failed with "could not claim a private extraction
  directory... after 1024 attempts") caused by this machine's ~1.29 million leaked `%TEMP%/cpe-archive`
  directories from prior runs colliding with a reused PID (CPE-1693 tracks the leak; not touched by this
  ticket) — reran alone immediately after and it passed. Full suite otherwise green.
- `cargo clippy --all-targets -- -D warnings` (crates/server): clean.
- No `specta::Type` struct touched — no `bindings.gen.ts` regen needed. No dependency added — no
  `Cargo.lock` regen needed.
