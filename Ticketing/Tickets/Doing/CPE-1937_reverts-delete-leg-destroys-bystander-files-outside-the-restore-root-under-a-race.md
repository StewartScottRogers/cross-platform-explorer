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

**RETRACTED — see round 2 below.** A fifth attempt (a by-path `remove_file` after the Unix descent)
did not redden the *static* suite, and round 1 wrote that up as a finding: "containment lives in the
descent". That is **false**. The race harness this PR had just written catches it — 89 destroyed
bystanders in 200 trials on my own re-run, 94 and 141 independently — because the leaf is reached on
every delete that succeeds. Round 2 corrects the claim everywhere it appeared and adds a non-ignored
test so CI can see the leaf at all.

### Verification

- **Windows** `cargo test -p cpe-server --lib`: 2419 passed, 0 failed, 11 ignored.
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

## Work Log — round 2, 2026-08-27 (review + security audit)

PR #1059's Reviewer and Security Auditor both cleared the containment — neither could break it — and
both found that the **narrative** around it was wrong in the same place. This round is corrections,
plus two real Windows defects the auditor found in the new primitive.

### Correction 1 — the "fifth sabotage does not redden" claim was FALSE. Retracted.

Round 1's Work Log said a by-path `remove_file` placed after the Unix descent stays green because
`O_NOFOLLOW` refuses at the component, so the leaf is never reached, and offered that as a *finding*.
It is wrong, and it was reached by running the static suite and not the `#[ignore]`d race harness this
same PR had just written. Reproduced here, on real Linux, descent completely intact:

```text
unix leaf                                    trials  FILES_DELETED_OUTSIDE  swaps
unlinkat (this module)                         200            0            7373/7373
fs::remove_file, after the same descent        200           89            7742/7742
```

The Reviewer measured 94/200 and the Security Auditor 141/200 and 1393/2000 independently. Comparable
denominators in every case, so it is signal, and the spread is what a race looks like.

**Why the reasoning was wrong:** the descent refuses only a component that *is* a link at that
instant. On every delete that is going to succeed, the descent hands back a handle and the leaf runs —
so the leaf is reached constantly, and a by-path leaf re-resolves the whole path from the root, where
a concurrent rename redirects it. `O_NOFOLLOW` makes the leaf unreachable only for a hostile name in a
*static* fixture.

This is good news for the code — `remove_file_beneath` is load-bearing and red-proofable — and bad
news for the record, so the claim is corrected in all three places it appeared:
`remove_file_beneath`'s doc, both `sys::unlink` docs (each now carries an explicit "do not replace
this with a by-path call, here is the measurement" note), and this log. No CPE-1929
untestable-backstop annotation was added: it does not apply, and adding one would cement the error.

### Correction 1b — and the leaf had ZERO CI coverage. Now it does not.

The auditor ran the **entire non-ignored suite** with that sabotage and got **2406 passed / 0 failed**.
So the only red-proof for a piece of live containment was a harness nobody runs by default — and the
next ticket reuses this module for `copilot::apply_op`.

Closed with `open_beneath::tests::cpe_1937_the_leaf_and_not_only_the_descent_contains_the_delete`, a
**non-ignored** test. A `#[cfg(test)]` seam (`BETWEEN_DESCENT_AND_LEAF`, compiled out of a shipped
binary exactly like `WALK_SYSCALLS`) fires once between the descent and the leaf — precisely where the
auditor measured the swap landing — and does deterministically what the racer was trying to hit by
luck: renames the real directory aside and leaves a link with the same name. A handle-relative leaf
deletes the in-tree file; a by-path leaf destroys the bystander outside the root and returns `Ok`.

Red-proofed on **both** platforms with a by-path leaf, and it fails on the harm assertion with the
silent-success signature: `HARM: the leaf re-resolved the path after the descent and deleted a
bystander outside the root; verdict was Ok(())`. The Windows swap stages successfully (the held handle
carries `FILE_SHARE_DELETE`), so this covers Windows too rather than skipping there.

### Correction 2 — `apply_delete`'s permanent/transient branch was computed and discarded. Removed.

`Refused::permanent` is read at exactly one place — `any_permanent_refusal` in the **write** loop,
consumed to pick the hold-back wording *before* any delete runs. The delete loop does
`report.skipped.push((path, refused.reason))`: it moves `reason` and drops `permanent`. Confirmed by
replacing the branch with an unconditional `transient` and getting an unchanged Windows suite.

So round 1's claim that a planted junction is reported permanent rather than "run the revert again"
had **no observable output**, and the CPE-579 doc asserted that distinction to users. That is the
CPE-1929 shape this ticket set out not to create, so the branch is **deleted rather than half-wired**:
`apply_delete` now returns `Result<(), String>` — the one thing its caller consumes — and its doc
records what is lost and what wiring it would actually cost (`held_back` is the only structure
carrying retryability, and a path in both `held_back.paths` and `skipped` is emitted **twice** by
`RevertOutcome::from_report`; the honest fix is a `DeleteRefusalGroup` beside `write_refusal`, which
is a new wire type + bindings + frontend). `open_beneath::Refusal::policy` still carries the answer,
one field away, for whoever does that. `src/docs/safety-undo.md` now says plainly that the delete half
does not make this distinction.

### Correction 3 — pass 1's justification moved to where a reader lands

`resolve_entry`'s own doc still said it yields "the `(target, blob)` pair pass 2 will use". Pass 2 has
not used it since this ticket — it recomputes from `safe_segments` + `blob_source` — and pass 1
discards the `Ok` entirely. Its doc now says so, and carries the pre-flight-not-shadowed-guard
reasoning that was only at pass 2's call site.

### F2 (new, Windows) — a revert could no longer delete a read-only file. Fixed.

The auditor found the first cut's plain `FileDispositionInfo` **fails** on `FILE_ATTRIBUTE_READONLY`:
`remove_file_beneath -> Err(policy=false), file survives` where `std::fs::remove_file -> Ok` on the
identical fixture. `main` used `fs::remove_file`, so this was a straight regression — and it was
classified `policy: false`, which round 1 turned into "try again", advice that could never work.

**Decision: match `std`, do not refuse.** The plan named that file and the user asked for the revert;
refusing would be a behaviour change this ticket has no mandate for. The leaf now uses
`FileDispositionInfoEx` with `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE`, which is what `std`
itself does. The pre-1709 fallback clears the attribute **on the open handle** — never by path, which
would have reintroduced this ticket's own defect inside its fix — and puts it back if the delete still
fails, so a refused deletion never leaves a silent attribute edit behind. `FILE_WRITE_ATTRIBUTES` is
requested *optionally*, falling back to the minimal access set, so a file whose ACL grants DELETE but
not attribute-write is still deletable. Covered by
`cpe_1937_a_read_only_file_is_still_deleted_as_std_would`, which runs on every platform.

### F3 (Windows) — `applied` could be reported while the name was still in the folder. Fixed.

Plain `FileDispositionInfo` is delete-on-close: with another handle open sharing delete, it returns
success and the name stays until the last close. That is the report-vs-filesystem divergence family
this ticket exists to close, arriving through the fix for it, so it is not left as a documented quirk.
The same `FileDispositionInfoEx` call carries `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS`, which unlinks
immediately — matching `unlinkat` on the Unix side and `std` on this one. Covered by
`cpe_1937_a_reported_delete_has_left_the_directory_immediately`, written once for both platforms.

### F4 — the stale residual paragraph in `restore` pass 2

Forty lines above the line this ticket changed, the comment still said *"The interior-component race
is NOT closed and is still the recorded residual … the open below is by path too … closing that needs
`openat`-relative resolution, which `std` does not expose."* Every sentence had stopped being true.
Corrected, and the correction says why it matters: this is the exact doc/code mismatch that produced
CPE-1937 in the first place, and a stale residual note is read as a live one.

### F5 — numbers, and a denominator that was too weak

- `src/docs/safety-undo.md` no longer quotes PR #1050's "596 across 200 attempts", which came from a
  differently-configured harness. It now gives all three measurements with their configurations —
  this harness 106 (Win) / 75 (Linux), the audit's run of the same harness 122 / 59, and the audit's
  by-path-leaf variant 141 (Linux) — and says the spread is what a race looks like.
- The race harness gained **`DELETES_APPLIED`**. `applied_total > 0` was satisfiable by the plan's
  single *write*, so a run in which the racer refused every deletion would have reported a proud zero.
  The new counter is asserted `> 0` and reads: Windows 204, Linux 1777.

### Round-2 verification

- **Windows** `cargo test -p cpe-server --lib`: **2422 passed, 0 failed, 11 ignored**.
- **Real Linux** (WSL Ubuntu, `~/lintools`, cargo 1.97.1): **2409 passed, 0 failed, 11 ignored**.
- Race, after: Windows `trials=200 FILES_DELETED_OUTSIDE=0 DELETES_APPLIED=204 swaps=224/232`;
  Linux `trials=200 FILES_DELETED_OUTSIDE=0 DELETES_APPLIED=1777 swaps=7442/7442`.
- `cargo clippy --all-targets -- -D warnings` clean in plain / `index` / `specta` on both platforms.

### Round-1 corrections to the log below

- Windows suite was **2419**, not 2418, at the end of round 1 (2422 now, with three new tests).
- "four sabotages, all on HARM assertions" was imprecise. Sabotages 1, 2 and 4 redden on harm
  assertions; sabotage 3's two reds are a directory-debris assertion and a *wording* assertion, and
  sabotage 4's `open_beneath` red fires at an `expect_err` — a verdict assertion — before reaching its
  harm check. The round-2 leaf sabotage reddens on harm on both platforms.
