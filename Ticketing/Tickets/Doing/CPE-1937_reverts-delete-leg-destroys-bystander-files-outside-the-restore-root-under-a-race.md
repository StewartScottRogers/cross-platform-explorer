---
id: CPE-1937
title: revert's delete leg destroys bystander files **outside the restore root** under a race — 596 in 200 trials, every one counted as `applied`
type: bug
priority: High
status: In Progress
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

- [x] Convert `apply_delete` to the per-component, handle-relative approach CPE-1896 established and
      CPE-1913 extended — `crates/server/src/open_beneath.rs`. Deleting needs `unlinkat`-shaped
      primitives that `open_beneath` does not currently expose (CPE-1913 named this as the reason it
      deferred Copilot too), so **adding those is part of this ticket**.
- [x] Check `snapshot_capture::restore`, which the CPE-1913 Work Log records as also keeping
      `safe_target` unchanged. It reportedly has no production caller — verify that, and if it is
      genuinely dead, say so or delete it rather than leaving a second copy of this shape.
- [x] **The harm test must run the junction BOTH ways** — pointing outside the root and pointing
      inside it — with the inside leg reddening on its own. The outside case has been covered for
      ages; inside is the one that slipped through, twice now.
- [x] Assert on the **filesystem**, not on the `Result`. The defect's signature is a clean `Result`
      alongside a destroyed file, so a test that reads only the report would pass against the bug.
- [x] Do not create a shadowed guard (CPE-1929): if a by-path check is left standing in front of the
      new one, the new one becomes untestable. CPE-1913's approach — **delete** the path probes rather
      than stack them — is the pattern to follow.
- [x] Once fixed, correct `src/docs/safety-undo.md`, which CPE-1913 is amending in the interim to stop
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

## Work Log — 2026-08-27

### Decision 1: add the primitive, don't contain the delete another way

`open_beneath` gained **`remove_file_beneath`** — `unlinkat(parent_fd, name, 0)` on Unix,
`NtCreateFile(DELETE) + SetFileInformationByHandle(FileDispositionInfo)` on Windows — rather than
wrapping the existing by-path `remove_file` in a stronger check. Three reasons, in order:

1. **No path check can close this.** Every alternative containment is another path question, and the
   defect is that the path question is answered against a name that then means something else. That is
   the module doc's own argument, and it does not become less true because the operation is a delete.
2. **It is the primitive CPE-1913 named as its blocker.** That PR's enumeration lists `copilot::apply_op`
   as deferred because `renameat`/`unlinkat` did not exist in `open_beneath`. Half of that is now built
   and tested; `apply_op` needs `renameat` on top of it, not a new design.
3. **A delete has no handle to return**, which is why `create_beneath` could not simply be reused: its
   whole product is an open file, and the delete's product is the removal of a name.

The descent is **parameterised, not copied** — one `Act` enum carries the two things that genuinely
differ (whether a missing directory is created, and the verb in a refusal). A delete descends with
`FILE_OPEN`/no-`mkdirat`, so a refused deletion cannot leave directory debris the way a `create_dir_all`
descent would. The refusal wording for every writing leg is byte-for-byte unchanged.

`apply_delete` swapped `safe_target` for `safe_segments` — **replaced, not stacked** (CPE-1929): leaving
`confined_to` in front would make the walk's refusal unreachable for everything the path check already
catches, i.e. a guard nothing can red-proof. And its refusals are now classified by `Refusal::policy`,
so a planted junction is reported **permanent** rather than "run the revert again".

### Decision 2: `snapshot_capture::restore` — converted, not deleted

AC2's premise re-verified: grepping `crates/` and `src-tauri/` finds **no production caller**. It is,
however, load-bearing for the test suite — ~30 tests in `snapshot_capture` plus `snapshot_prune`'s
"a kept manifest still restores" oracle — and it is a `pub` API of `cpe-server`. Deleting it would have
removed a working oracle and a large body of documented reasoning to fix a defect that the primitive
added two files over fixes outright. So pass 2 now writes through `create_beneath` against a root handle
held for the restore.

**Pass 1 keeps `safe_target`, and that is not a shadowed guard.** Pass 1 creates nothing and answers one
question — may this manifest be attempted at all — before a byte is written, which is what gives a
refused manifest its all-or-nothing property. It is a pre-flight. The guard the *write* rests on is the
walk, and it is the only one, which is why the inside-pointing junction (invisible to `confined_to`, and
therefore invisible to pass 1) is caught at the write and can be red-proofed there. It was.

### Reproduction, before the fix

`apply_delete` temporarily restored to `main`'s body (`safe_target` → `fs::remove_file`):

```text
Windows, static, junction <dest>/sub -> <dest>/other (INSIDE the root):
  RestoreReport { applied: 2, skipped: [], held_back: None, write_refusal: None }
  bystander <dest>/other/victim.txt = GONE
```

Exactly the report the ticket describes. The plan is an **ordinary** one — a non-empty, fully restorable
checkpoint whose single write succeeds — so CPE-1823's delete stand-down never arms, confirming the
mitigation does not close this.

### The race, before and after

`cpe_1937_raced_delete_never_escapes_the_restore_root` (`#[ignore]`d; `--release --ignored`). Shape:
`<dest>/sub` is a real directory holding the files the plan deletes, `<dest>/junc` is a link to a folder
**outside** the destination holding identically-named bystanders, and a racer thread swaps the two names
with a triple rename while the revert resolves `sub/vNNN.txt`. The racer **pauses for staging** and the
bystanders are asserted pristine *before* every run — without that the harness writes through its own
junction while staging and reports its own writes as escapes (the CPE-1846 false-escape shape; the first
cut of this test had it).

| | trials | FILES_DELETED_OUTSIDE | applied_total | swaps |
|---|---|---|---|---|
| Windows, before (`main`'s body) | 200 | **106** | 1388 | 615 / 630 attempts |
| Windows, after | 200 | **0** | 351 | 214 / 220 attempts |
| Linux, before (by-path `remove_file` ahead of the walk) | 200 | **75** | — | — |
| Linux, after | 200 | **0** | 1980 | 7407 / 7407 attempts |

Linux completes every swap it attempts (POSIX has no open-descendant rename rule), so it races this
leg far harder than Windows does — and still nothing escapes.

### Red-proofing — four sabotages, on the harm assertions

1. **`apply_delete` = `main`'s body** (Windows). `cpe_1937_a_junction_…_delete` red on the HARM
   assertion with `applied: 2, skipped: [], held_back: None`; the race red at 106.
2. **`restore` pass 2 = `create_dir_all` + `copy_file_onto_no_follow`** (Windows).
   `cpe_1937_a_link_pointing_back_inside_…` red on HARM — the write went through the inside-pointing
   junction, reproducing the A9 measurement.
3. **`Act::Delete` → `Act::Write` in `sys::unlink`** (Windows). 2 tests red: the "a REFUSED delete
   created directories inside the root" assertion, and "a refused DELETE must not be reported in the
   vocabulary of a write".
4. **`O_NOFOLLOW` dropped from `child_dir`** (real Linux). All 3 CPE-1937 tests red on their HARM
   assertions — `applied: 2, skipped: [], held_back: None` and `verdict=Ok(())`. This is what proves the
   Linux legs are *live* rather than silently skipping, which is the trap CPE-1913 fell into.

A fifth attempt — putting a by-path `remove_file` **after** the descent in the Unix `unlink` — did *not*
redden, and that is a finding rather than a miss: on Unix the descent's `O_NOFOLLOW` refuses at the
component, so the leaf is never reached. Containment lives in the descent; the leaf primitive is what
makes the descent's answer binding.

### Verification

- **Windows** `cargo test -p cpe-server --lib`: 2418 passed, 0 failed, 11 ignored.
- **Real Linux** (WSL Ubuntu, the `~/lintools` toolchain, `cargo 1.97.1`) `cargo test --lib`:
  2406 passed, 0 failed, 11 ignored. Not a cross-check — a real x86_64-unknown-linux-gnu build and run.
- `cargo clippy --all-targets -- -D warnings` clean in **all three feature modes** (plain, `index`,
  `specta`) on **both** platforms — six runs.
- The race harness's `std::fs::rename` calls carry `#[allow(clippy::disallowed_methods)]` with a reason:
  the raw rename **is** the attack under test, and both names are the test's own scratch tree.

### Deliberately not done

- **`copilot::apply_op` is still by-path.** It needs `renameat`, which this ticket did not add;
  `remove_file_beneath` is half of what it was waiting on. Not in scope, and CPE-1913's enumeration
  still names it correctly.
- **`tar`/`7z` extraction** remains by-path — unchanged by this ticket, and still named as such in
  `src/docs/safety-undo.md`.
- `safe_target` is **kept**: `checkpoint_store`'s per-file diff and `restore`'s pass-1 pre-flight still
  use it, and neither then touches the filesystem by that path. No `safe_target` caller in this crate
  resolves-then-acts any more.
