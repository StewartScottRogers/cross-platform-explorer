---
id: CPE-1973
title: the ZIP symlink branch is redirected by a **planted** inside-pointing link, **deletes a file the archive never named**, and reports `Ok`
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

**Data loss on `main` today, from a planted link — no race required.** Found by PR #1084's Security
Auditor and measured on real ext4 against that PR's unmodified head.

Fixture: a zip with one entry `sub/victim`, external attributes `0o120777` (`S_IFLNK`), content
`benign.txt`. `dest/sub` is a **planted symlink** to `dest/other` — no privilege needed, no race.
`dest/other/victim` is a pre-existing user file.

```
outcome = Ok(ArchiveReport { done: 1, failed: 0, skipped: 0, cancelled: false, errors: [] })
dest/other/victim is now a symlink: true      link target: Some("benign.txt")
its content reads back as: None               <- the user's file was DELETED
```

**The chain:** `link_target_action` canonicalises `dest/sub/victim` **through** the planted link, lands
in `dest/other`, and correctly answers *inside* — the exact blind spot CPE-1938 exists for. Then
`create_entry_symlink` by path hits `AlreadyExists`, and **`fs::remove_file(out)` re-resolves `sub`
through the link and unlinks `dest/other/victim`.**

## Why it survived CPE-1938's sweep

Two statements in PR #1084 assert this path is covered, and both are wrong:

- The per-path table lists ZIP rows 15/16/23 as *"already handle-gated (CPE-1913) — component walk not
  needed."* But `create_dir_beneath` is called only under `if entry.is_dir()` and `create_beneath` only
  in the file branch — **the symlink sub-branch has no handle walk at all.**
- Its residual note says *"a planted (non-racing) link at a component is refused, because
  `link_target_action`'s `confined_to` resolves the whole path."* **False for an inside-pointing
  planted link** — the note re-commits the exact error it was written to bound. It also claims the
  residual *"creates a link, never bytes — `create_entry_symlink` is exclusive-create, so it clobbers
  nothing."* **The `AlreadyExists` retry IS the clobber, and it is a delete.**

That is the **CPE-1972 shape** in a second subsystem: an operation reports success while removing data
the user never asked it to touch. Here it is worse in one respect — CPE-1972 needed an unreadable
directory, this needs only a symlink anyone with write access to the destination can plant.

## The fix is cheap and does not need a new primitive

Call `entry_component_action` / `create_dir_beneath` on the **symlink entry's parent chain** before
`materialise_entry_symlink`. The handle walk refuses a planted link at a component *before* the by-path
`symlink` is ever attempted. Only the **raced** case would need `symlinkat`, which does not exist in
`open_beneath` yet (`remove_file_beneath` landed in CPE-1937; `symlinkat` did not) — and that residual
is already recorded.

## Acceptance criteria

- [ ] **Reproduce first**, with the fixture above, and **assert on the filesystem** — the victim's bytes
      — never on the returned `ArchiveReport`. That report says `done: 1, errors: []` while destroying a
      file, which is the entire point.
- [ ] Fix by walking the symlink entry's parent chain. **Red-proof it**: planted link → refused, victim
      intact; and the refusal must name the component, not an attacker-controlled absolute path.
- [ ] **Run the CPE-1929 pair** on whatever refusal you add — disable it and see whether the suite stays
      green, force its predicate to lie and see whether behaviour changes — and **write both numbers at
      the site**.
- [ ] **Correct the two false statements in the per-path table and the residual note.** They are what
      let this survive a sweep that was specifically looking for it, and the table is what the next
      audit reads.
- [ ] **Check the other `remove_file`-on-`AlreadyExists` retries.** A by-path delete used to clear the
      way for a by-path create is the general shape here; enumerate them at run time (CPE-1932) and
      report a verdict per site.
- [ ] Say plainly what remains for the **raced** case, and whether `symlinkat` in `open_beneath` is
      worth its own ticket — it would also unblock `copilot::apply_op` and CPE-1961.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1084's Security Auditor. **Being fixed in that PR's
round 3**; this ticket exists so the defect has a record independent of it, since it is live on `main`
now. If it closes entirely there, close this **as verified there, with the measurements** — not as a
duplicate.

Related: **CPE-1938** (PR #1084 — the sweep this survived, and where it is being fixed), **CPE-1972**
(the same report-success-while-deleting shape in backup), **CPE-1913** (the handle gates the ZIP file
and dir branches already use), **CPE-1937** (`remove_file_beneath`; `symlinkat` still absent),
**CPE-1961** (the other `renameat`/`symlinkat`-blocked work).
