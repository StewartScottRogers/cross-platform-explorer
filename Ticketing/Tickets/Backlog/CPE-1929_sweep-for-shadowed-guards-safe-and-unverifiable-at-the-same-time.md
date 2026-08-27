---
id: CPE-1929
title: sweep for **shadowed guards** — a check that is simultaneously safe and unverifiable, because an earlier check answers on the same fact
type: task
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## The pattern

Discovered on CPE-1896 / PR #1043, generalised past that instance:

> A guard cannot be given test coverage while an earlier guard answers on the same underlying fact.
> No fixture can make the later one the decider, because every input that would trip it trips the
> earlier one first. The later guard is then simultaneously **safe** (nothing gets through) and
> **unverifiable** (nothing can prove it works) — and those two properties are easy to mistake for
> each other.

**The tell, and this is the part worth carrying:** a sabotage that leaves the suite green **and** a
fault-injection that changes no behaviour, **on the same guard**. Separately each reads as evidence
of safety. Together they mean the guard is **unreachable**, and the next question is *which earlier
check is shadowing it*.

## How it presented on CPE-1896

Three symptoms, and only the third looked like a problem at the time:

1. The Reviewer disabled the leaf surrogate refusal entirely — **2,404 tests stayed green.**
2. The Security Auditor forced the shared predicate to a lying `Some(false)` — **nothing got through
   anyway.**
3. The fixture the Foreman specified to fix the test came out **unbuildable**: it went red on the
   *other* guard.

Cause: `std::fs::FileType::is_symlink` on Windows tracks the **same name-surrogate bit** the new tag
check reads, so the `symlink_metadata(dst)` path check standing in front of it caught every surrogate
first. Resolved by reordering — handle check before path check, which is the direction CPE-1896
argues for throughout.

## The lead this ticket exists to chase

`batch_media::open_output_verified` has **the same shape**: a path check and a handle check both
answering about links at one name. CPE-1896's worker named it and was explicit that it is **a lead,
not a finding** — the two-sabotage check has *not* been run against it, and no claim is made that it
is affected.

## Acceptance criteria

- [ ] Run the two-sabotage check against `batch_media::open_output_verified`: disable the later guard
      and see whether the suite stays green; separately force its predicate to lie and see whether
      behaviour changes. Both green means shadowed.
- [ ] Sweep `crates/server` for the same shape more generally — any site where a **path** question and
      a **handle** question answer about the same property of the same name. `fsutil`, `batch_media`,
      `backup`, `revert_engine`, `archive` and `transfer` all carry link/reparse guards worth checking.
- [ ] For each shadowed guard found, decide **reorder vs delete**. Reordering is right when the later
      guard asks the more trustworthy question (a handle cannot be substituted after the open; a path
      can) — that is what CPE-1896 did. Deleting is right when the later guard is genuinely redundant.
      Leaving it shadowed is the one wrong answer, because it reads as coverage.
- [ ] Where a guard is kept **deliberately** as an unreachable backstop, say so at the site **and** say
      that it is untestable and why — so the next person's sabotage returning green is expected rather
      than alarming.
- [ ] Consider whether the two-sabotage check can be mechanised at all, even partially. Probably not
      worth full automation, but a short note in the repo's testing guidance costs nothing, and this is
      now a named pattern.

## Notes

Filed 2026-08-27 by the sprint Foreman. Origin: CPE-1896's worker, after a Foreman-specified fixture
failed to build and the worker diagnosed *why* rather than working around it.

Related: **CPE-1896** (where it was found), **CPE-1927** (a different flavour of test that does not
prove what it appears to).

## Second named lead, added 2026-08-27 (from CPE-1931's sweep)

CPE-1931's worker ran a research-only sweep across every guard/ratchet test in `src/`, `gui-smoke/`
and the Rust guards in `crates/updater-verify` and `crates/server`, looking for the same shape it had
just fixed. Result: **no other guard shares the risky hex/numeric-over-whole-file shape.** One
lower-risk relative worth checking here:

**`src/lib/lockfileLockedGuard.test.ts`** regexes **raw `.yml` text** for cargo subcommands and strips
only **whole-line** `#` comments, not trailing ones. Same raw-text-rather-than-syntactic-position
fragility as the pre-CPE-1787 apt-get guards. It is **not** a hex/ticket-number collision risk — it
matches literal cargo subcommand words — but its siblings (`ciAptGetHardening`, `releaseHangHardening`
and others) have already migrated to `parseYaml`, and this one has not.

A trailing `# cargo build --locked` in a comment would therefore count as a real invocation. Worth
the two-sabotage check and, if confirmed, the same `parseYaml` migration its siblings already had.
