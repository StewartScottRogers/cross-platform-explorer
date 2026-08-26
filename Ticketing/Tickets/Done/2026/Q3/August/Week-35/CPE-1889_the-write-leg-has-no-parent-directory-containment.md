---
id: CPE-1889
title: the write leg has no parent-directory containment, so a junction one level up writes outside the root and reports success
type: bug
priority: High
status: Done
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

- [x] A3 and A4 reproduced on today's code, then shown refused. Both pasted.
- [x] The refusal is reported per file, never a silent skip.
- [x] The write and delete legs now agree on containment.
- [x] Cost measured and stated.
- [x] Docs updated to match whatever is actually true afterwards.

## Work Log

- **2026-08-25 15:30 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  the Security Auditor's finding on PR #1022. Deliberately kept out of that PR: different fix, different
  code path. The auditor's `SEC PASS` was explicit that it meant the live holes are truthfully
  documented and ticketed, not that they are closed.
- **2026-08-26 (Worker)** — Fixed on `cpe-1889-write-parent-containment`.
  - **Mechanism.** `copy_one_verified` now asserts parent-directory containment via
    `fsutil::confined_to_resolved_root` before it writes. It does **not** reuse `contained_under` (the
    delete leg's check) and the ticket's step 1 was right to say "the way the delete leg already does"
    only about the *principle*: `contained_under`'s own doc forbids the reuse in as many words — it
    returns `Ok` for a target that will not canonicalise, which is sound for something about to be
    removed and fails open every single time for something about to be created. The crate's create/write
    answer is `confined_to`, which walks to the deepest existing ancestor, follows a dangling link by
    hand, and refuses everything it cannot resolve. `confined_to` was **extended, not forked** (its doc
    demands this): it is now a two-line wrapper over the new `confined_to_resolved_root`, which is the
    same walk with `canonicalize(root)` hoisted so the backup loop pays it once per run instead of once
    per file. Pinned by `confined_to_resolved_root_agrees_with_confined_to_on_every_shape`.
  - **The guard runs twice.** Check (1) before `create_dir_all` — otherwise a refusal still leaves
    *directory debris* outside the root, because `create_dir_all` walks a junction like any other
    directory. Check (2) after it, and only when this call actually created the parent.
  - **Reproduced live before the fix**, through the public `apply_backup_plan`, on Windows via
    `make_dir_link`'s privilege-free NTFS junction fallback. A3: `outside/authorized_keys` held
    `USER KEY`, ran a backup of `sub/authorized_keys` through a junction at `dst/sub`, and the outside
    file came back `ATTACKER PAYLOAD` with `ok: true`. A4: a plan entry named a file that did not exist
    outside, and the run *created* it there. A third leg proved the `create_dir_all` debris. All pasted
    in the PR body with the exact panic output.
  - **Red-proof, per guard.** Neutralising `parent_contained` reddens all four harm tests; deleting
    check (1) alone reddens all four too (an already-planted junction means the parent exists, so
    `create_dir_all` and check (2) are both skipped); sabotaging `confined_to_resolved_root` to `true`
    reddens the fsutil seam test. **Deleting check (2) alone reddens nothing**, and that is recorded on
    the function rather than papered over: check (2) can only ever differ from check (1) if the tree
    changes *between* them, which is a race no deterministic test can stage. Kept for the window
    `create_dir_all` opens (many `mkdir` round trips on a share), amortised once per new directory —
    explicitly not kept on a claim that it closes anything.
  - **TOCTOU: narrowed, not closed**, and said so. Between the last check and the destination `open`
    a parent component can still be swapped for a junction; closing that needs `openat2(RESOLVE_BENEATH)`
    or an `O_NOFOLLOW` walk, neither of which `std` offers. The *final* component remains atomic — the
    CPE-1879 guard reads the handle the bytes go through.
  - **Cost, measured — and the measurement's honest answer is "below the noise floor here".** New
    `#[ignore]`d A/B (`cpe_1889_measure_the_guard_cost`) runs the guarded engine against the pre-fix
    shape in one process over 2,000 files. Four local NTFS runs: **+11.3, −67.0, −21.2, +29.2 µs/file** —
    both signs, swamped by the copy's own variance. So the number quoted is the **syscall count**: the
    root is canonicalised once per run; per file the common case adds one `metadata` + one `canonicalize`
    (~200k extra path resolutions on a 100k-file backup), and a new directory adds the walk-up plus one
    confirming resolve. Flagged for re-measurement against the QNAP, where each resolution is a round
    trip and no longer hides behind the copy.
  - **Scope (item 4): swept, nothing knowingly left.** `archive::entry_sink_action`/`entry_dir_action`
    already resolve every intermediate component via `confined_to` *before* their `create_dir_all`
    (CPE-1744/1759); `transfer::download_tree` walks ancestors via `classify_ancestor_probe` before
    `create_dir_all` (CPE-1742); `revert_engine::apply_write` asks `confined_to` before both
    `create_dir_all` and the blob source (CPE-1750); `copilot::apply_op` asks it on every path field;
    `batch_media` resolves the computed `out_dir` rather than comparing text (CPE-1623). Backup was the
    last site of the create-then-write class. Recorded on `copy_one_verified`.
  - **Docs.** `copy_one_verified`'s "scoped to the final path component only" section is replaced by the
    containment section; `apply_backup_plan_walk`'s "known asymmetry, recorded rather than fixed" block
    now records that the asymmetry is closed and why the two legs deliberately ask *different* functions.
    `src/docs/safety-undo.md`'s "this only covers the file's own name — not a folder above it" bullet is
    replaced with what is now true, plus a plain-language bullet on the residual race for anyone whose
    backup destination is a shared network folder.
  - **Guardrails.** `cargo test` (crates/server): 2392 passed, 0 failed, 9 ignored. `cargo clippy
    --all-targets` with `-D warnings` in three modes — plain, `--features specta`, `--features index` —
    all clean. No new dependencies. No `specta::Type` struct touched, so no `bindings.gen.ts` regen.
