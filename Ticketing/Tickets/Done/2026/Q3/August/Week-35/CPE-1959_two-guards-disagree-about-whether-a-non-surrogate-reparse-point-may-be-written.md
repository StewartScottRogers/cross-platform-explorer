---
id: CPE-1959
title: two guards disagree about whether a non-surrogate reparse point may be written — `fsutil` writes it, `batch_media` refuses it, and only the refusal is now pinned by a test
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

After PR #1066 (CPE-1929) the codebase states **opposite doctrines** about the same input class:

- **`fsutil::overwrite_confirmed_no_follow` writes** a non-surrogate reparse point. That is CPE-1896's
  rule: a dehydrated cloud placeholder (OneDrive, dedup, WOF) is an ordinary file the user expects to
  be written, and refusing it was the regression CPE-1896 removed.
- **`batch_media::open_output_verified` refuses** it, on the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit —
  and PR #1066's new test now **cements** that verdict.

**This is not a regression.** `batch_media` refused non-surrogates before CPE-1929 too; the reorder
changed which check says so, not the verdict. Both PR #1066's Reviewer and its Security Auditor
confirmed that independently, and the split is documented at the `batch_media` site as **deliberately
unresolved**.

## Why it needs a ticket of its own

PR #1066's site comment points at **CPE-1958** as the place to revisit this. CPE-1958 is scoped to the
`links > 1` TOCTOU race at a *neighbouring* guard — a different problem. Its Reviewer flagged the
mismatch:

> Everything else this ticket deferred got a ticket that *owns* it, with acceptance criteria
> (CPE-1957). The doctrine question gets a pointer at a ticket about something else, so a worker
> arriving via CPE-1958 is working the race and may never read this.

So this ticket exists to own the question.

## What the site comment already establishes, and what it does not

**Established:** the split is real, both halves are named, it is non-regressive, and there is one
substantive asymmetry — *a refused batch item is **skipped** and the user keeps their input, whereas a
refused restore has failed at its only job.* That argument is correctly labelled an argument.

**Not established, and this is the crux, in the comment's own words:** nobody has asked a user whose
batch output landed on a cloud placeholder what they expected to happen.

## Acceptance criteria

- [x] **Decide which doctrine is right for `batch_media`, and record the reasoning**, not just the
      outcome. The asymmetry above is the starting argument; it is not obviously decisive.
- [x] **Get the missing evidence, or state plainly that it was not gettable.** What does a user with a
      dehydrated placeholder at a batch-media output path actually experience today — a skipped item
      with a link-shaped message they cannot act on? That is answerable from the code and the message
      text without asking anyone, and it is the input the comment says is missing.
- [x] If `batch_media` should match `fsutil`, narrow it to `reparse_name_surrogate` **and update the
      test PR #1066 added**, which currently pins the opposite. Do not weaken the test into vagueness;
      replace it with one that pins the new verdict just as hard.
- [x] ~~If the split is right, say so at both sites~~ — the split is **not** right; it is closed. The
      "unresolved" framing is deleted at both `fsutil` sites and replaced with "settled, and here is
      where".
- [x] Either way, **fix the follow-up hook**: the `batch_media` comment should point here, not at
      CPE-1958.
- [x] Check whether any **third** site takes a position on this class. PR #1066 enumerated all seven
      `handle_facts` call sites; use that enumeration rather than recalling (CPE-1932).

## Work log — 2026-08-28

**Verdict: `batch_media` was wrong, and it now calls `reparse_name_surrogate` like every other write
path in the crate.** The bare-bit refusal is gone. The reasoning is at the site; the three headline
inputs:

1. **The mechanism the refusal defended stopped existing at CPE-1961.** Its stated property was "a batch
   never writes *through* one". `VerifiedOutput::write_all` hands the handle to
   `fsutil::stage_bytes_over_checked_handle`, whose own doc says it is "the same three steps
   `overwrite_confirmed_no_follow` performs after its own checks, in the same order, for the same
   reasons; only the checks in front differ". A placeholder is **replaced by name**, so CPE-1896's three
   unresolved worries about writing through a `FILE_OPEN_REPARSE_POINT` handle are unreachable here.
   This ticket removed the last difference in the checks in front.
2. **The asymmetry argument does not survive being measured.** Derived end to end rather than assumed:
   `execute_one` → `open_output_verified` → `Err(…)` → `execute_plan_walk` pushes `(input, reason)` into
   `BatchReport::skipped` → `App.svelte` renders `notice.convertedWithSkipped` =
   `"{written} converted, {failed} skipped: first \"{name}\" — {reason}"`, which shows **`skipped[0]`
   only**. So a user whose synced folder's outputs OneDrive has dehydrated — which on a re-run is *every*
   previous output — got a count and exactly one sentence, and until today that sentence was:

   > refusing at write time: "…\out.png" is a link or other reparse point (a symlink, a junction, or any
   > name that can stand in for another), and a batch never writes through one — such a name's target can
   > be re-pointed after any check, even a dangling link that happens to point back inside this same
   > folder. Nothing was written for this file

   For a placeholder every noun in that is false and no action is named. The cost of over-refusing was
   never "one item"; it was systematic, and invisible past the first row.
3. **CPE-1957's unprivileged-plantable finding cuts *toward* narrowing.** Anyone who can plant a
   non-surrogate tag can equally plant a surrogate one, which both doctrines refuse. All the bare bit
   denied was a tag that by definition does not redirect the name, on a name about to be replaced by
   rename inside an already-contained folder — i.e. it was an attacker-**triggerable** refusal (a
   denial-of-service on the batch item), not an attacker-blocking one.

**Per-site verdicts, derived at run time (CPE-1932), not from PR #1066's "seven".** Enumerating
`handle_facts(` in `crates/`+`src-tauri/`+`sidecar/` and splitting at each file's `mod tests` gives
**nine production call sites**, not seven — `fsutil`'s staged and reopened identity reads (CPE-1961 and
CPE-1963) and `vault_manager::overwrite_pinned_file`'s post-date that count. Of the nine, exactly **four**
consult `is_reparse_point` at all (derived by grepping the field, same split).

**Deliberately no line numbers below.** Round 3 carried five and every one had drifted by the end of the
same PR — a second, unguarded copy of a fact the source already states, which is the defect this ticket
is about. Function names do not drift:

| site | position on this class | verdict |
|---|---|---|
| `batch_media::open_output_verified` | was bare bit → **now `reparse_name_surrogate`** | **changed here** |
| `fsutil::claim_destination_handle` (the guard lives here; reached from `copy_file_onto_destination_handle`) | narrow since CPE-1896 | fine; note added that the split is closed |
| `fsutil::overwrite_confirmed_no_follow` | narrow since CPE-1929 | fine; **its docblock claimed the opposite** — fixed |
| `vault_manager::overwrite_pinned_file` | **narrow** — CPE-1957 / PR #1101 merged and narrowed it at **both** its checks (the by-path `probe.is_link` in `shred_dir_pinned` and the handle check) | not touched — that PR owns it |
| `backup::landed_inside`, and `fsutil`'s four identity-only reads (the claimed-destination, root-handle, staged and reopened ones) | use `.id`/`links` only | no position; nothing to do |

A fifth consumer of the rule, `open_beneath::sys::name_surrogate_at`, is already narrow (CPE-1938).

**The `vault_manager` row was order-dependent, and the order has now resolved.** PR #1101 merged
(`5a207fd5`) and this branch is rebased on top of it, so with this PR the crate has **no site left
reading the bare bit**. That sentence is written here, once, rather than at any of the sites: the site
comments each say what is true of their own function unconditionally, so none of them goes stale on a
merge, and the crate-wide count lives in this table (CPE-1932).

**A naming correction from the review:** the narrow guard reached from `copy_file_onto_destination_handle`
lives in `claim_destination_handle`. Round 1 named the caller, so a reader grepping the named function
would have found no `is_reparse_point` at all.

**A false provenance claim found and fixed (CPE-1933).** `overwrite_confirmed_no_follow`'s docblock said
*"This site refuses a reparse point on the **bare bit**"*. It has narrowed since CPE-1929 and a green
test pins that it does. Prose about a check thirty lines away, with the suite reading as though it
vouched for it.

**Sabotages — three, all red, Windows 11 / NTFS, `cargo test -p cpe-server --lib`.** Baseline
**2,461 passed / 0 failed / 14 ignored**; each sabotage **2,460 / 1**, the failure being the new test
every time. Disable; predicate forced to lie; un-narrow to the bare bit. This ticket replaces one test
with one test, so it moves the count by zero — the figures went 2,460 → 2,461 because **CPE-1957
(PR #1101) merged and this branch was rebased onto it**, and they were re-taken rather than left stale
by that merge.

**`TMP` must be on the platform default to reproduce those numbers** — and round 1 of this log got the
reason wrong, which mattered. It reported 2,458 / 2 and attributed the two failures to "running from a
worktree nested inside the repo". Re-measured: the same nested worktree with a normal `TMP` gives a
clean run (2,460 / 0 at the time, 2,461 / 0 after #1101). The cause is `TMP`'s **location** — inside the
repo tree,
`ticket_board::…nearest_project_root…` finds the repo's own `Ticketing/` above the tempdir and
`archive::…cpe1774…` hits a path-length limit. A reader reproducing from a nested worktree with a default
`TMP` would have got a baseline the comment did not predict and no way to tell whether the deltas were
stale.

**And a correction worth more than the result.** Round 1 built the surrogate half of the two-halves
fixture from a GUID surrogate tag, asserted the handle refusal's `"stands in for another name"` wording,
and passed — *and went on passing with the handle refusal disabled*, i.e. a green sabotage that read as
safety. `classify_reparse_tag` sends an **unrecognised** surrogate to the path-side `WHY_SURROGATE_TAG`
before the open, and that constant contains the same phrase. So the one-bit-apart GUID pair cannot reach
the narrowed handle check at all.

**Four legs, and the last two make different claims.**

- **Half 3** — a real symlink, the tag `classify_reparse_tag` passes through as `Probe::Link`: the one
  fixture that reaches the handle check unaided. Asserts on `"(a symlink, a junction, or a mount point)"`,
  which belongs to the handle refusal alone. This is what makes the disable sabotage red.
- **Half 4** — added in round 2 rather than documenting the remainder as a gap. Half 3 only proves the
  check fires for a *recognised* tag; every other surrogate is answered by `WHY_SURROGATE_TAG` first, so
  the handle check's coverage of the **non-symlink surrogate class** was still shadowed in CPE-1929's
  sense. The crate already owned the seam: `between_containment_and_open` now also plants a GUID tag,
  armed by `surrogate_between_containment_and_open_for_test`, in the window *after* the path probe has
  passed the name as an ordinary file — so the handle check is the **first** thing that can see it. The
  seam records whether the plant took, so "the guard refused" cannot be confused with "the fixture never
  armed" (CPE-1923).

**Round 2 wrote "sole decider" for half 4 and round 3 measured it false — the same defect this ticket
exists to remove, one level in.** `std`'s `FileType::is_symlink` on Windows is
`attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && tag & IO_REPARSE_TAG_NAME_SURROGATE != 0` — exactly
`reparse_name_surrogate`'s rule, read off a path instead of a handle, and this file's own reorder note
already said so. Half 4's fixture leaves the tag on disk, so the `symlink_metadata` net **below** the
handle check reaches it too. Measured (handle check disabled, half 3 neutered so half 4 could run):
half 4 reds carrying that net's sentence, *"is a link, and a batch never writes through one — a link's
target can be re-pointed after any check…"*.

So half 4 pins **ordering plus wording**, not visibility — and the guard's non-redundancy has a better
argument than "sole decider" ever was: **it reads the OBJECT.** An attacker who plants the tag before
the open and removes it before the `symlink_metadata` defeats the path net and not this one. Corrected
at five sites, the load-bearing one being the assertion's own failure message, which is what a future
reader sees when the test reds. The path net's *"deliberately unreachable — do not be alarmed when
sabotaging it leaves the suite green"* note now says explicitly that this is an **ordering** claim, with
half 4 as the measurement, so nobody upgrades it to "nothing gets here" and concludes the handle check is
redundant.

**The test asserts filesystem state, never `Ok`** (CPE-1957's lesson). Half 1 drives the whole write and
reads the bytes back, plus asserts the reparse bit is **gone** — which is the staged replace, and the
direct evidence for reason 1 above. Half 2 asserts the refused object still carries its reparse bit and
its `$DATA` length (it cannot be `fs::read` — a driverless surrogate answers `ERROR_CANT_ACCESS_FILE`
(1920), measured). Half 3 asserts the link is still a link; half 4 that the planted tag survives.

**Reason 1 is structural on both halves, not just "the handle isn't used"** (PR #1103 review, recorded at
the site). `read_alternate_data_streams` filters to `BACKUP_ALTERNATE_DATA` and explicitly skips
`BACKUP_REPARSE_DATA`; `carried_attribute_mask()` is `HIDDEN|SYSTEM|ARCHIVE|NOT_CONTENT_INDEXED` with no
`REPARSE_POINT`. So the staged replacement **cannot come back as a reparse point**, and a filter driver
behind a non-surrogate tag (WCI, ProjFS) cannot act — the bytes never reach it.

**Also corrected, because the decision inverts them:** `batch_execute::execute_one`'s step 4 still said
"Truncate + write **through the handle from step 2**", and its CPE-1725 note still said the bytes go
through that handle "plus the handle's reparse bit". Both false since CPE-1961/CPE-1959 respectively.
`src/docs/explorer-batch-media.md` gains a user-facing bullet and loses the same "writes through it".

`clippy --locked --all-targets -D warnings` clean in both feature modes (default and `--features index`).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1066's Reviewer (round 2), which called the follow-up
hook "weaker than the rest of the PR's own standard" while approving.

Related: **CPE-1896** (the dehydrated-placeholder rule this rests on), **CPE-1929** (the reorder that
surfaced the split, PR #1066), **CPE-1957** (the three shadowed sites left unmeasured — the same guard
family), **CPE-1958** (the TOCTOU race at the neighbouring guard, which this is *not*).

## Closing record — merged as PR #1103 (`8fa96407`), 2026-08-28

### The split, and the verdict

The codebase stated **opposite doctrines** about the same input class. `fsutil::overwrite_confirmed_no_follow`
**writes** a non-surrogate reparse point — CPE-1896's rule, that a dehydrated cloud placeholder (OneDrive,
dedup, WOF) is **an ordinary file the user expects to be written**. `batch_media::open_output_verified`
**refused** it on the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit, and PR #1066's test **cemented** that verdict.
Non-regressive, deliberately unresolved, and documented as such.

**Verdict: `batch_media` was wrong.** It now asks the narrow question, by **calling
`reparse_name_surrogate`** — the crate's single owner of the tag rule. No comment claims agreement with
`fsutil`; **the shared callee is the agreement** (CPE-1933).

**The crate now has one doctrine**, re-derived at run time by the Reviewer: `batch_media`, and both
`fsutil` sites, all narrow — and after CPE-1957 landed the same evening, **`vault_manager` no longer has a
bare-bit decision site at all.** That is what the two PRs together were for.

### The standing argument for the strict side collapsed, and not for the reason anyone expected

The defence was an asymmetry: *a refused batch item is **skipped** and the user keeps their input, whereas a
refused restore has failed at its only job.* Three reasons retired it:

1. **CPE-1961 removed the mechanism the refusal defended.** `VerifiedOutput::write_all` hands the handle to
   `fsutil::stage_bytes_over_checked_handle`, which stages a `create_new` sibling and commits by
   `rename_beneath` — **the placeholder is replaced by name**, so CPE-1896's three unresolved worries about
   writing *through* a `FILE_OPEN_REPARSE_POINT` handle are **unreachable**. The refusal had been defending
   a mechanism that no longer exists.
   **Verified hardest of anything in the PR, and the Reviewer added two supports the author had not cited:**
   `read_alternate_data_streams` filters to `BACKUP_ALTERNATE_DATA` and **explicitly skips
   `BACKUP_REPARSE_DATA`**, and `carried_attribute_mask()` is `HIDDEN|SYSTEM|ARCHIVE|NOT_CONTENT_INDEXED` —
   **no `REPARSE_POINT`**. So the staged replacement **structurally cannot come back as a reparse point**,
   which upgrades the claim from *"the handle isn't used"* to *"the result cannot be one either."*
2. **CPE-1957's unprivileged-plantable finding cuts TOWARD narrowing.** Anyone who can plant a non-surrogate
   tag can plant a **surrogate** one, which **both** doctrines refuse. **The bare bit was an
   attacker-triggerable refusal — a DoS on the batch item — not an attacker-blocking one.**
3. The cost of over-refusing is **systematic, not one item.**

### The evidence the ticket said nobody had, derived from code

`execute_one` → `Err` → `execute_plan_walk` pushes `(input, reason)` into `BatchReport::skipped` →
`App.svelte` renders **`skipped[0]` only**: *"{written} converted, {failed} skipped: first \"{name}\" —
{reason}"*. And **OneDrive dehydrates exactly the files it already synced — which are the batch's own
previous outputs.** So a re-run over a synced folder could skip **every item** and show a count plus one
sentence whose every noun is false for a placeholder (*"is a link… a link's target can be re-pointed after
any check"*) and which **names no action the user can take.** Verified end to end by the Reviewer through
`batch_execute.rs:505`, `App.svelte:3332-3336` and `i18n.ts:1041` + 12 locales.

### A green sabotage that nearly shipped

Round 1's surrogate fixture used the one-bit-apart GUID tag, asserted the **handle** refusal's wording, and
passed — **and went on passing with the handle refusal disabled.** `classify_reparse_tag` sends an
*unrecognised* surrogate to the **path-side** `WHY_SURROGATE_TAG` **before the open**, and that constant
carries the same phrase; **the GUID pair cannot reach the narrowed handle check at all.**

Fixed with a third leg — a real symlink, which containment passes through as `Probe::Link` — asserting on
`"(a symlink, a junction, or a mount point)"`. **Phrase uniqueness confirmed by the Reviewer:** that string
is split by a `\` line continuation (which is why grep only hits the test) and exists in **exactly one**
production message; the two `fsutil` analogues use the em-dash form in a different call path. **The new leg
does not have the old leg's defect.**

### "Sole decider" was measurably false — the round's own subject, one scope in

Half 4 (the GUID plant at `between_containment_and_open`) claimed the handle check is the **sole** decider in
its window. It is not: Rust's `FileType::is_symlink` on Windows is `reparse_name_surrogate`'s rule **read off
a path instead of a handle** — **and this file's own reorder note already said so, twenty lines above the
sentence.** With the handle check disabled, half 4 reds carrying the **path check's** sentence.

Corrected at **six** sites (five enumerated plus one the sweep found), including **the assertion's own
failure message**, which is the artefact a future reader sees. The Reviewer's verdict on the result:
***"the failure message predicts its own failure mode and is checked by producing it."***

**And the non-redundancy argument that survives any fixture is now the one written:** the handle check reads
**the object**, so a tag planted before the open and removed before the `symlink_metadata` **defeats the
path net and not this one.**

**The internal inconsistency was resolved rather than avoided.** The path net's *"deliberately unreachable —
do not be alarmed when sabotaging it leaves the suite green"* note now says explicitly that this is an
**ordering** claim, with half 4 as its measurement — so the next reader cannot upgrade it to *"nothing gets
here"* and conclude the guard above is redundant.

The author's own note on the failure mode, worth keeping: *"I wrote a false claim into the exact artefact
this PR exists to purge, and it would have been read as vouched-for by a green test."*

### CI went red after `APPROVE` + `SEC PASS`, and the reason is the record

```
error[E0425]: cannot find function `make_guid_reparse_point` in module `crate::fsutil`
    --> src/batch_media.rs:2137:33
note: found an item that was configured out — #[cfg(windows)]
```

Four jobs red — `Server crates` on **ubuntu and macos**, `MSRV`, and the verdict rollup. **Windows green.**
The F4 seam called a Windows-gated function from ungated code.

**Three careful parties missed it** — the author ran the full suite and both clippy feature modes; an
independent Reviewer re-ran every sabotage, re-derived the nine-site enumeration and audited the security
posture; the Foreman read both. **All three on Windows, all three green.** Nothing in the gauntlet *asks*
the platform question.

**The fix chose no non-Windows arm at all, deliberately.** A stub would compile everywhere and *read* as
portable, and if a later edit dropped the gate from half 4's test, **the leg would silently pass on Linux
against a fixture that was never planted** — the exact unarmed-fixture-reports-success shape
`surrogate_was_planted_for_test` exists to prevent. With no stub, the compiler enforces that the only
consumer is Windows-gated. Said at the site, with the reason. The whole seam is `#[cfg(all(test, windows))]`
with the gate on an **item** rather than a statement, matching how the crate already gates this family.

**And the worker said plainly what it could not do:** a non-Windows `cargo check` is impossible on this
machine — `zstd-sys`, `lzma-sys`, `ring` and `bzip2-sys` need `x86_64-linux-gnu-gcc`. *"Toolchain gap, not a
verdict on the crate."* So it built the check that **does** run anywhere: enumerate every `cpe-server` item
under a windows-only `cfg` with no `not(windows)` sibling (**96**), tokenise every added non-comment line in
the diff (**291** identifiers), intersect — **five real hits, each confirmed gated; one false positive from
an identifier inside a string literal.** MSRV checked directly (`cargo +1.88.0 check --locked --all-targets`,
exit 0) rather than reasoned about. **Filed as CPE-1988** to land that sweep as a standing guard.

### Sabotages, and the enumeration re-derived

Clean-TMP baseline **2,460 / 0 / 14** (later 2,461 after CPE-1957 merged, and **re-taken rather than
incremented**). Disable → 2,459/1; predicate lying → 2,459/1; un-narrowed to the bare bit → 2,459/1. **The
test asserts filesystem state, never `Ok`.**

**PR #1066's "seven `handle_facts` sites" was stale — there are nine** (`fsutil.rs:4237`/`4335` and
`vault_manager.rs:1930` post-date it), re-derived independently by both author and Reviewer, with a verdict
per site **including the ones that are fine**. Four consult `is_reparse_point`; `open_beneath`'s
`name_surrogate_at` is a fifth consumer of the rule, already narrow.

**Two false provenance claims found and fixed, both verified genuinely false at the base:**
`overwrite_confirmed_no_follow`'s docblock said *"this site refuses on the **bare bit**"* — false since
CPE-1929, **with a green test pinning the opposite**; and `execute_one`'s step 4 (*"write through the handle
from step 2"*) plus its CPE-1725 note — false since CPE-1961, **and both the exact premise this ticket
re-examined.**

**And the line numbers were dropped rather than refreshed**, for a reason that makes the point better than
the fix: round 3 carried five, **all five drifted inside the same PR**, and the Reviewer's refreshed list and
the author's own re-derivation **disagreed by about six lines**. *Two careful parties, same tree, different
numbers — which is exactly what a second unguarded copy of a fact the source already states looks like.*
Function names replace them and do not drift.

### Security — `SEC PASS`

**Newly writable:** every tag `classify_reparse_tag` returns `OpaqueData` for — OneDrive `0x9000001A`, dedup
`0x80000013`, WOF `0x80000017`, container isolation `0x80000018`, ProjFS `0x9000001C`, AppExecLink
`0x8000001B`, any third-party non-surrogate. **For every one, the write lands in a `create_new` sibling
committed by `rename_beneath` on a held folder handle — so a filter-driver redirection (WCI, ProjFS)
cannot act, because the bytes never reach the driver.** Structural, not conditional.

**Surrogates still refused on both sides:** path side via `classify_reparse_tag` → `WHY_SURROGATE_TAG`;
handle side via `reparse_name_surrogate`, which refuses **any** tag with `0x2000_0000` set, symlink and
mount point included. `unwrap_or(true)` fails closed. Carry-over of the placeholder's DACL, streams and four
attribute bits is **not a new capability** — an attacker who can plant a reparse point can plant an ordinary
file with the same DACL and streams, and that was always carried. Hard-link census, ADS handling and
containment unchanged; no capability added, `tauri.conf.json` untouched, no key material.

**One honest gap left standing rather than papered over:** the narrowed handle check is exercised by exactly
one fixture class in the ordinary path, because every other surrogate is answered by the earlier guard —
half 4 closes that for the non-symlink class **at the seam**, and the site says what remains.

### Gates at merge

`cargo test -p cpe-server --lib` **2,461 / 0 / 14** · clippy `--locked --all-targets -D warnings` clean in
**both** feature modes · `cargo +1.88.0 check --locked --all-targets` exit 0 · CI `completed success —
total_count=26 pending=0 skipped=1 coverage=ok`.

**Family:** CPE-1896 (the dehydrated-placeholder rule this rests on), CPE-1929 (the reorder that surfaced the
split), CPE-1957 (PR #1101 — the same question at the vault, answered the same evening, and the source of
the unprivileged-plantable finding), CPE-1961 (the staging commit that removed the defended mechanism),
CPE-1958 (the neighbouring TOCTOU race this is *not*), CPE-1988 (the cfg-intersection sweep this PR's CI
failure paid for), CPE-1932, CPE-1933, CPE-1950.
