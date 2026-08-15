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
