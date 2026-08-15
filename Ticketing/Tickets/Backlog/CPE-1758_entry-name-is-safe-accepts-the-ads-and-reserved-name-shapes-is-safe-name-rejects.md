---
id: CPE-1758
title: entry_name_is_safe accepts the ADS and reserved-name shapes is_safe_name rejects
type: task
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-15
closed:
---

## Why this exists

Split out of **CPE-1744** by the worker that closed its containment half, so the remainder is scheduled
rather than left as a sentence. CPE-1744 asked whether `crate::transfer::guarded_join` could be adopted
wholesale at `archive.rs`'s extraction sinks. **Half of it was**, and half deliberately was not:

- `guarded_join`'s **filesystem-resolving** half — does the joined path still land under the base once
  every intermediate component is followed — was the intermediate-directory escape, and CPE-1744 closed
  it at rows 15/16/18/19/20 via `fsutil::confined_to`.
- `guarded_join`'s **per-segment name** half is this ticket. It applies
  `crate::transfer::is_safe_name` to each segment (fails closed on a `:` anywhere and on a leading `..`,
  CPE-1461/1709) and on Windows sanitises through `local_safe_segment`. `archive::entry_name_is_safe`
  has **no equivalent to either**, and that is a question about what a name may *be*, not about where a
  path *lands* — a different guard with a different blast radius, shared with the `local_safe_segment`
  family rather than with `confined_to`.

## The measurement (from PR #906's UAT, unchanged)

```text
[M7] entry_name_is_safe("file:stream") = true    entry_name_is_safe("..evil") = true
     entry_name_is_safe("con") = true            entry_name_is_safe(" sp ") = true    ("x." = true)
[M8 fs::write to "adsbase:stream"] = Ok(())
     adsbase_len = Some(4) (unchanged)   a plain file named "adsbase:stream" exists = false
```

A ZIP entry named `file:stream` passes `entry_name_is_safe`, reaches rows 15–16's `File::create`, and on
NTFS the bytes land in an **alternate data stream of a neighbouring file** — the user is shown a
successful extraction and has no file. That is CPE-1709's bug at a sink CPE-1709 did not cover. The
Windows reserved-device names (`con`, `nul`, …) and the trailing-space/trailing-dot shapes are accepted
too.

## What to do

- [ ] Decide and write down first: adopt `local_safe_segment`/`is_safe_name` per segment at
      `entry_name_is_safe`, or a third implementation. Adopting is strongly preferred — three
      implementations of "is this leaf name safe" is how `deny_stat_of` ended up needing the same fix
      three times.
- [ ] Note the **rename vs refuse** decision explicitly: `local_safe_segment` *sanitises* (the transfer
      sink renames the file so the bytes still arrive), while `entry_name_is_safe` *refuses* (the entry is
      skipped). Extraction may want the sanitising behaviour — an entry silently dropped is the same
      "successful extraction, no file" outcome the ADS bug produces. Whichever is chosen, the in-app docs'
      zip-slip bullet describes the current one and must move with it.
- [ ] `entry_name_is_safe` is `pub(crate)` and `crate::extract_plan` reuses it (CPE-1055) — check that
      caller before changing the contract.
- [ ] **`archive::tests::entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects` will go red.
      That is the intended signal.** Re-point it at the new behaviour in the same commit; never delete it.
      The `archive.rs` section-comment paragraph that records this delta, and the table above it, move in
      the same commit too.
- [ ] Every guard broken on its own, a **distinct** test red, real output pasted. Assert on the filesystem
      and the bytes **before** unwrapping the `Result` — this whole family fails by returning `Ok`.
- [ ] Pin a **distinctive** refusal, never `is_err()`: on Windows several of these shapes make
      `File::create` fail by itself, so an `is_err()` leg passes straight through a deleted guard.

## Notes

Filed by the CPE-1744 worker, 2026-08-15. Related: **CPE-1744** (the containment half, closed),
**CPE-1709**/**CPE-1461** (`is_safe_name`/`local_safe_segment`, the ADS shape at the transfer sink),
**CPE-1733** (the enumeration that first recorded this delta), **CPE-1759** (the other CPE-1744 remainder).
