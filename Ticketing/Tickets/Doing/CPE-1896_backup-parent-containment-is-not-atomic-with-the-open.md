---
id: CPE-1896
title: backup's parent containment is not atomic with the open — a three-syscall parent swap writes outside the root and reports ok:true (measured 73/1200)
type: bug
priority: High
status: In Progress
tags: ready
estimate: L
created: 2026-08-26
---

## Summary

CPE-1889 closed the static case: a junction sitting in the backup destination no longer redirects a
write outside the root. What it did **not** close — and said so honestly, in the code and in
`src/docs/safety-undo.md` — is the race between the containment check and the open.

An independent Security Auditor turned that documented residual into a **measured** one, and the harm
it still reaches is byte-for-byte the shape CPE-1889 was filed for.

**The attack, in the common case** (parent already exists, so the second check never runs):

A junction is parked inside the destination as `dst/junc`. A racing thread does two renames —
`dst/sub` to `dst/sub_old`, then `dst/junc` to `dst/sub`. Rename works on a **non-empty** directory,
so the attacker never needs the slot to be empty.

**The window, in operations, not adjectives.** After check (1)'s `canonicalize(parent)` returns,
exactly three syscalls precede the destination open:

    std::fs::metadata(parent) -> File::open(src) -> r.metadata() -> batch_media::open_no_follow(dst)

**Measured result, `verify = false`:** ESCAPED 73, refused 1086, wrote-inside-normally 41, over 1200
trials. Verbatim from the probe:

    !!! ESCAPE at trial 16: the backup OVERWROTE "...\OUTSIDE\victim.txt" which is OUTSIDE the
    destination root, verdict ok=true err=""

**Verification does not notice.** With `verify = true`: 68 escapes over 1200. The sha256 read-back
re-opens the *same* redirected path, so it agrees with itself. **The verify leg is not a mitigation
for this** — that is the single most important sentence in this ticket.

**Blast radius.** It **overwrites** a pre-existing file outside the root, it also creates new ones,
and it reports `ok: true` with an empty error — the silent-success shape, not a loud skip. It is
**targeted, not arbitrary**: the attacker chooses the junction's target directory (a Startup folder,
an `.ssh` directory, a config dir); the only thing they do not choose is the filename, which comes
from the source tree — and a backup source tree contains thousands of names.

**Precondition:** write access to the destination tree — the same precondition as the bug CPE-1889
fixed, and a backup destination is by design an external drive or a share. Roughly a 6% single-shot
win rate on local NTFS with a naive racer; a real attacker loops, and a run gives one window **per
file**.

## Acceptance criteria

- [ ] Make the containment atomic with the open, per component. `std` cannot do this. It needs
      `openat2(RESOLVE_BENEATH)` on Linux and an `O_NOFOLLOW` directory walk (or `NtCreateFile` with
      `FILE_OPEN_REPARSE_POINT` per component) on Windows. CPE-1889's own doc comment already names
      this; start from there rather than re-deriving it.
- [x] Land the auditor's race probe as a repeatable test — `#[ignore]`d if it must be, but in the
      tree — so the fix has something that goes red without it. A fix for a race with no racing test
      is unverifiable, and this repo's recurring defect is guards that prove nothing.
- [x] **Cheaper partial mitigation, worth doing even if the full fix slips:** make the verify leg read
      back through a handle opened relative to a *verified* parent, so that at minimum the engine stops
      reporting `ok: true` on an escaped write. A loud failure is enormously better than a silent one.
- [x] Decide and record whether the same window exists on the other legs that resolve-then-write
      (archive extract, revert apply, copilot apply, transfer download). The auditor confirmed all of
      them check before `create_dir_all`, but none of them are atomic either. — **Recorded in the Work
      Log below; deliberately not fixed here.**
- [ ] Weigh the syscall cost of a per-component walk against PURPOSE.md's tiebreaker and record it.
      Correctness outranks speed here, but the number should be known — see CPE-1895.

## Notes

Filed 2026-08-26 by CPE-1889's independent Security Auditor, which staged all attacks inside its own
worktree and cleaned up every junction. CPE-1889 merged as PR #1031 with this residual known and
documented; this ticket is the follow-through, not a regression report against it.

Related: **CPE-1889** (the static case, closed), **CPE-1897** (the second check's race probe),
**CPE-1898** (the source leg's missing containment), **CPE-1895** (the syscall-cost measurement).

## Work Log

- **2026-08-26 (Worker) — SCOPE: this branch implements the MITIGATION HALF ONLY. The race itself is
  still open and this ticket STAYS OPEN.** Read this before anything below it. The branch
  `cpe-1896-verify-leg-catches-escaped-write` builds acceptance criterion 3 — the ticket's own stated
  *cheaper partial mitigation* — and deliberately nothing else. It does **not** build criterion 1: the
  containment is still not atomic with the open, the three-syscall window after the parent check is
  unchanged, and a racing thread can still get the bytes written outside the backup root. What changes
  is only that the engine no longer calls that a **success**. `openat2(RESOLVE_BENEATH)` on Linux and
  an `O_NOFOLLOW` / `NtCreateFile(FILE_OPEN_REPARSE_POINT)` per-component walk on Windows remain
  unwritten; criteria 1 and 5 (the syscall-cost measurement, CPE-1895) remain unticked, and criterion 4
  is *recorded* below rather than fixed. **Do not read this branch as closing CPE-1896.**

- **2026-08-26 (Worker) — what was built.**

  - **Mechanism: a landing check, `backup::landed_inside`, run AFTER the write.** The pre-write guards
    (CPE-1889's checks (1) and (2)) can only answer "where would the bytes go?", which is the question
    the race invalidates. Check (3) asks the one question that has an answer once the write has
    happened: *where did they actually go?* It canonicalises `dst`, refuses unless the resolved path
    is inside the already-resolved `real_dst_root`, and hands that resolved path back. Every
    uninspectable answer refuses, matching `fsutil::confined_to`'s stance for the write direction.

  - **The verify leg now reads back through the resolved, contained path**, not the plan-relative name.
    That one substitution is the whole of what stopped verification laundering the escape: `sha256_file(dst)`
    re-traversed whatever junction had redirected the write, so the read-back agreed with itself and
    produced a checksum-confirmed "backed up" for a file outside the root (the ticket's 68/1200). The
    resolved path cannot lead out of the root, because `landed_inside` would have returned an error first.

  - **It runs in BOTH verify modes, and that is a deliberate widening of the ticket's AC3.** The
    ticket scopes the mitigation to the verify leg, but the measured silent success was **73/1200 with
    `verify = false`** — gating the honesty on a setting the user may not have ticked would leave the
    *default* configuration reporting `ok: true` on an escaped write. The property this branch claims
    ("an escaped write is never reported as a success") is therefore unconditional.

  - **The message names the path it could not verify — both of them.** The refusal names the entry
    (`dst`) *and* the outside path the bytes actually reached, because by the time it fires the file out
    there has already been overwritten and a user who is not told where cannot go and look. Pinned by
    `cpe_1896_the_landing_check_refuses_a_destination_that_resolves_outside_the_root`, which asserts on
    both, not just on `is_err()`.

- **2026-08-26 (Worker) — the race probe, and what it asserts.**

  `cpe_1896_a_parent_swapped_under_the_copy_is_never_reported_as_a_success` is the auditor's recipe in
  the tree: a junction parked at `dst/junc`, a racing thread doing `dst/sub` → `dst/sub_old` then
  `dst/junc` → `dst/sub` (rename moves a **non-empty** directory, so `dst/sub` is deliberately populated
  and the attacker never needs the slot free), 600 trials sweeping the sub-millisecond offset with a
  spin count that varies per trial, **half the trials with `verify = true` and half with it off**.

  **It asserts the safety property, not the race outcome:** *if a write escaped, it was never reported
  as a success.* It does **not** assert "the race fired at least once" — that is an assertion on a rate,
  and a rate depends on machine, filesystem, scheduler and cache, which is the recipe for a test that is
  green locally and red on one runner every third week. The assertion is a conditional: vacuously
  satisfied on a run that hits nothing, violated the instant an escape comes back `ok: true`.

  **Observed rates, for the record and for nothing else** — nothing in the assertion depends on them:

  | run | trials | escaped | refused | wrote-inside-normally |
  |-----|--------|---------|---------|-----------------------|
  | local NTFS (`%TEMP%`) | 600 | **15** | 74 | 511 |
  | worktree volume (`Z:`) | 600 | **2** | 436 | 162 |
  | sabotaged, run A | 600 | 0 | 41 | 559 |
  | sabotaged, run B | 600 | (aborted on the first escape) | — | — |
  | sabotaged, run C | 600 | (aborted on the first escape) | — | — |

  The rate swings by an order of magnitude between two volumes on the same machine and same binary.
  That is exactly why it is not asserted on. The counts are printed to stderr each run so a future
  reader can see whether the window is still being hit at all; a run reporting 0 proves nothing.

  `#[ignore]`d deliberately: it spawns a thread per trial, plants a junction per trial and costs ~4–10 s
  of wall clock, none of which belongs on every `cargo test`. Run with
  `cargo test --lib cpe_1896_a_parent_swapped_under_the_copy -- --ignored --nocapture`.

  **When the atomic half lands, the assertion to ADD here is `escaped == 0`.** Adding it today would
  simply be red, which is the honest state of this ticket.

- **2026-08-26 (Worker) — red-proof, both directions.**

  - **Red.** Neutralising `landed_inside` to `Ok(dst.to_path_buf())` (a temporary env-gated sabotage,
    removed before commit — the committed tree has no such hook) and re-running the probe: two of three
    runs **FAILED**, aborting on the first escape with the silent-success shape verbatim —

    ```text
    HARM (CPE-1896): trial 447 wrote the backup's bytes to "…\t447\outside\victim.txt", OUTSIDE the
    backup destination, and reported it as a SUCCESS with an empty error — the silent-success shape
    this branch exists to close: OpResult { path: "…\t447\dst\sub/victim.txt", ok: true, error: "",
    outcome: Applied }
    ```

    The third sabotaged run passed vacuously (0 escapes) — the same rate-dependence recorded above, and
    the reason the probe cannot be the *only* proof.
  - **Deterministic red-proof, no timing:** `cpe_1896_the_landing_check_refuses_a_destination_that_resolves_outside_the_root`
    stages the exact on-disk state an escape leaves behind (a junction at `dst/sub` pointing outside,
    victim file at the far end) and asserts the refusal plus both named paths. A swap that happens
    *between two syscalls* cannot be staged deterministically by any test, so the engine-level proof has
    to race and this one does not — together they cover what neither covers alone.
  - **Green on healthy backups**, which matters at least as much: check (3) is now on the inner loop of
    every backup anyone runs. `cpe_1896_an_ordinary_backup_still_verifies_and_still_reports_success`
    covers both verify modes × both shapes that differ inside `copy_one_verified` (a not-yet-existing
    parent chain where `create_dir_all` runs, and an overwrite where it does not), asserting `ok` with
    **empty** error text and reading the copied bytes back off disk.
    `cpe_1896_the_landing_check_admits_an_ordinary_destination` pins the helper's positive answer.
    A landing check that reds on healthy backups would be worse than the bug it closes.

- **2026-08-26 (Worker) — cost.** One `canonicalize(dst)` per file, in both verify modes: the common
  case goes from **two** extra path resolutions per file (CPE-1889) to **three** — ~300,000 rather than
  ~200,000 on a 100,000-file backup. It replaces no existing work in the `verify = true` path (that
  leg's `sha256_file` now takes the resolved path instead of the plan path — same single open, different
  argument). Wall clock is **not** re-measured: CPE-1889's A/B already showed the first two resolutions
  sitting below the copy's own noise floor on local storage (+11.3, −67.0, −21.2, +29.2 µs/file — both
  signs), and a third does not change what that measurement can resolve. CPE-1895 owns measuring the
  whole guard against a network destination, where each resolution is a round trip.

- **2026-08-26 (Worker) — AC4, the other resolve-then-write legs: RECORDED, NOT TOUCHED.** The prior
  audit's finding stands and is confirmed by CPE-1889's own sweep: `archive::entry_sink_action` /
  `entry_dir_action`, `revert_engine::apply_write`, `copilot::apply_op` and `transfer::download_tree`
  all resolve their path **before** `create_dir_all`, so none of them has CPE-1889's original hole —
  **and none of them is atomic either.** Every one carries the identical three-syscall-class window
  between its containment check and its open, and none of them has a landing check. This branch
  deliberately changes none of them: they are four separate call sites with four different sinks and
  four different failure policies, and folding them into a race-mitigation ticket for the backup engine
  would make the change unreviewable. They need their own ticket (or the atomic primitive, which would
  serve all five at once). Recorded here rather than fixed, per the AC's own wording ("decide and
  record").

- **2026-08-26 (Worker) — docs.** `src/docs/safety-undo.md`'s "What the folder check does and does not
  promise" bullet — which already told the user the instant-of-the-write swap was not covered — gains a
  companion bullet saying what happens now if it fires: the job reports a **failure** naming the file
  and the outside path, verification no longer agrees with itself, and — stated in the same breath —
  **the redirect is not prevented**, only the false success. `copy_one_verified`'s doc comment gains a
  CPE-1896 section with the measured numbers and the same "still open" statement; `landed_inside` carries
  the mechanism, the failure policy, and its own residual (an attacker who swaps the junction *back*
  between the write and the check wins, which now requires winning twice in opposite directions).

- **2026-08-26 (Worker) — guardrails.** `cargo test` in `crates/server`: **2395 passed, 0 failed, 10
  ignored** (9 before — the new probe is the tenth), plus every integration binary green.
  `cargo clippy --all-targets -- -D warnings` clean in three modes: plain, `--features index`,
  `--features specta`. No new dependencies. No `specta::Type` struct touched (the change is two private
  functions in `backup.rs` plus tests), so no `bindings.gen.ts` regeneration. One
  `#[allow(clippy::disallowed_methods)]` added, on the probe's racing thread and *only* there: the lint
  fires on raw `fs::rename` because it replaces its destination silently and destroys a link at it —
  which is precisely the primitive the attacker uses, so routing it through `fsutil::rename_into_slot`
  would make the racer refuse to perform the attack and the probe would go quiet without the engine
  getting any safer. Every attack staged in this ticket ran inside a self-cleaning scratch tree (the
  escape target is a sibling directory *inside* it, never the machine's real filesystem); the probe was
  re-run with `TMP`/`TEMP` pointed inside the worktree to confirm it reproduces there too, and both
  scratch roots were checked empty of leftover junctions afterwards.

- **2026-08-26 (Worker) — WHAT IS STILL OPEN (the reason this ticket is not Done).** AC1: the
  containment is still not atomic with the open. The escape still happens at the measured rate; the
  bytes still land outside the backup root and still overwrite whatever is there. All that changed is
  that the engine says so. AC5 (syscall cost of a per-component walk vs PURPOSE.md's tiebreaker) is
  untouched and belongs with CPE-1895. **Next step for whoever picks this up:** `openat2(RESOLVE_BENEATH)`
  on Linux, `NtCreateFile` with `FILE_OPEN_REPARSE_POINT` per component (or an `O_NOFOLLOW` directory
  walk) on Windows, threaded down to `batch_media::open_no_follow` so the whole create-then-write class
  inherits it — then add `assert_eq!(escaped, 0)` to the probe already sitting in the tree.

- **2026-08-26 (Worker) — ROUND 2, after the Security Auditor's blocking finding on PR #1037. The
  scope note at the top still stands: this is still the mitigation half, the race is still open.**

  - **What was blocked.** Round 1's landing check asked containment about `canonicalize(dst)` and
    stopped. The auditor beat it by winning the same window **twice in opposite directions**: phase 1 is
    the known race (junction in, the write escapes); it then watches `outside/victim.txt` grow past
    4 KiB — proof the escaped write is streaming — and renames the innocent directory **back** before
    the engine asks where the bytes went. `canonicalize(dst)` then resolves to a real, ordinary file
    inside the root: on an `update` entry, the stale previous backup naturally already there.
    Containment says yes. Measured, `verify = false`, 400 trials: **297 escaped, 38 reported
    `ok: true`** — both harms at once, the escape *and* the backup silently not happening:

    ```text
    AUDIT A2 HARM trial 9: 2097152 bytes landed at "...\outside\victim.txt" (OUTSIDE the root) and the
    engine reported OpResult { ok: true, error: "", outcome: Applied }; the file inside the root still
    holds 21 bytes
    ```

  - **And round 1's own disclosure was FALSE about it**, which is the part that mattered most. It said
    an attacker who swaps back still has to get past the sha256 read-back — but **there is no read-back
    when `verify = false`**, which is precisely the configuration the check had been widened to cover,
    three lines above, for exactly that reason. That clause is deleted, not softened.

  - **The fix: the landing check is now a HANDLE question, not a second path question — and it needs no
    `openat2`.** `fsutil::copy_file_onto_no_follow_with_wording` already calls
    `batch_media::handle_facts` on the destination handle it writes through, for the reparse/directory
    guard it has always had, so the identity of the object the bytes went into is available for **no
    additional syscall**. It now returns that identity (new `CopiedOnto { bytes, written }`; the `pub`
    `copy_file_onto_no_follow` entry point every other caller uses is unchanged and still returns
    `u64`). `landed_inside` opens the contained path and compares. A swapped-back name is a *different
    file object*, so the comparison refuses it. Renaming cannot forge an identity, and the hard-link
    route to forging one is already refused upstream by that function's `facts.links > 1` branch.

  - **New `batch_media::open_existing_no_follow_read`** — the read-only twin of
    `open_existing_no_follow`. Read-only is load-bearing rather than tidy: the copy carries the source's
    permissions onto the destination, so asking for write access would `PermissionDenied` on **every
    read-only file in a backup** and turn the ordinary case into a reported failure.

  - **The one place it degrades, stated as a live residual rather than glossed.** An identity that is
    absent or **degenerate** (a zero volume or file index — `FileIdentity::is_degenerate`; several
    network redirectors let `GetFileInformationByHandle` succeed and return one) falls back to the
    containment answer alone, and the two-phase swap-back stays open **on those volumes**. The
    crate-wide rule says refuse on a degenerate identity; this caller deliberately does not, because
    refusing here does not mean "decline to act", it means **report every file of a backup as failed**,
    and a backup destination is by design an external drive or a share. An open that *fails* still
    refuses — "this attempt could not tell" is not the same as "this volume cannot answer". What would
    close the degenerate case is the path *of the open handle itself* (`GetFinalPathNameByHandleW`,
    `F_GETPATH` / `/proc/self/fd`), named on the function as the follow-up mechanism.

  - **Message improvements the review asked for.** Every refusal now names the **source file whose
    bytes are sitting at the escaped path** — a user told their file was destroyed but not what replaced
    it has half an answer, and `src` was right there. (`fsutil` withholds `src` at its own call sites
    for a stated reason about a private checkpoint store; that reason does not apply here, and the
    difference is recorded.) And `safe_join` now builds the path component by component instead of
    splicing the plan's forward slashes in, so paths stop rendering mixed (`...\dst\sub/victim.txt`) —
    that string is what `OpResult::path` carries to the dashboard and what every refusal interpolates.

- **2026-08-26 (Worker) — round 2 measurements.**

  - **The probe now runs the two-phase form on half its trials**, uses a 1 MiB source (so the escaped
    write actually streams) and an `update`-list entry (so the swap-back lands on a real file). With a
    1 MiB source the window is far wider than round 1's 19-byte file: **400 trials, 273 escaped (132
    one-phase, 141 two-phase), 0 reported `ok: true`.**

  - **The two-phase leg does NOT win on Windows here, measured, and that is recorded rather than
    hidden.** With the identity comparison deliberately neutralised — the state in which a landed
    swap-back *must* have tripped the harm assertion — 400 trials gave **131 two-phase escapes and zero
    `ok: true`**: the rename-back never completed before the engine looked. Retrying it (2,000 attempts
    10 µs apart) changed nothing except the runtime, 20 s → 256 s, so the retry was reverted. The likely
    cause is that the engine holds the escaped file open through the junction while it streams and
    Windows refuses the directory rename until that handle closes. **So the probe does not red-proof the
    identity comparison and must not be cited as doing so.** The leg is kept because it is the auditor's
    actual attack shape and because POSIX imposes no such restriction (`rename(2)` on a directory with
    open files inside always succeeds), so CI's Linux and macOS legs are where it has a real chance.

  - **What DOES red-proof it, 100% of the time, on every `cargo test`:**
    `cpe_1896_the_landing_check_refuses_a_swapped_back_path_that_is_not_the_file_it_wrote`. The
    post-swap-back state is fully stageable with no thread — that is the auditor's own demonstration —
    because afterwards every path genuinely *is* what it says it is and only the **object** differs.
    Neutralising the identity comparison reddens it every run; the other three CPE-1896 tests stay
    green, which is what makes it a proof of *that* leg rather than of the file in general.

  - **The admit test now covers both identity worlds**, including `written = None`, so the
    degrade-don't-refuse policy for identity-less volumes is pinned by a test rather than asserted in a
    comment.

- **2026-08-26 (Worker) — the probe's role is now written down at the probe, because it is a trap.**
  The reviewer measured its escape rate at **1 per 600 on the machine `TEMP` volume against 4, 4, 5 and
  3 per 600 on the worktree volume** — same binary, same machine, same afternoon. At 1/600 a zero-escape
  run is entirely plausible, so **a run of this test can go green against a removed fix**. Its doc now
  says, at the top: it is an **observation instrument** for a still-open race, **not** the regression
  gate; the deterministic landing-check tests are; and nobody should later "harden" it by asserting a
  rate, which would buy nothing they do not already give in exchange for a test that reds at random.
  Also recorded: it is markedly more sensitive with `TMP`/`TEMP` redirected to a fast local volume (the
  reviewer got 4-of-4 sabotage detection that way against 2-of-3 at the default), so run it that way
  when you want it to bite. The one rate-shaped assertion it should ever grow is `escaped == 0`, and
  only once the atomic half lands.

- **2026-08-26 (Worker) — noted for the atomic fix, NOT built here: a `#[cfg(test)]` synchronous
  injection hook between check (1) and the destination open**, letting a test perform the swap with no
  thread and no timing. That is what would make this whole class deterministically testable — including
  the Windows two-phase leg that will not land in-process — and it would make verifying the eventual
  `openat2`/`O_NOFOLLOW` fix far easier than racing it. Deliberately out of scope for a mitigation PR
  (it is a production seam added for tests, which deserves its own review), but it is the first thing
  the atomic ticket should build.

- **2026-08-26 (Worker) — round 2 guardrails.** `cargo test` in `crates/server`: **2396 passed, 0
  failed, 10 ignored**, plus every integration binary green. `cargo clippy --all-targets -- -D warnings`
  clean in plain, `--features index` and `--features specta`. No new dependencies. No `specta::Type`
  struct touched, so no `bindings.gen.ts` regeneration. `copy_file_onto_no_follow_with_wording` went
  `pub` → `pub(crate)` because `CopiedOnto` carries a `pub(crate)` type; a repo-wide grep confirms
  `backup::copy_one_verified` is its only caller anywhere and every other site calls the unchanged `pub`
  `copy_file_onto_no_follow`. Every attack ran inside a self-cleaning scratch tree with the escape
  target a sibling directory *inside* it; the probe was run with `TMP`/`TEMP` redirected inside the
  worktree, and both scratch roots were checked empty of leftover junctions afterwards.

## Status 2026-08-26 — the MITIGATION HALF shipped; this ticket stays open

PR #1037 merged as `074e7267`. **The race is not fixed.** The three-syscall window between the
containment check and the destination open is unchanged, and bytes still escape at the measured rate.
What changed is that the engine no longer calls an escape a success.

Deliberately scoped that way: the full fix needs `openat2(RESOLVE_BENEATH)` on Linux and an
`O_NOFOLLOW` per-component walk (or `NtCreateFile` with `FILE_OPEN_REPARSE_POINT`) on Windows, which is
a large platform-specific change and the wrong thing to attempt under an attempt cap.

**What shipped:** a post-write `landed_inside` that compares **file identity** — did the bytes go into
the object we wrote? — rather than comparing paths. The destination handle was already being asked for
its facts by an existing guard, so the identity costs no extra syscall. The verify leg now hashes the
resolved, identity-confirmed path instead of the plan-relative name, which is what stopped verification
laundering the escape by re-reading the redirected file and agreeing with itself. The check runs in
**both** verify modes, because the original 73/1200 was measured with `verify = false`.

**Proven closed by A/B**, same racer, same 400 trials, minutes apart: identity comparison neutralised →
206 escapes, **9 reported `ok: true`**; live → 214 escapes, **0 reported `ok: true`**, swap-back
completing 214 times in both. The attack landed equally; only the engine's ability to be lied to
changed.

## What the atomic half should build first

A `#[cfg(test)]` synchronous injection hook between the containment check and the destination open, so a
test can perform the swap with no thread and no timing. That converts this whole class from a
probability into a certainty and is the only way to test a between-two-syscalls swap deterministically.

And read **CPE-1915** before choosing an approach: `GetFinalPathNameByHandleW` / `F_GETPATH` on the open
handle is strictly better than identity — it answers containment directly rather than by proxy, works on
volumes that report no usable file index, and would close **CPE-1912** for free by letting the handle's
real path be compared against the *plan* path rather than only against the root.

Related, all filed from this work: **CPE-1912**, **CPE-1913**, **CPE-1915**.
