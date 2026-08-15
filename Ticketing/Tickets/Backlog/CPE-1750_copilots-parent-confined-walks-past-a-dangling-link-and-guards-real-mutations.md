---
id: CPE-1750
title: The Copilot's parent_confined walks past a dangling link, and it guards real production mutations
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-14
closed:
---

## Problem

Found and reproduced by the PR #909 (CPE-1730) reviewer, 2026-08-14, while checking whether the repo now had
two containment primitives. It has **three** — and the weakest of them is the only one guarding **real
production mutations**.

`crates/server/src/copilot.rs:178` `parent_confined` guards the AI Copilot's actual `fs::rename`, `fs::copy`,
`create_dir_all` and `trash::delete`. Its doc at `copilot.rs:198-200` claims:

> no mutation ever lands outside the confirmed folder.

That claim is false. Replicated verbatim and run against the new `cpe_server::fsutil::confined_to` on
identical inputs:

```
dangling link root/dangling -> <outside>/not-created-yet
  parent_confined(root, root/dangling/x.txt) = true      confined_to = false
  parent_confined(root, root/dangling)       = true      confined_to = false
live link root/live -> <outside>
  parent_confined(root, root/live)           = true      confined_to = false
```

Two defects, either sufficient on its own:

1. Its `Err(_) => cur = dir.parent()` walks **past a dangling link** — the exact trap `confined_to`'s doc
   names and fails closed on.
2. It never inspects the **final component at all**, so a link at the leaf is invisible to it.

Consequence: `FileOp::Copy { dst: root/dangling }` reaches `fs::copy`, which **follows the link** and creates
its target outside the confirmed folder — and outside the undo checkpoint, so the operation is also not
reversible by the app's own undo.

## Why this is High

This is not a test rig. CPE-1730 confined the FTP/SFTP/WebDAV **test rigs**; this is the shipped Copilot
acting on a real user's filesystem, with a doc comment asserting the property it does not have. The
confirmed-folder boundary is the Copilot's entire safety story.

## The fix

`cpe_server::fsutil::confined_to` (added by CPE-1730, `fsutil.rs:1043`) already answers this question
correctly and was adversarially probed across 26 cases (nested links, relative links, junctions, drive
prefixes, drive-relative paths, extended-length and UNC paths, trailing dots, embedded NUL, 200- and
5000-deep missing tails, mutually-referential dangling links, and a sibling whose name is a string prefix of
the root). Route `parent_confined` through it rather than re-deriving the walk, and cross-reference it from
`confined_to` so the next reader finds one answer instead of three.

**Also**: `copilot.rs:760` has a private `make_dir_link` that duplicates the new `fsutil::make_dir_link` —
same crate, two files apart — which is precisely the duplication `fsutil::make_dir_link`'s own doc says it
exists to prevent. Fold it.

## Acceptance criteria

- [x] A Copilot `Copy`/`Move`/`Delete`/`mkdir` whose destination is a **dangling** link inside the confirmed
      folder is refused, and nothing is created outside that folder.
- [x] The same for a **live** link at the leaf, and for a link at an intermediate component.
- [x] Breaking the guard turns a **distinct** test red, and the assertion names what landed outside the
      confirmed folder — asserted **before** the `Result` is unwrapped, since this defect fails by
      succeeding.
- [x] `copilot.rs:198-200`'s claim is either true or rewritten to what actually holds.
- [x] `copilot.rs:760`'s private `make_dir_link` is removed in favour of `fsutil::make_dir_link`.
- [x] TOCTOU's residual is stated where a reader of the Copilot path will hit it, the way `confined_to`
      states its own.

## Work log — 2026-08-15

### What changed

`crates/server/src/copilot.rs`

- **`parent_confined` is deleted, not fixed.** `apply_op` now calls `crate::fsutil::confined_to` on each
  path field. A block comment stands where the function was, recording the two measured defects and why a
  fifth copy of the walk must not be written. Containment has one answer in this crate again.
- **`confinement_refusal` (new, small).** `confined_to` returns one bit, and its "no" covers two truths:
  *resolves outside*, and *the OS would not say where it resolves* (it fails closed on `EACCES`/`ELOOP`).
  Reporting the first when the truth is the second is the confident-false-statement failure of CPE-1687 /
  1705 / 1710 / 1716. So a refusal asks `try_exists` — the same probe `clobber_refusal` classifies, nothing
  re-derived — purely to pick the wording, and borrows `fsutil::unknown_slot_message` for the uncertain
  case so it is phrased identically to every other site.
- **The false claim at the old `:198-200` is rewritten**, and `apply_op` now carries a *What this does NOT
  cover* section: the TOCTOU residual in full (and that this is not a boundary against a local adversary
  racing the app), that containment says nothing about what a primitive does to a link resolving *inside*
  the root, and the pointer to `execute_with`'s breadth note. The module header records the same.
- **The private test `make_dir_link` is gone**; both link-staging tests now call `fsutil::make_dir_link`,
  which brings the CPE-1717 `require_staged` policy with it — a machine that cannot stage a link now goes
  red under CI instead of quietly covering nothing.

`crates/server/src/fsutil.rs` — **doc only, no behaviour changed.** `confined_to` now names the Copilot as
a caller, records the `parent_confined` measurement as the reason not to fork it, and its TOCTOU bullet no
longer claims "the callers are single-threaded in-process test rigs" — that stopped being true the moment
the shipped app started calling it.

`src/docs/21-ai-copilot.md` — a plain-language *Links and shortcuts that lead out of the folder* section:
why an operation can be refused although the path you see starts inside your folder.

### Evidence — the mutation, run both ways

`apply_op`'s call was temporarily replaced with the deleted `parent_confined`, verbatim. Two tests went
red, and **only** those two — every other copilot test, including the pre-existing intermediate-live-link
one, stayed green, so the red is distinct and belongs to this ticket:

```
cpe_1750_execute_refuses_an_op_whose_own_final_component_links_out_of_the_root ... FAILED
  the link at "…\cpe-copilot-cpe1750-leaf-…\outlink" was DESTROYED — the Copilot's Delete reached the
  trash seam with a path that resolves to "…\cpe-copilot-cpe1750-leaf-outside-…", outside the confirmed
  folder "…\cpe-copilot-cpe1750-leaf-…", because the guard never looked at the op path's own final
  component

cpe_1750_execute_refuses_ops_under_a_dangling_link_pointing_out_of_the_root ... FAILED
  the trash seam was handed ["…\cpe-copilot-cpe1750-dangling-…\dangling"], which resolves to
  "…\cpe-copilot-cpe1750-dangling-outside-…\soon", outside the confirmed folder
  "…\cpe-copilot-cpe1750-dangling-…"
```

Both panics are on assertions placed **before** the `Result` is unwrapped — this defect fails by
succeeding, so anything after an `unwrap()` would never have run — and both name what left the confirmed
folder. Restored: 16/16 copilot tests green.

The guard is also non-deletable in the weaker sense: `assert_all_refused_for_escaping` insists the refusal
carries the **containment reason**, not merely that the op failed. Several of these inputs also happen to
make `create_dir_all` fail with `EEXIST`/`ENOENT` on POSIX, so a test asserting only "it failed" would
have stayed green with the guard removed — the exact shape that shipped a deletable guard earlier this
sprint. The accidental POSIX stop is not a guard: `create_dir_all`'s behaviour on a dangling reparse point
differs between Windows and POSIX.

`cpe_1750_…_final_component_…` also carries a discrimination leg: a leaf link resolving back *inside* the
root must still run. This is containment, not a ban on links.

### One interaction worth knowing about

Containment is now asked **before** the slot guards, which is the right order — a path that might resolve
outside must not be probed, stat'd or written at all. Consequence, caught by CPE-1705's own copilot leg
going red on the first run: a destination the OS refuses to `stat` used to reach `rename_slot_refusal` and
get its *"could not check what is at …"* wording from there. It now stops at the containment guard, which
fails closed on `EACCES`. `confinement_refusal` borrows the identical shared wording, so the user sees no
change and CPE-1705's property still holds at this seam; that test keeps its original assertion and gains
one more — an unreadable in-root destination must **not** be reported as a containment escape. Recorded on
both the test and `apply_op`.

### Verification

`crates/server`: `cargo build --tests`, `cargo test` (2183 passed / 0 failed), `cargo test --features
index` (2231 / 0), `cargo test --features copilot` (copilot module 16/16), and `cargo clippy --all-targets
-- -D warnings` clean in the default, `index` and `copilot` feature modes. `src-tauri`: `cargo clippy
--all-targets -- -D warnings` clean.

## Notes

Related: CPE-1730 (PR #909, which added `confined_to` and surfaced this), CPE-1709, CPE-1733.
`fsutil::contained_under` is a *third* primitive that deliberately fails **open** on a not-yet-existing path
— correct for its two removal-side callers, wrong for anything create-side. Do not reuse it here.
