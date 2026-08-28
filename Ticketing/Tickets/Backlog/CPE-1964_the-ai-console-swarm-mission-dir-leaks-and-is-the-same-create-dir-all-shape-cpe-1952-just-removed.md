---
id: CPE-1964
title: the AI Console's `cpe-swarm-<millis>` mission directory leaks — 55 on one machine — and is the same predictable-`create_dir_all` shape CPE-1952 just removed
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1075's Reviewer while confirming CPE-1952's field evidence. CPE-1952 deferred this site as
a table row with a stated reason; the Reviewer's measurement says it deserves a ticket instead, and the
argument is hard to dispute.

**On this machine's real `%TEMP%`, right now:**

| directory shape | count | note |
|---|---|---|
| `cpe-catalog-stage-<pid>` | **9** | eight from 2026-07-14→19, **plus one from today at 18:29** with a real 155-byte `index.json` — the shipped app was still leaking as the fix was being reviewed. **Closed by CPE-1952** (PR #1075): nothing is written at all now, so no exit path can leak. |
| **`cpe-swarm-<millis>`** | **55** | **still leaking, ~6× harder than the site that got the ticket** |
| `cpe-ai-console-catalog` | 0 | fallback branch, never reached on this machine |
| `cpe-sidecar-storage` | 0 | same |

All are plain directories, no reparse points — nobody's repro left a hazard behind.

**Of the three residuals CPE-1952 deferred, this is the only one with live evidence, and it is not the
one that ticket foregrounds.** The two `temp_dir()` fallbacks it discussed at length show **zero**
on-disk instances; the one it listed in a table is the one filling the disk.

## Why it is the same defect, not merely similar

`cpe-swarm-<millis>` is a **predictable path in a shared namespace**, created with **`create_dir_all`**
— the exact primitive CPE-1952 established will follow a junction/symlink into an attacker-chosen
directory. `<millis>` is a timestamp, so it is guessable within a narrow window rather than random.

Two things make it *worse* than the catalog case in one respect and better in another:

- **Worse:** it leaks constantly (55 vs 9), so an attacker watching `%TEMP%` has abundant signal about
  when and how the app creates these, and the leaked names publish the timestamp pattern outright.
- **Better:** the content is mission scaffolding rather than bytes off the wire, so the escape
  primitive is weaker than the pre-fix catalog bug's *"unverified download written to a location the
  attacker chose."*

**Threat-model caveat, from the same Reviewer (F2 on #1075) — do not over-inherit CPE-1952's framing.**
On **Windows**, `std::env::temp_dir()` resolves to the **per-user** `%LOCALAPPDATA%\Temp`, not a
machine-shared namespace, so the Windows attack needs a same-user process. *"Predictable path in a
shared namespace"* is fully true of **Unix `/tmp`**. Both halves are real; state them separately.

## Acceptance criteria

- [ ] **Reproduce the escape before fixing**, on both platforms, and **assert on the filesystem** —
      where the bytes actually landed — never on a returned verdict. Plant a junction (Windows,
      `junction::create`, no admin needed) / symlink (Unix, on a **real ext4** path, not `/mnt/z`).
- [ ] **Keep a sensitivity control**: with the fix disabled the escape must happen, as a normal CI test
      on all three OSes — **not `#[ignore]`d**. PR #1075's
      `the_old_staging_primitive_writes_through_a_planted_link` is the model. Note its Reviewer's F3:
      a `Scene::planted()` that silently returns is a **green** test, because `eprintln!` is captured
      by default — `panic!` on the platforms where the link must work.
- [ ] **Plant at the REAL path**, not a stand-in inside a `tempfile::tempdir()`. A stand-in is
      unreachable by any regression and every assertion about it is unfalsifiable (CPE-1929).
- [ ] **Fix the leak as well as the escape.** These are two defects sharing a site: the directory is
      created in a place an attacker can pre-empt, *and* it is never cleaned up on the error paths.
      **Prefer the shape CPE-1952 chose** — if the mission scaffolding need not exist on disk, deleting
      the directory beats defending it, and it closes both defects at once. If it genuinely must exist,
      say why, and then the leak needs its own answer (RAII guard, or a sweep with a stated retention).
- [ ] **Decide what to do with the 55 existing directories.** They are on a real user's machine. A
      startup sweep is the obvious answer and is also a new destructive operation over a shared
      namespace — argue it, and make it refuse anything that is not plainly ours.
- [ ] **Re-derive the `temp_dir()` enumeration** and re-check the two fallbacks CPE-1952 left
      (`catalog_dir`'s and sidecar-storage's). Use the **corrected** recipe: `git ls-files '*.rs'`,
      minus `tests/`, minus everything after each file's first **column-0** `#[cfg(test)]`. PR #1075's
      stated recipe said "first `#[cfg(test)]`", which matches indented in-function attributes and doc
      comments and amputates production code — run literally it yields **10** sites instead of **15**,
      dropping **both swarm sites**. A derivation nobody else can re-run is halfway back to recall
      (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1075's Reviewer (**APPROVE**, F6), which measured the
counts rather than accepting the deferral. It made the case plainly: *"the residual this ticket
deferred is leaking roughly six times harder than the one it fixed… it should not sit as a table row."*

Related: **CPE-1952** (the catalog staging fix, PR #1075 — the model for the fix shape and the test
shape), **CPE-1937** / **CPE-1929** (the containment and shadowed-guard families), **CPE-1932**
(enumerate, don't recall — and the corrected recipe above).

## ID note 2026-08-27

Filed as CPE-1963 and renumbered to **CPE-1964** within the hour: PR #1070's round-2 worker filed its
own **CPE-1963** (the staging rename's source being an enumerable attacker-writable path) at almost
the same moment, from a worktree that could not see this one. Theirs is referenced from `fsutil.rs`
comments, its PR body and CPE-1961; this one was referenced only by itself, so this is the cheaper
side to move. Standing hazard: two agents allocating the next free ID from different checkouts will
collide, and the tell is that neither can see the other's file.
