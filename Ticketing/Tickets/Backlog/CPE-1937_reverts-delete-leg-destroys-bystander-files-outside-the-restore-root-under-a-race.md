---
id: CPE-1937
title: revert's delete leg destroys bystander files **outside the restore root** under a race — 596 in 200 trials, every one counted as `applied`
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`revert_engine::apply_delete` (`revert_engine.rs:1149-1156`) still resolves by path — `safe_target` →
`confined_to` → `fs::remove_file` — and `confined_to` **cannot see a junction that points inside the
root**. Both paths are within the restore folder, so the containment check answers yes, and the
delete lands on a file the plan never named.

Demonstrated 2026-08-27 by PR #1050's independent Reviewer. Junction `<dest>/sub -> <dest>/other`,
plan of a single `RestoreOp::Delete` for `sub/victim.txt`:

    report = RestoreReport { applied: 1, skipped: [], held_back: None, write_refusal: None }
    victim still there = false

**Complete silent success, and a bystander file destroyed.** This is CPE-1912's exact shape — the one
where "both paths are inside the root" made the check answer yes while a subtree was redirected — but
at a **destructive** leg rather than a writing one. CPE-1912 was closed for backup by CPE-1896's
per-component walk; delete never got it.

## Why High

- **It destroys data rather than misplacing it.** A redirected *write* puts bytes somewhere wrong; a
  redirected *delete* removes something that still mattered, and there is nothing to recover from.
- **It is silent.** `applied: 1`, empty `skipped`, no `held_back`, no `write_refusal`. The
  silent-success shape that CPE-1896 and CPE-1913 exist to eliminate, still live.
- **It is in the undo subsystem.** Revert is what a user reaches for *after* something went wrong.

## Mitigation that exists today, and why it is not enough

`cpe_1823_a_skipped_write_holds_back_every_delete_in_the_plan` stands down **every** delete in a plan
if **any** write is refused. So reaching this needs a plan whose deletes are not accompanied by a
refused write — which is an ordinary plan, and is exactly what the Reviewer's probe was. The
mitigation narrows the window; it does not close it.

## Acceptance criteria

- [ ] Convert `apply_delete` to the per-component, handle-relative approach CPE-1896 established and
      CPE-1913 extended — `crates/server/src/open_beneath.rs`. Deleting needs `unlinkat`-shaped
      primitives that `open_beneath` does not currently expose (CPE-1913 named this as the reason it
      deferred Copilot too), so **adding those is part of this ticket**.
- [ ] Check `snapshot_capture::restore`, which the CPE-1913 Work Log records as also keeping
      `safe_target` unchanged. It reportedly has no production caller — verify that, and if it is
      genuinely dead, say so or delete it rather than leaving a second copy of this shape.
- [ ] **The harm test must run the junction BOTH ways** — pointing outside the root and pointing
      inside it — with the inside leg reddening on its own. The outside case has been covered for
      ages; inside is the one that slipped through, twice now.
- [ ] Assert on the **filesystem**, not on the `Result`. The defect's signature is a clean `Result`
      alongside a destroyed file, so a test that reads only the report would pass against the bug.
- [ ] Do not create a shadowed guard (CPE-1929): if a by-path check is left standing in front of the
      new one, the new one becomes untestable. CPE-1913's approach — **delete** the path probes rather
      than stack them — is the pattern to follow.
- [ ] Once fixed, correct `src/docs/safety-undo.md`, which CPE-1913 is amending in the interim to stop
      presenting revert as fully converted.


## Blast radius raised 2026-08-27 — it reaches OUTSIDE the root, measured

PR #1050's independent Security Auditor corroborated the inside-the-root case above **and then went
further**, applying CPE-1896's racing double-rename (`root/sub` <-> `root/junc -> OUTSIDE`) to this
same leg:

    [A8b delete raced out of the root]  trials=200  FILES_DELETED_OUTSIDE=596
                                        applied_total=4327  swaps=1835

**596 bystander files destroyed outside the destination across 200 trials.** Every one counted in
`applied`. None in `skipped`. No `held_back`. No error. This is the **CPE-1896 escape shape at a
destructive leg with no handle guard at all** — strictly worse than the inside-only case this ticket
was originally filed for, which is why the title and priority were raised.

The auditor also confirmed the CPE-1823 delete stand-down does **not** save it: an ordinary plan — a
non-empty, fully restorable checkpoint whose deletes are not accompanied by a refused write — reaches
`apply_delete` directly. That is exactly the fixture it used.

## AC2 answered with evidence (2026-08-27)

The second acceptance criterion below asked whether `snapshot_capture::restore` is genuinely dead.
**It is** — the auditor grepped `crates/` and `src-tauri/` and found the only non-doc references are
`revert_engine`'s own tests and `snapshot_prune`'s test. But it also **measured** that it carries the
same defect:

    [A9 point_outside=false]  REDIRECTED=true   verdict=Ok(())   victim="CAPTURED BYTES"
    [A9 point_outside=true ]  REDIRECTED=false  verdict=Err("... escapes ...")

Identical on `main` and on PR #1050's branch. It has no production caller, but it **is** a public API
of `cpe-server` and a second live copy of the CPE-1912 shape. Delete it or convert it; do not leave it.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1050's Reviewer, which found it while checking
whether that PR's documentation claims matched its code. CPE-1913's Work Log **is** honest about the
gap (line 161: *"`apply_delete` and `snapshot_capture::restore` keep `safe_target` unchanged"*); the
user-facing doc was not, and that mismatch is what surfaced it.

Related: **CPE-1896** (the walk), **CPE-1913** (which converted the writing legs), **CPE-1912** (the
same shape, closed for backup), **CPE-1929** (shadowed guards), **CPE-1935** (another
partial-success-reported-as-success in the extraction leg).
