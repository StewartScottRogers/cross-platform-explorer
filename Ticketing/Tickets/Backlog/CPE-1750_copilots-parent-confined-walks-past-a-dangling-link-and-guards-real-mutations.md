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

## Work log — 2026-08-15, attempt 2 (PR #916 review: CHANGES REQUESTED)

The reviewer found that attempt 1 made **two operations more permissive**, in exactly the way this ticket
exists to prevent. Both reproduced against the head commit and confirmed refused on the base. They were
right, and the reason is a blind spot in my own attempt-1 analysis.

### The blind spot, stated plainly so the next reader does not inherit it

I compared `parent_confined` against `confined_to` across six input shapes and concluded "strictly
stricter or identical, never more permissive". Every row of that table was correct. The table was
**incomplete**: it never considered `path == root`.

`confined_to` returns **true** for the root itself — deliberately, and its own doc says so twice, once at
the `starts_with` line (*"it is true for `real_root` itself — the root is contained in itself, by
design"*) and once as an explicit hand-off (*"the caller still decides whether the root **itself** is an
acceptable answer … 'Not the root itself' is `same_place`'s question, **and the rename sites ask both**"*).
`apply_op` asked only one of the two. The `parent_confined` I deleted had answered the second by
**accident** — it inspected `path.parent()`, and the root's parent is outside the root by definition — so
deleting it removed a guard nobody had noticed was there. `op_plan::validate` is no backstop: its
`within_root` is a `>=`-length prefix test, true for equality.

The corrected row:

| input | `parent_confined` | attempt-1 `confined_to` alone | now |
|---|---|---|---|
| `path == root` | **false** (refused) | **true** (allowed — regression) | **false** (refused, by `same_place`) |

Lesson, and it is the same one CPE-1731 recorded: a guard's *accidental* properties are still properties
users depend on. Replacing a guard means enumerating what the old one refused, not only what the new one
refuses better.

### Blocker 1 — `Delete { path: <root> }` trashed the entire confirmed folder

Reached `trash.trash(root)` — `trash::delete` on the user's real filesystem in the shipped app — sending
the whole folder the human approved to the Recycle Bin, and reporting the op **successful**.

**Fix:** a second guard pass over every field, `crate::fsutil::same_place(value, canonical_root)`, with its
own message (`root_itself_refusal`) that does not claim the path is outside the folder — it is not; it *is*
the folder. Applied **uniformly to all five ops** rather than to a hand-picked "destructive" subset,
because "which arms need it" is precisely the judgement this repo keeps getting wrong. `Mkdir { <root> }`
was a no-op reported as success and is now an honest refusal; `Copy { src: <root> }` recursed the folder
into itself.

### Blocker 2 — `Rename { path: <root>, new_name: X }` relocated the folder to `<parent-of-root>/X`

My attempt-1 justification for leaving the rename destination unguarded — *"the slot is reached only after
`path` itself has been confined in full, which transitively confines its parent"* — is **false at
`path == root`**, and only there: the root's parent is outside the root. `rename_into_slot` guards *what is
sitting in* the slot, never *where the slot is*, so an empty name outside the folder went straight through.

**Fix:** `op_path_fields` now carries the rename's computed destination as a real field, so it goes through
the same containment pass as everything else. `rename_destination` is the single computation both the
guard and `rename_into_slot`'s call site use, so they cannot drift. The false doc claim at
`op_path_fields` is rewritten to say what is actually true and why the old reasoning broke.

### Why two passes, not one check per field

Containment runs as a **whole pass** before identity. `Rename { path: <root> }` trips both — identity on
`path`, containment on the computed `dst` — and the pass order decides which one reports. That is what
keeps each guard independently non-deletable: see the mutation matrix below.

### Mutation matrix — each guard's removal reds a DIFFERENT test

| mutation | `…never_trashes_the_confirmed_folder_itself` | `…never_renames_the_confirmed_folder_out_of_itself` | others |
|---|---|---|---|
| remove the `same_place` pass | **FAILED** (effect) | ok — caught by dst confinement | all ok |
| drop the rename `dst` field | ok — caught by identity | **FAILED** (reason) | all ok |
| both off (= attempt 1) | **FAILED** (effect) | **FAILED** (effect) | all ok |

```
--- remove the same_place pass
panicked at src\copilot.rs:1184:9:
the trash seam — `trash::delete` on the user's real filesystem in the shipped app — was handed
["…\cpe-copilot-cpe1750-root-delete-33856-4"], which IS the confirmed folder
"…\cpe-copilot-cpe1750-root-delete-33856-4" itself. The whole folder the human approved went to the
Recycle Bin, reported as a successful operation

--- drop the rename `dst` field
panicked at src\copilot.rs:1254:9:
and it must be refused BY THE DESTINATION-CONFINEMENT GUARD — if identity catches it first, that guard
is unreachable and deletable with every test still green: refused: path "…" IS the folder you confirmed…

--- both off (attempt 1, the reviewer's reproduction)
panicked at src\copilot.rs:1239:9:
the confirmed folder "…\cpe-copilot-cpe1750-root-rename-8996-5" was RELOCATED to
"…\cpe1750-relocated-root" — outside the folder, and outside what the pre-execute checkpoint took, so
the app's own Undo cannot bring it back
panicked at src\copilot.rs:1184:9:
the trash seam … was handed ["…\cpe-copilot-cpe1750-root-delete-8996-18"], which IS the confirmed
folder … itself.
```

Both effect assertions sit **above** their `let out = out.unwrap();` — these ops fail by *succeeding*, so
the call still returns `Ok` when they fire.

The rename test deliberately asserts the **reason**, not just the effect: with identity in place the
effect can never occur, so a reason-blind test would let the destination guard be deleted outright with
every test green. That is the same non-deletability mechanism as `assert_all_refused_for_escaping`.

A third test, `cpe_1750_root_identity_refusal_does_not_catch_ordinary_in_root_work`, pins the other
direction — an `Mkdir` two levels below the root and a `Rename` of a child must still run — so the
identity guard cannot degenerate into refusing everything near the root.

### Docs corrected, not patched over

`apply_op`'s claim and `src/docs/21-ai-copilot.md`'s promise both said the Copilot refuses anything landing
outside the confirmed folder. Blocker 2 falsified both. `apply_op` now documents **two** questions and why
`confined_to` alone cannot answer the second; the module header says the same; the user-facing page gains
a paragraph stating the folder itself is off limits and why.

### Verification (attempt 2)

`crates/server`: `cargo test` **2186 / 0**; `cargo test --features index` **2234 / 0**; `cargo test
--features copilot` (copilot module **19/19**); `cargo clippy --all-targets -- -D warnings` clean in the
default, `index` and `copilot` modes. `src-tauri`: `cargo test` **182 / 0**; `cargo clippy --all-targets
-- -D warnings` clean in the default and `sidecar-platform` modes.

---

### Verification (attempt 1 — superseded by the run above)

`crates/server`: `cargo build --tests`, `cargo test` (2183 passed / 0 failed), `cargo test --features
index` (2231 / 0), `cargo test --features copilot` (copilot module 16/16), and `cargo clippy --all-targets
-- -D warnings` clean in the default, `index` and `copilot` feature modes. `src-tauri`: `cargo clippy
--all-targets -- -D warnings` clean.

## Notes

Related: CPE-1730 (PR #909, which added `confined_to` and surfaced this), CPE-1709, CPE-1733.
`fsutil::contained_under` is a *third* primitive that deliberately fails **open** on a not-yet-existing path
— correct for its two removal-side callers, wrong for anything create-side. Do not reuse it here.
