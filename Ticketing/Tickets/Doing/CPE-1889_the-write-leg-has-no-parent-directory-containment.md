---
id: CPE-1889
title: the write leg has no parent-directory containment, so a junction one level up writes outside the root and reports success
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-25
closed:
---

## Problem

CPE-1857 and CPE-1879 made writes refuse a link **at the final path component**. A directory junction
**one level up** still routes the write outside the root entirely, and the operation reports
`ok: true` with an empty error.

Measured by the independent Security Auditor on PR #1022, through the public `apply_backup_plan`:

```
A3 result: ok=true err="" path=…\dst\sub/authorized_keys
A3 outside file now: "ATTACKER PAYLOAD"      <- outside the backup root, reported as SUCCESS
A4 result: ok=true                            <- created a NEW file outside the root
```

Identical on `main`. The cause is that `copy_one_verified` calls `std::fs::create_dir_all(parent)`
before the guarded open, and `create_dir_all` walks straight through a junction.

## Why High — this is the cheap route to the harm the other tickets closed expensively

- **It needs no privilege at all on Windows.** A symlink needs `SeCreateSymbolicLinkPrivilege`; a hard
  link needs a pre-existing second name at one exact filename. A junction (`mklink /J`) needs neither.
- **One junction redirects an entire subtree**, not a single name.
- **It reports success** — no refusal, no error text, nothing in the failed count. The silent-success
  shape, not a loud skip.
- The entry names come from the *source* tree, so an attacker only has to plant the junction in the
  **destination** — which for a backup is by design an external drive or a network share, the least
  defended directory the user has.

## The asymmetry that shows the fix is already known

The mirror-**delete** leg of the same engine **is** protected: it asserts `contained_under` on the
resolved path, and the auditor confirmed a delete through the same junction is refused (A10). So
within one engine, deletes are containment-checked and writes are not. `apply_backup_plan_walk`'s own
doc comment already names this and calls the fix "a separate change". This is that change.

## What to do

1. **Resolve the parent before writing and assert containment**, the way the delete leg already does.
   Read how `contained_under` is used there and match it rather than inventing a second mechanism.
2. **Mind the TOCTOU.** Resolving a parent, then writing, opens a window. The final-component guard
   avoided this by reading facts off the handle it already had. Say what your approach does about the
   window — narrowing it honestly is fine; claiming to close it when you have not is the defect this
   repo keeps closing.
3. **Cost.** This runs per file on the backup engine's inner loop. CPE-1879's reviewer measured the
   existing guard at 3–5 extra syscalls per file; say what yours adds, and what that means for a
   100k-file backup to a network destination where each round trip is expensive.
4. **Scope.** The same create-then-write shape may exist on the archive-extract and transfer-download
   paths CPE-1857 covered. Check them; fix the class or state which sites you left and why. CPE-1857's
   committed verdict table over 54 write sites is the map.
5. **Correct the docs on the way through.** `copy_one_verified`'s comment and `src/docs/safety-undo.md`
   were deliberately scoped to "the final path component only" *because* this was open. When it closes,
   both need updating — and if you fix only some sites, they must say which.

## Acceptance criteria

- [ ] A3 and A4 reproduced on today's code, then shown refused. Both pasted.
- [ ] The refusal is reported per file, never a silent skip.
- [ ] The write and delete legs now agree on containment.
- [ ] Cost measured and stated.
- [ ] Docs updated to match whatever is actually true afterwards.

## Work Log

- **2026-08-25 15:30 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  the Security Auditor's finding on PR #1022. Deliberately kept out of that PR: different fix, different
  code path. The auditor's `SEC PASS` was explicit that it meant the live holes are truthfully
  documented and ticketed, not that they are closed.
