---
id: CPE-1861
title: a manifest's inner id can disagree with its filename, and the obvious fix destroys blobs
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

Every checkpoint manifest carries an `id` field, and nothing checks it against the filename it is stored
under. Retention reads that field. Two shapes, both measured through `snapshot_prune::apply`:

```
inner id -> a sibling's id      : apply Ok(kept=[id,id], pruned=[])   nothing pruned, manifest immortal
inner id -> "no-such-manifest"  : apply Err(...)                      the whole retention pass dies
```

The second is the worse one: **one tampered manifest wedges retention permanently** and nothing is ever
thinned again.

## Why this is its own ticket, and why the obvious fix is wrong

CPE-1847 fixed this as an enumeration extra by **deriving the id from the filename**. Its Security Audit
then measured two new regressions that fix introduces, and CPE-1847 was split rather than carry them.

**Regression 1 — a duplicated manifest file destroys the surviving checkpoint.**

```
cp <id>.json <id>-backup.json

before: preview keep=[id,id]  prune=[]           apply pruned=[]
        blobs=[f7e3...]   restore(id)=Ok(())     tree=["a.txt"]
after:  preview keep=[id]     prune=[id-backup]  apply pruned=[id-backup]
        blobs=[]          restore(id)=Err(".../blobs/f7e3...: cannot find the file")   tree=[]
```

The two copies get distinct ids, retention prunes one, `release` drops the **shared** blob refcounts to
zero, the blobs are deleted, and the **kept** manifest can no longer restore anything.
`RetentionApplyResult` reports it as `kept`.

`snapshot_schedule::snapshot_run_due` retention-prunes after every scheduled capture, so this fires
**unattended, with no UI and no user action**. The triggers are ordinary: Explorer copy/paste
(`X - Copy.json`), a cloud-sync conflict copy, a backup script, a partial restore-from-backup — and
"a store synced by a cloud client" is CPE-1823's own stated threat premise.

Content destroyed, complete success reported. The same failure grammar CPE-1847 exists to close.

**Regression 2 — a crafted filename wedges the pass.**

```
plant a..b.json (a copy of any manifest)
before: apply -> Ok(kept=[id,id], pruned=[])
after:  apply -> Err("a..b: not a valid manifest id")    every pass, forever
```

That is the *original* harm, relocated from the inner field to the filename rather than removed. `..` in
a stem suffices on any platform; on Unix `:` or `\` does too.

## The design choice this ticket must settle

The Auditor tested a candidate and it is **not a drop-in**:

`if m.id != id { continue; }` in `list_manifests` — a **skip**, matching that function's own documented
skip-the-unparseable guardrail, and deliberately *not* the `load_manifest` refusal (which would wedge the
pass). Measured: the duplicate case returns to `pruned: []` / `restore = Ok(())` / tree restored,
`a..b.json` returns to `Ok`, and an inner-id lie neither steers nor wedges.

It costs CPE-1847's prune test, which asserts the liar **is** pruned. So the real question is
**skip-and-leak versus prune-the-liar**, and it has to be decided rather than defaulted.

The Auditor's alternative, and the better shape if it holds: fix `prune` instead — **do not release refs a
surviving manifest still holds**. That closes regression 1 at its cause rather than by declining to prune,
and it would protect against any future path that prunes something sharing blobs.

## Acceptance criteria

- [ ] An inner id disagreeing with its filename neither steers retention nor wedges the pass.
- [ ] **The duplicated-manifest fixture must show `restore(<kept id>) = Ok(())` with its tree intact after
      a retention pass.** This is the gate; nothing merges without it.
- [ ] `a..b.json` (and a filename with `:` or `\` on Unix) must not turn the pass into `Err`.
- [ ] Decide skip-and-leak versus prune-the-liar versus fixing `prune`'s refcount release, and record why.
      If refs are the fix, state the invariant plainly: one manifest, one refcount, and a release must not
      drop a blob another manifest still names.
- [ ] Assert each test's fixture is live — that the tamper landed on disk **and** reached the planner —
      before asserting harm. CPE-1823 caught six inert tests; CPE-1847's three-sabotage liveness check is
      the pattern to copy.
- [ ] Red-proof every test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by CPE-1847's worker while enumerating, fixed there, and split back out after that ticket's Security
Audit measured the two regressions above. Read CPE-1847's Work Log first — it carries the measurements and
the reason the split was taken.

Related: CPE-1847 (the zero-entry stand-down that shipped), CPE-1844 (`index.json` steering prune — the
same store, the same "a hand-editable file steers a destructive decision" shape).
