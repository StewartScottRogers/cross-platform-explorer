---
id: CPE-1913
title: every other containment gate in cpe-server reports success on an escaped write — archive, transfer, revert and copilot all check-then-create with no landing check
type: bug
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-26
---

## Summary

CPE-1896 added `landed_inside` to the backup engine — a post-write check that asks where the bytes
actually went. It is **the only one in the crate.** Four other subsystems do check-then-create with the
same window class and no landing check at all, and all four reach the same silent-success shape:

| site | check → write gap | success shape on escape | who chooses the path |
|---|---|---|---|
| `archive::entry_dir_action` (`archive.rs:838`) | **0 syscalls** on the zip leg; the *entire rest of the archive* on the one-shot tar leg (dir entries deferred to a second pass, `tar_unpack_with:2781` → `:2818`) | `Ok(ArchiveReport { done: 1, errors: [] })` | the archive |
| `archive::entry_sink_action` (`archive.rs:764-809`) | ≥2, unbounded in tree depth; then a plain by-path `fs::File::create` at `:3589` — not `create_new`, not `O_NOFOLLOW` | same | the archive |
| `transfer::download_tree` (`transfer.rs:651`) | 4 local syscalls **plus a full remote file-body fetch** (`provider.read` at `:806`) before `fs::write` at `:807` | `files += 1`, `Ok(n)` — **no per-entry result channel at all** | **the remote server**, every segment |
| `revert_engine::apply_write` (`revert_engine.rs:852`) | ~6-7; the only post-write `canonicalize` is inside the `map_err` closure at `:961` — the *failure* path, for wording only | `report.applied += 1`, `skipped: []` | checkpoint-manifest JSON, against the user's live tree |
| `copilot::apply_op` (`copilot.rs:355`) | 2 (Mkdir) to ~9-10 (Move/Copy) | `OpResult { ok: true, error: "", outcome: Applied }` | LLM plan paths in a user-confirmed folder |

**`transfer::download_tree` is the worst of the five** and should be done first: the widest window (an
entire network transfer sits inside it), the most attacker-controlled name source (a remote SFTP/FTP/
WebDAV server chooses every path segment), and **no per-file reporting channel to be honest in even if
it detected the problem.**

**`archive::entry_dir_action`'s zip leg has a literally zero-syscall gap** — which sounds safe and is
not, because the tar leg defers directory entries to a second pass, putting the whole remainder of the
archive inside the window.

And `archive.rs:426-435` **already records the pre-race version of exactly this shape**
(`landed_outside=TRUE  Ok(ArchiveReport { done: 1, errors: [] })`). The file knows the shape. The fix
was never generalised.

## Acceptance criteria

- [ ] Give `transfer::download_tree` a per-entry result channel first. It currently cannot report a
      per-file failure at all, so no containment fix there can be honest until this exists. Treat that as
      its own deliverable and land it before the guard.
- [ ] Add a landing check to each of the five sites, or — better — extract the one `landed_inside` proved
      out in `backup.rs` into a shared helper so there is one implementation and five call sites rather
      than five implementations. This ticket exists because a fix was written once and not generalised;
      do not repeat that.
- [ ] Fix `archive.rs`'s by-path `fs::File::create` at `:3589` to the `create_new` / no-follow pair the
      rest of the crate uses (CPE-1718's `create_slot_refusal` + `create_exclusive`).
- [ ] Red-proof **each** site with its own harm test asserting on the filesystem — the bytes arriving
      outside the root — never on the returned `Result`. Assert the escape is reported as a failure.
- [ ] Consider splitting this into five tickets once the shared helper exists. It is filed as one because
      the diagnosis is one; the work plausibly is not.

## Notes

Filed 2026-08-26 by CPE-1896's independent Security Auditor, which spot-checked all five sites rather
than the two it was asked for.

Related: **CPE-1896** (the backup landing check, the only one that exists), **CPE-1912** (a junction
inside the root, no race required), **CPE-1898** (the source leg), **CPE-1889** (the static parent
containment), and the resolve-before-write family: CPE-1744/1759 (archive), CPE-1742 (transfer),
CPE-1750 (copilot), CPE-1623 (batch media).

Note the distinction that matters for triage: a previous audit confirmed all five sites **do** check
containment before `create_dir_all`, and that finding was correct. This ticket is not that they skip the
check — it is that the check happens before the write and nothing verifies afterwards, so an escape that
wins the window is reported as a success. Same shape as CPE-1896, four more places.
