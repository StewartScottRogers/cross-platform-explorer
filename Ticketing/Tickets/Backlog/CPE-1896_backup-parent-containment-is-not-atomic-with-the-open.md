---
id: CPE-1896
title: backup's parent containment is not atomic with the open — a three-syscall parent swap writes outside the root and reports ok:true (measured 73/1200)
type: bug
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-26
---

## Summary

CPE-1889 closed the static case: a junction sitting in the backup destination no longer redirects a
write outside the root. What it did **not** close — and said so honestly, in the code and in
`src/docs/safety-undo.md` — is the race between the containment check and the open.

An independent Security Auditor turned that documented residual into a **measured** one, and the harm
it still reaches is byte-for-byte the shape CPE-1889 was filed for.

**The attack, in the common case** (parent already exists, so the second check never runs):

A junction is parked inside the destination as `dst/junc`. A racing thread does two renames —
`dst/sub` to `dst/sub_old`, then `dst/junc` to `dst/sub`. Rename works on a **non-empty** directory,
so the attacker never needs the slot to be empty.

**The window, in operations, not adjectives.** After check (1)'s `canonicalize(parent)` returns,
exactly three syscalls precede the destination open:

    std::fs::metadata(parent) -> File::open(src) -> r.metadata() -> batch_media::open_no_follow(dst)

**Measured result, `verify = false`:** ESCAPED 73, refused 1086, wrote-inside-normally 41, over 1200
trials. Verbatim from the probe:

    !!! ESCAPE at trial 16: the backup OVERWROTE "...\OUTSIDE\victim.txt" which is OUTSIDE the
    destination root, verdict ok=true err=""

**Verification does not notice.** With `verify = true`: 68 escapes over 1200. The sha256 read-back
re-opens the *same* redirected path, so it agrees with itself. **The verify leg is not a mitigation
for this** — that is the single most important sentence in this ticket.

**Blast radius.** It **overwrites** a pre-existing file outside the root, it also creates new ones,
and it reports `ok: true` with an empty error — the silent-success shape, not a loud skip. It is
**targeted, not arbitrary**: the attacker chooses the junction's target directory (a Startup folder,
an `.ssh` directory, a config dir); the only thing they do not choose is the filename, which comes
from the source tree — and a backup source tree contains thousands of names.

**Precondition:** write access to the destination tree — the same precondition as the bug CPE-1889
fixed, and a backup destination is by design an external drive or a share. Roughly a 6% single-shot
win rate on local NTFS with a naive racer; a real attacker loops, and a run gives one window **per
file**.

## Acceptance criteria

- [ ] Make the containment atomic with the open, per component. `std` cannot do this. It needs
      `openat2(RESOLVE_BENEATH)` on Linux and an `O_NOFOLLOW` directory walk (or `NtCreateFile` with
      `FILE_OPEN_REPARSE_POINT` per component) on Windows. CPE-1889's own doc comment already names
      this; start from there rather than re-deriving it.
- [ ] Land the auditor's race probe as a repeatable test — `#[ignore]`d if it must be, but in the
      tree — so the fix has something that goes red without it. A fix for a race with no racing test
      is unverifiable, and this repo's recurring defect is guards that prove nothing.
- [ ] **Cheaper partial mitigation, worth doing even if the full fix slips:** make the verify leg read
      back through a handle opened relative to a *verified* parent, so that at minimum the engine stops
      reporting `ok: true` on an escaped write. A loud failure is enormously better than a silent one.
- [ ] Decide and record whether the same window exists on the other legs that resolve-then-write
      (archive extract, revert apply, copilot apply, transfer download). The auditor confirmed all of
      them check before `create_dir_all`, but none of them are atomic either.
- [ ] Weigh the syscall cost of a per-component walk against PURPOSE.md's tiebreaker and record it.
      Correctness outranks speed here, but the number should be known — see CPE-1895.

## Notes

Filed 2026-08-26 by CPE-1889's independent Security Auditor, which staged all attacks inside its own
worktree and cleaned up every junction. CPE-1889 merged as PR #1031 with this residual known and
documented; this ticket is the follow-through, not a regression report against it.

Related: **CPE-1889** (the static case, closed), **CPE-1897** (the second check's race probe),
**CPE-1898** (the source leg's missing containment), **CPE-1895** (the syscall-cost measurement).
