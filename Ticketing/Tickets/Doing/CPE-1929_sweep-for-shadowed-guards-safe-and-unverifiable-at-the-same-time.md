---
id: CPE-1929
title: sweep for **shadowed guards** — a check that is simultaneously safe and unverifiable, because an earlier check answers on the same fact
type: task
priority: Medium
status: In Progress
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

- [x] Run the two-sabotage check against `batch_media::open_output_verified`: disable the later guard
      and see whether the suite stays green; separately force its predicate to lie and see whether
      behaviour changes. Both green means shadowed.
- [x] Sweep `crates/server` for the same shape more generally — any site where a **path** question and
      a **handle** question answer about the same property of the same name. `fsutil`, `batch_media`,
      `backup`, `revert_engine`, `archive` and `transfer` all carry link/reparse guards worth checking.
- [x] For each shadowed guard found, decide **reorder vs delete**. Reordering is right when the later
      guard asks the more trustworthy question (a handle cannot be substituted after the open; a path
      can) — that is what CPE-1896 did. Deleting is right when the later guard is genuinely redundant.
      Leaving it shadowed is the one wrong answer, because it reads as coverage.
- [x] Where a guard is kept **deliberately** as an unreachable backstop, say so at the site **and** say
      that it is untestable and why — so the next person's sabotage returning green is expected rather
      than alarming.
- [x] Consider whether the two-sabotage check can be mechanised at all, even partially. Probably not
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

## Third named lead, added 2026-08-27 (from PR #1049's re-review)

Two gaps in `hexColourSites()`'s new `scriptStyleAssignmentValues()` (CPE-1931). **Both are latent
today** — the reviewer grepped the tree and neither shape exists — and both lean toward the *safer*
failure direction. Neither is documented anywhere, which is why they belong on this sweep:

1. **`el.style.setProperty("--x", "#abc123")` is not counted.** `STYLE_ASSIGNMENT_START` requires
   `.style.<prop> =`; a method call has no `=`, so it never matches. Structurally the **same
   false-negative class** as the `FileList.svelte` bug CPE-1931 round 2 fixed — a real CSS-affecting
   value set from `<script>`. Unlike the `style:prop={"#literal"}` gap, which that round explicitly
   documents as a known deliberate non-fix, this one is unmentioned.
2. **A regex literal containing a quote character inside a `.style.` assignment desyncs quote
   tracking.** e.g. `el.style.color = /['"]/.test(x) ? "#fff" : "#000"`. The apostrophe opens a quote
   that never closes, so the char-scanner swallows to end-of-source. Net effect measured: the *next*
   real assignment is **double-counted**, not dropped — because each `.style.` start is re-found by a
   fresh pass over the untouched source and re-scanned from `quote = null`. So a desync in one
   assignment cannot blind the matcher to another; it can only **inflate** the count of the
   assignment containing the bad regex, surfacing as a false-positive CI failure on unrelated code.

Worth a note or a fix when someone next touches that matcher. The second one is the more interesting
of the two — a hand-rolled character scanner over JS is exactly the shape that produced
`shellScriptLines.ts`'s escaped-quote bug, and it was found by feeding it adversarial input rather
than by reading it.

## Work Log — 2026-08-27

Baseline before any change, `cargo test --lib` in `crates/server` on Windows: **2,423 passed / 0 failed /
11 ignored**. Every number below is from that same command unless stated.

### The two-sabotage check, per candidate

**1. `batch_media::open_output_verified` — SHADOWED, confirmed.** The named lead, and it held.

| sabotage | result |
|---|---|
| disable the later guard (`if false && facts.is_reparse_point`) | **2,423 passed / 0 failed** — unchanged |
| force the predicate to lie (`is_reparse_point: false` in the Windows `handle_facts`) | **2,422 / 1** — and the one failure was `fsutil::cpe_1896_a_non_surrogate_reparse_point_at_the_leaf_is_written_not_refused`, a *different* site's control. **Zero `batch_media` behaviour changed.** |

Both green for this guard. Shadowed by the `symlink_metadata(output).file_type().is_symlink()` path
check standing in front of it: on Windows `std`'s `is_symlink` reads the same name-surrogate reparse
bit, so every symlink and junction anyone could stage was refused there and the handle check was never
the decider for any fixture in the crate.

**Decision: REORDER, not delete** — the same direction CPE-1896 took at `fsutil`. The handle check asks
the more trustworthy question: the object it interrogates is the one the bytes will land in and cannot
be substituted after the open, whereas the name can be swapped either way in the window before a
`symlink_metadata`. Deleting was wrong here because the handle check is strictly *broader* — it sees
non-surrogate reparse points the path check cannot.

Made verifiable by a new test,
`cpe_1929_a_non_surrogate_reparse_point_at_the_output_is_refused_by_the_handle_check`, using
`make_guid_reparse_point` (no privilege, no filter driver). Its control asserts `is_symlink()` is
**false** on the fixture — without that the path check would satisfy the test for free, which is exactly
how the pre-CPE-1896 fsutil test proved nothing. After the fix: **2,424 / 0**; with the guard disabled,
**2,423 / 1** — the new test is the red-proof. (Both figures were taken before the `fsutil` test below
existed. On the merged state, where the suite is **2,425**, the same sabotage is **2,424 / 1** — the
security audit's correction, re-measured here and confirmed. The in-code comments now state the delta
rather than an absolute that will read two low to the next person.)

The `symlink_metadata` path check is **kept** as the second net and now documented as *deliberately
unreachable on every shipped platform* — on Windows the handle check has already refused; on
Linux/macOS/BSD `O_NOFOLLOW` fails the open first; only an exotic Unix compiling `O_NOFOLLOW` as `0`
reaches it. Measured and recorded **at the site**, so the next green sabotage is expected: with this
backstop and its `fsutil` twin BOTH disabled, the suite is **2,425 / 0** on the merged state —
identical to baseline, so the figure covers each of them individually too.

**2. `fsutil::overwrite_confirmed_no_follow` — SHADOWED, found by the sweep, confirmed.** Same shape,
not previously named.

| sabotage | result |
|---|---|
| disable both later refusals (`if false && facts.is_reparse_point`, `else if false && facts.is_dir`) | **2,424 passed / 0 failed** — unchanged |
| force `is_reparse_point` to lie | no failure at this site (only the `fsutil` *leaf* control, a different function) |

**Decision: REORDER *and* NARROW.** Reordering alone would have turned a latent bug into a live one:
the only input that could ever reach the reparse refusal past the path check is a **non-surrogate**
reparse point — a dehydrated cloud placeholder, dedup, WOF — and refusing those is precisely the
regression CPE-1896 removed from the sibling `copy_file_onto_destination_handle`. So the guard was
unreachable *and*, in its one reachable case, wrong. It now asks
`reparse_name_surrogate(&file).unwrap_or(true)`, matching the sibling exactly.

The path check is kept below, and here it is a genuine fallback rather than a formality: `handle_facts`
returning `None` (a platform whose identity model `batch_media` does not know) falls through to it, and
that is the case this function's original doc argued for. Documented as such.

New test `cpe_1929_overwrite_confirmed_refuses_a_surrogate_but_writes_a_non_surrogate_reparse_point` —
the CPE-1896 two-halves fixture pointed at this function, the two destinations differing in **exactly
one bit**. It asserts on "stands in for another name" rather than on "link", because the path check
below also says "link" and would satisfy a looser assertion for free. After: **2,425 / 0**; sabotaged:
**2,424 / 1**.

**3. `src/lib/lockfileLockedGuard.test.ts` — the raw-text scanner, two false negatives measured.**
Baseline 6 tests passing. Both sabotages left it fully green:

| sabotage | old guard | rewritten guard |
|---|---|---|
| delete BOTH real `cargo check --locked` preflights from `gui-smoke.yml`, replacing them with `run: echo skipped   # cargo check --locked --all-targets` | **6 passed / 0 failed** | **2 failed / 8 passed** |
| rewrite ci.yml's `run: cargo test --locked` as `cargo \` / `test --all-targets` (a real backslash continuation, no `--locked`) | **6 passed / 0 failed** | **1 failed / 9 passed** |

The first is the *same* false negative the file's own comment says was already found and fixed once —
the fix only covered comments on their own line, so the trailing form was still live. The second was
invisible rather than unlocked: the first physical line does not match `cargo\s+(build|test|…)` and the
second contains no `cargo`.

**Note on the second sabotage, because it nearly went the way the ticket warns about.** The first
attempt at that fixture wrote a literal `\` + letter `r` instead of `\` + CR, so the "continuation" was
not one. Both guards "missed" it and it read as a confirmed finding. Caught by dumping the bytes
(`92,114,10` where `92,13,10` was intended). A fixture that reproduces a defect for the wrong reason is
the same trap CPE-1923 hit; re-measured with correct bytes and the finding held.

**Decision: MIGRATE, not patch.** Rewritten onto `parseYaml` (the migration its siblings already had)
plus the **shared** `logicalLines` from `src/lib/shellScriptLines.ts` — deliberately not a fifth
hand-rolled comment stripper. Reading `step.run` structurally deletes the old `startsWith("name:")`
heuristic rather than keeping it as dead code. The 40-line lookback for the Tauri preflight became exact
step adjacency: all four anchors in the tree have the preflight as the step directly above them, which
is what their own comments promise.

Parity measured both ways so the rewrite cannot have quietly narrowed the scan: old and new both find
exactly **79** cargo invocations (66/3/7/2/1), with no line found by the old detector and missed by the
new. Rewritten file: **10 tests passing**, four of them unit tests pinning the shapes that defeated the
old one.

**4. `hexColourSites()`'s `scriptStyleAssignmentValues()` — both gaps measured on adversarial input.**

- `el.style.setProperty("--x", "#abc123")` → **zero values extracted, zero hex counted.** A real
  false negative, the unsafe direction for a growth ratchet. **Fixed** by widening
  `STYLE_ASSIGNMENT_START` to `setProperty(` as well as `= `. No live site uses it today (grepped), so
  no baseline moves — this closes the door before someone walks through it.
- `el.style.color = /['"]/.test(x) ? "#fff" : "#000";` followed by a real assignment → **4 hex hits
  against a control's 3**; the extra is a double-count of the *later* assignment, nothing is dropped.
  **Deliberately not fixed**, documented at the site and pinned by a test: inflation trips the ratchet
  loudly, whereas teaching the char scanner to recognise JS regex literals needs the
  regex-vs-division heuristic and risks getting it wrong in the direction that **swallows** a real
  assignment — converting a loud failure into a silent one.

### Sweep of the rest of `crates/server`

Also documented, both measured as subsumed and therefore untestable on their own:

- `archive.rs` (staging sweep): `is_symlink() || !is_dir()` — `is_symlink()` implies `!is_dir()` on a
  `symlink_metadata` result on every platform. Kept as a statement of intent, noted as unreachable.
- `vault_manager.rs` (staging-prefix cleanup): `!is_file() || is_symlink()` — the second disjunct is
  the dead one. Same treatment.

Checked and cleared, each for a stated reason: `fsutil::claim_destination_handle` (already reordered by
CPE-1896), `backup::landed_inside` (path *containment* then handle *identity* — different properties),
`open_beneath::descend` (handle-only, no path question in front), `archive::entry_sink_action` (three
different facts, none implying another), `transfer.rs`/`archive.rs` claim sites (path probes already
removed), `fsutil::rename_slot_refusal` and `create_slot_refusal` (genuinely reachable, documented),
`batch_execute` (confirmation gate, explicitly not the last line of defence).

### Review round — five recording items, no structural change

1. **The PR failed its own new rule at four sites.** The CLAUDE.md entry says to write the numbers into
   the comment at the site; only the two *moved* refusals carried theirs. The two retained path
   backstops and the two dead disjuncts argued where they could have measured — on a ticket whose whole
   thesis is that an argument is not a measurement. Re-measured and recorded at all four: disabling
   **both** path backstops → **2,425 / 0**; deleting **both** dead disjuncts → **2,425 / 0**.
2. **In-code absolutes replaced by deltas.** The comments cited the suite size at the moment each was
   measured (2,423 / 2,424); the merged suite is 2,425, so a future reader re-running the sabotage
   would see numbers two higher than the comment. They now say "left the suite **unchanged** — N/0 at
   the time of measurement" and give the merged-state red-proof figure alongside.
3. **The doctrine split is now recorded at the `batch_media` site.** After this PR the two sites say
   opposite things about the same input class: `fsutil` **writes** a non-surrogate reparse point (the
   narrowing, on CPE-1896's dehydrated-placeholder rule) while `open_output_verified` still **refuses**
   it on the bare bit — now cemented by the new test. Not a regression (it refused them before too),
   but only one half was documented as a choice. The site now states the split, gives the one real
   asymmetry (a refused batch item is *skipped* and the user keeps the input; a refused restore has
   failed at the only thing it was asked to do), and says plainly that this has **not** been
   established as the right answer — unresolved on purpose, and **owned by CPE-1959**. Round 2 first
   pointed it at CPE-1958; that ticket is the `links > 1` TOCTOU race at a *neighbouring* guard, so a
   worker arriving there would be working the race and might never read the comment. Everything else
   this ticket deferred got a ticket that owns it; this now does too.
4. **CPE-1957 corrected**: `vault_manager` citations were `origin/main` numbers while `batch_media`'s
   was post-merge, so the set would have been stale on arrival. All now post-merge, with the reason
   stated in the ticket. Raised **Low → Medium**: site 1 carries the same bare-reparse-bit defect fixed
   here, so a dehydrated cloud placeholder in a vault session dir makes the wipe refuse — a live
   behaviour bug, not merely an unverifiable guard.
5. **The 79-parity became an assertion instead of a comment.** `toBeGreaterThan(0)` would not catch a
   *partial* narrowing — ci.yml could fall from 66 real invocations to 3 and still read as "the
   detector works". Now a per-file floor (`MIN_CARGO_INVOCATIONS`, 66/3/7/2/1), red-proofed by
   truncating each job's step list: **4 files fail**, each naming the file and the shortfall.

### Left, and filed as CPE-1957

`vault_manager::overwrite_pinned_file` (strongest of the three — on Windows the earlier `probe.is_link`
and the later `facts.is_reparse_point` are literally the same expression, and it carries the same
bare-reparse-bit defect this ticket fixed in `fsutil`), `vault_manager::same_object_or_refuse`'s link
re-check, and `revert_engine`'s Create-op occupancy check shadowing the write gate. Each is located to
the line with the shadowing check named. **The two-sabotage results are the one thing not carried
over — none was run against these three**, and saying otherwise would be exactly the "reads as
coverage" failure this pattern is about.

Separately, the security audit found and filed **CPE-1958 (High)**: `overwrite_confirmed_no_follow`'s
`links > 1` guard is TOCTOU-racy and was measured destroying a file outside the root — **17 / 1,000**
on this branch against **30 / 1,000** on a replica of `main`'s body in the same run, so the reorder
**halves the window** but check-then-use remains. `batch_media` under the identical racer: **0 /
2,000**. Deliberately not carried into this PR: re-checking the same racy fact cannot fix it — it needs
claim-then-rename or a post-write handle-identity re-verify, which is a different change.

### Mechanisation (last acceptance item)

**Answer: not worth full automation, and the reason is which half is automatable.** The "disable the
guard" half already has a tool — mutation testing; `cargo-mutants` would flag every one of these as a
surviving mutant. The "force the predicate to lie" half does not, because a machine cannot know what a
*lie* means for a given predicate (`Some(false)`? `true`? a stale value?). And neither half produces the
conclusion: *which* earlier check is shadowing it, and whether to reorder or delete, is a judgement about
which question is more trustworthy.

What is cheap and is now written down: a "Shadowed guards" entry in CLAUDE.md's "Guards and ratchets"
section naming the pattern, the two-green-sabotages tell, the reorder-or-delete rule, the requirement to
say so at the site when a backstop is kept deliberately, and the instruction to run the pair by hand
whenever a refusal is added or moved and write the numbers into the comment. **Every shadowed guard found
so far was found that way. None was found by reading the code** — including both of the two fixed here.

### Verification

- `crates/server` Windows: **2,425 passed / 0 failed / 11 ignored**.
- `crates/server` Linux (WSL, sources touched first): **2,410 passed / 0 failed / 11 ignored**.
- `cargo clippy --all-targets -- -D warnings` clean; same with `--all-features`.
- `npm run check`: 0 errors, 0 warnings. `npm test`: **4,932 passed, 2 skipped** at this branch's head.
  The figure moved three times while this ticket was open (4,857 → 4,883 → 4,923 → 4,932) purely
  because `main` landed fourteen PRs underneath it. That is exactly why the in-code sabotage comments
  now state a **delta** rather than an absolute: a suite size is a fact about a moment, and citing one
  as though it were a fact about the code is how a comment starts lying without anyone editing it.
- `src-tauri` `cargo test --lib`: **230 passed / 0 failed**. Added on review — this PR changes refusal
  *wording*, and `src-tauri` asserts on some of it, so clippy alone was not enough there.
