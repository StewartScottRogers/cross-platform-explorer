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

- [x] Make the containment atomic with the open, per component. `std` cannot do this. It needs
      `openat2(RESOLVE_BENEATH)` on Linux and an `O_NOFOLLOW` directory walk (or `NtCreateFile` with
      `FILE_OPEN_REPARSE_POINT` per component) on Windows. CPE-1889's own doc comment already names
      this; start from there rather than re-deriving it. — **Built: `crates/server/src/open_beneath.rs`.**
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
- [x] Weigh the syscall cost of a per-component walk against PURPOSE.md's tiebreaker and record it.
      Correctness outranks speed here, but the number should be known — see CPE-1895. — **Counted and
      wall-clocked in the Work Log below; the walk is not the cost, the pre-existing landing check is.**

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

## 2026-08-27 — THE ATOMIC HALF (AC1 + AC5), branch `cpe-1896-atomic-containment`

**The race is closed, not narrowed.** Read the red/green numbers before anything else: same probe, same
machine, same afternoon, 400 trials each.

| build | escaped | refused | wrote-inside-normally | probe verdict |
|-------|---------|---------|-----------------------|---------------|
| **this branch (atomic walk live)** | **0** | 394 | 6 | **PASS** |
| this branch, second run | **0** | 397 | 3 | PASS |
| sabotaged: walk swapped for the pre-fix path open, **CPE-1889 checks (1)+(2) restored** — main's shape | **369** (185 one-phase, 184 two-phase) | 3 | 28 | FAIL |
| sabotaged: walk swapped for the pre-fix path open, no path checks at all | **397** (199 / 198) | 1 | 2 | FAIL |

The third row is the honest comparison — it is what `main` does today — and it escaped on **92% of
trials**. (The ticket's original 73/1200 was measured against a 19-byte source; this probe uses 1 MiB,
so the window is far wider. The rate is a property of the probe and the volume, not of the fix.) The
fix was committed **before** any sabotage, and each sabotage was reverted with `git checkout --` against
a clean commit — never over uncommitted work.

- **2026-08-27 (Worker) — what was built: `crates/server/src/open_beneath.rs`.**

  - **The mechanism, in one sentence:** the run canonicalises the destination root once and **holds it
    open**, and every plan entry is then opened one path component at a time, each component resolved
    **relative to the handle of the component before it**. There is no second lookup of any parent, so
    the racing rename has nothing left to redirect. The handle the bytes go through is beneath the root
    *by construction* rather than by a check that could be stale.
  - **Windows: `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` = the parent handle**, plus
    `FILE_OPEN_REPARSE_POINT` and `FILE_DIRECTORY_FILE` per intermediate component and
    `FILE_NON_DIRECTORY_FILE` for the leaf. Win32 has **no** handle-relative open — `CreateFileW`
    re-parses a full path from a drive letter every time — so the NT layer is not a preference, it is
    the only route. Directories are created with `FILE_OPEN_IF`, i.e. inside the handle we hold, which
    is why a refusal still cannot leave directory debris outside the root (CPE-1889 check (1)'s whole
    reason to exist, now structural).
  - **Linux: `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)`** as a one-syscall fast path, with an
    `openat`/`mkdirat` + `O_NOFOLLOW` per-component walk behind it. **Any** failure of the fast path
    falls through to the walk, which then produces the authoritative answer *including the authoritative
    refusal* — so the fast path cannot weaken anything and no errno classification is performed at all
    (the one real hazard of hand-rolling a syscall). It cannot replace the walk either: creating a
    missing directory chain still needs `mkdirat` per level, and `openat2` is absent before Linux 5.6
    and blocked by some seccomp policies. macOS and every other Unix use the walk.
  - **`fsutil::copy_file_onto_destination_handle`** is the split-out body of
    `copy_file_onto_no_follow_with_wording`: same function, same three handle-read guards (reparse
    point, directory, `links > 1`), but the destination opener arrives as a **closure**. That is not
    style — the source must still be opened and described *first*, or an unreadable source would leave
    behind an empty file this call had just created at `dst`, a new silent way for a failed entry to
    change the user's tree. The old `pub` entry point `copy_file_onto_no_follow` is untouched and every
    other caller is unchanged.
  - **`backup::copy_one_verified` no longer resolves any path before the write.** CPE-1889's checks (1)
    and (2) are behind `if !open_beneath::ATOMIC`, a `const` that is `false` on every platform this
    ships on, so they compile out. They are kept in the source because on a target with no
    handle-relative open the walk degrades to a path open and they are then the only containment there
    is.

- **2026-08-27 (Worker) — the residual, because there is exactly one and it is specific.**

  The walk holds a handle on each intermediate directory. An actor who **renames one of those directory
  objects out of the root** while the copy is in flight takes the write with it — we keep writing into
  the object we hold, wherever it now lives. This is strictly weaker than the bug it replaces: the
  attacker can only relocate a directory the backup itself owns, so the bytes cannot be aimed at a
  pre-existing `.ssh` or Startup folder the way a planted junction aimed them, and `rename` cannot put
  a directory *onto* an existing non-empty one. `openat2(RESOLVE_BENEATH)` is immune (it re-resolves
  from the root fd every call and the kernel enforces the property), so on Linux this exists only on
  the fallback walk. **`landed_inside` catches it after the fact on every platform** — which is the main
  reason it is kept rather than deleted as redundant, and is recorded on both functions.

- **2026-08-27 (Worker) — AC5, the cost. The walk is not the cost. Bisected rather than quoted.**

  **Syscalls, counted exactly** by a thread-local counter around every syscall the walk makes
  (`open_beneath::tests::cpe_1896_report_the_walk_syscall_cost`), for the ordinary `a/b/name.txt` shape
  with the chain already present: **5 syscalls/file creating a new name** (2 dirs x (open +
  `GetFileInformationByHandle`) + 1 create), **6 overwriting an existing one** (the exclusive create
  loses to the file being there, then the plain open). Those are *handle-relative* opens — one name
  against one open directory object, not a path walk from a drive letter — against the previous shape's
  2 `canonicalize` + 1 `metadata` + 1 open. So the count went up while the per-operation work went down.
  *(A first attempt used a process-wide `AtomicU64` and reported 5.16 syscalls/file for a shape that can
  only cost a whole number: libtest runs tests in parallel and the sibling tests were adding their own
  walks. Thread-local is exact.)*

  **Wall clock, `cpe_1889_measure_the_guard_cost`, 2,000 files, guarded engine vs the pre-fix shape:**

  ```text
  engine as it ships (walk + landing check)   +920.5  +1093.2  +945.7  +960.8  us/file
  landing check minus its identity probe       +71.2   +101.9   +92.5          us/file
  landing check removed entirely (walk only)  +100.9   -51.1    +19.6          us/file  <- both signs
  ```

  So: **the per-component walk measures below the copy's own noise floor** (both signs, exactly as
  CPE-1889's own A/B behaved), and **~850 us/file is `landed_inside`'s identity probe** — the read-only
  second open of the file just written.

  **And that ~850 us is not syscall time, it is antivirus.** `canonicalize` opens the destination for
  *attributes* (~80 us); the identity probe opens it for *read data*, and the first read-open of a
  just-written file is what makes Windows Defender's real-time scanner scan it synchronously. Two opens
  of the same file, one ~80 us and one ~850 us, is the giveaway. Recorded as measured rather than filed
  as a syscall cost, because tuning syscalls would not move it.

  **Against PURPOSE.md's fast/small/predictable tiebreaker:** this change is **cost-negative** — it
  removes two `canonicalize` calls per file and adds a walk that does not measure. The ~950 us/file is
  **inherited from PR #1037**, not introduced here, and this is the first time anyone has measured it;
  it is ~95 s on a 100,000-file backup. The obvious follow-up is deleting the identity probe, which the
  atomic walk makes redundant (it existed to catch a swap-back that can no longer redirect the write) —
  **deliberately not done here**, because removing an auditor-mandated guard belongs in its own reviewed
  change and not in the same PR as ~800 lines of new platform FFI. CPE-1915's `GetFinalPathNameByHandleW`
  route would remove both re-opens at once. **Recommend the Foreman file that follow-up.**

  **Host and tooling, since none of these numbers travel:** Windows 11 Pro 10.0.26200, x86_64, local
  NTFS (`%TEMP%`); `rustc`/`cargo` **1.98.0** (`x86_64-pc-windows-msvc`) — determined by `rustc -Vv` and
  `cargo -V` on the `~/.cargo/bin` toolchain this worktree builds with, not by assumption; Defender
  real-time protection **on**, antimalware engine **4.18.26070.9** (`Get-MpComputerStatus`); `cargo test`
  **debug** profile. CPE-1895 still owns re-measuring against a network destination, where each
  resolution is a round trip and the balance between those rows changes completely.

- **2026-08-27 (Worker) — decide-and-log assumptions.**

  1. **`openat2` is a fast path, not the Linux implementation.** The AC names it; the walk is what
     actually carries the guarantee on every Unix. Reasoning above: directory creation needs `mkdirat`
     per level regardless, `openat2` is not universally available, and falling through to the walk on
     *any* failure is what makes the fast path incapable of weakening the result. This is also the
     lowest-risk shape for code I **cannot execute locally** — this is a Windows machine, so CI's
     ubuntu and macOS legs are the first execution of the Unix arm.
  2. **`landed_inside` kept, not deleted**, despite being ~99% of the guard cost and largely redundant.
     Reasons in the residual section above; the removal is a separate reviewed change.
  3. **Two Cargo feature/dependency additions, no new crate.** `windows` 0.56 gains
     `Wdk_Foundation` + `Wdk_Storage_FileSystem` + `Win32_System_IO` (the same already-vendored crate,
     three more feature flags); `libc` is named directly under `[target.'cfg(unix)'.dependencies]` and
     was already resolved in **both** lockfiles via `xattr`/`rayon`/`rusqlite`. `cargo tree` gains
     nothing. Both `Cargo.lock`s regenerated (`crates/server` and `src-tauri`) — one added `libc` line
     each, and the `version = 3` → `4` format bump cargo 1.98 wanted was reverted in both.
  4. **One existing assertion changed, deliberately made stronger.**
     `cpe_1889_a_junction_at_the_parent_never_redirects_the_write_outside_the_root` asserted
     `error.contains("outside")`, which was all a path resolution could establish. The walk never
     resolves the whole path, so the refusal now names the exact component: the assertion is
     `contains("\"sub\"") && contains("is a link")`. The harm assertions in that test are untouched.

- **2026-08-27 (Worker) — the probe's assertion, and why a quiet run still proves little in one
  direction only.** `assert_eq!(escaped, 0)` is added, exactly as this ticket said it should be once the
  atomic half landed. The asymmetry matters and is written at the test: a **zero** count on the *pre*-fix
  code proved nothing (the window was hit a few times per 600 and the rate swung by an order of magnitude
  between volumes), which is why the probe was previously labelled an observation instrument; a
  **nonzero** count on the post-fix code is a defect in the walk, because the property no longer depends
  on winning a race. It stays `#[ignore]`d on cost alone — 400 trials x 1 MiB x a thread and a junction
  each is **~64-90 s** here. The per-run deterministic cover for the same mechanism is
  `open_beneath::tests::refuses_a_link_at_an_intermediate_component_and_writes_nothing_through_it`, which
  needs no thread and no timing and asserts the victim's bytes are untouched.

- **2026-08-27 (Worker) — guardrails.** `cargo test` in `crates/server`: **2401 passed, 0 failed, 10
  ignored** (2396 before; the five new ones are `open_beneath`'s), plus every integration binary green.
  `cargo clippy --all-targets -- -D warnings` clean in **three** modes: plain, `--features index`,
  `--features specta`. No new dependency crate. No `specta::Type` struct touched, so no
  `bindings.gen.ts` regeneration; no frontend TypeScript touched, so no `npm run check` needed. No
  public API change — `open_beneath` is `pub(crate)`, `copy_file_onto_no_follow` is unchanged, and a
  repo-wide grep confirms `src-tauri` and the sidecars name none of the changed items. `src/docs/safety-undo.md`
  updated: the bullet that told the user the instant-of-the-write swap was *not* covered now says it is,
  and names the renamed-folder residual that replaces it. Every attack ran inside the crate's
  self-cleaning `ScratchDir` guard with the escape target a sibling directory *inside* it; nothing was
  installed or changed machine-wide.

## 2026-08-27 — ROUND 2 on PR #1043: Reviewer CHANGES REQUESTED + Security Auditor findings

**The containment itself was confirmed sound by three independent passes** before any of this. The
Reviewer audited `nt_child`/`walk` line by line ("this code is correct" — every `OBJECT_ATTRIBUTES`
field, no handle leak on any path, MAX_PATH-free) and reproduced the probe both ways on its own
machine (400 trials → 0 escapes; sabotaged → **119 of 120** escaped). UAT ran it independently: 0/400.
The Security Auditor ran 1,100 trials across three probe shapes — including its own 3-level chain with
the junction at `a/b` giving two race windows — for **0 escapes, 0 silent successes**, and could not
get round the fix by hard link at the leaf, by swapping the root mid-run, or with 39 hostile path
shapes. Every blocker below was in the arm I had flagged as never executed, or in prose.

- **2026-08-27 (Worker) — I found a way to execute the Unix arm, so B1/B2 are measured rather than
  reasoned.** This is a Windows machine, which is why the Unix arm shipped unexecuted in round 1. WSL2
  is present (kernel 6.6.87.2) but has **no C toolchain**, so `cargo` cannot link there. The route that
  worked: `rustup target add x86_64-unknown-linux-musl` (machine-global, **left installed**, see
  shared-machine note below), then cross-build a **static musl** binary on Windows with
  `-C linker=rust-lld` and run it under WSL. The harness crate lived in `.unixcheck/` inside this
  worktree and **extracts the Unix arm verbatim from the shipping file** with a script rather than
  hand-copying it, so it cannot drift from what CI compiles; it was deleted before the final commit.
  `x86_64-unknown-linux-gnu` and `x86_64-apple-darwin` std were already installed, so both CI arms can
  also be *type-checked* from here without a linker.

- **2026-08-27 (Worker) — B1: classify a link by asking the filesystem, not by reading the errno.
  RED then GREEN on a real Linux kernel.**

  `child_dir` opens each directory component with `O_RDONLY | O_DIRECTORY | O_NOFOLLOW`, and the code
  mapped only `ELOOP` to the link wording. Linux's `do_open()` reaches the `LOOKUP_DIRECTORY` check
  (`-ENOTDIR`) **before** `may_open()`'s `S_IFLNK -> -ELOOP`, so a symlink at an *intermediate*
  component reports `ENOTDIR`; xnu has the same ordering, and FreeBSD returns `EMLINK` — the errno
  test was wrong on three platforms. Containment always held; the *message* was wrong.

  ```text
  errno classification (as shipped in round 1)   intermediate: victim_intact=true says_link=FALSE
                                                 "the path component "sub" could not be opened
                                                  (Not a directory (os error 20))"
  fstatat(AT_SYMLINK_NOFOLLOW) classification    intermediate: victim_intact=true says_link=TRUE
  ```

  Fixed with a `link_at()` helper that runs **only on a component that already failed to open**, so it
  costs nothing in the ordinary case.

- **2026-08-27 (Worker) — and the two assertions the Reviewer flagged were proving NOTHING anyway.**
  Found by the harness, not by the review: `refuse()`'s shared tail ended "…and a component that **is
  a link**, or that cannot be opened, stops the entry", which put the literal phrase `is a link` into
  *every* refusal this module produces. `assert!(err.contains("is a link"))` therefore passed for a
  permission error, a vanished name, or a plain file sitting where a directory belongs — both
  `open_beneath`'s test and `backup.rs`'s CPE-1889 assertion were green for the wrong reason. The tail
  is reworded so boilerplate and diagnosis are lexically disjoint, and a new test,
  `a_plain_file_where_a_directory_belongs_is_refused_but_not_called_a_link`, pins the **negative**
  case — which is the only thing that proves the classification discriminates rather than always
  answering "link". This is the repo's recurring defect (a guard that proves nothing) caught in my own
  work.

- **2026-08-27 (Worker) — B2: the `openat2` fast path was dead for every overwrite. Measured both
  ways.** `open_how.mode` was `0o666` unconditionally; the kernel's `build_open_flags()` rejects
  `!WILL_CREATE(flags) && mode != 0` with `EINVAL`, and `EINVAL` is in the process-wide latch set — so
  the **first overwrite in a run turned the fast path off for the whole process**, and every
  `update`-list entry (the case the module doc calls the common one) fell into the walk, invisibly,
  because the fast path swallows errors by design. Now `mode: if flags & O_CREAT != 0 { 0o666 } else
  { 0 }`. Measured on WSL2 with the syscall counter, 10 overwrites of an existing `a/b/f.txt`:

  ```text
  mode: 0o666 unconditionally (the bug)    8 walk-syscalls/file   fast path dead, full walk every time
  mode: 0 when not creating (the fix)      2 walk-syscalls/file   fast path serves the overwrite
  ```

  `rootfd` is also widened to `c_long` for the variadic `syscall()` call, and the latch now carries a
  comment saying it is process-wide and permanent and must only ever be set by kernel-property errors.

- **2026-08-27 (Worker) — B3: deleted the fallback that had never compiled.** The
  `#[cfg(not(any(unix, windows)))]` arm passed `root.path` (a `PathBuf` behind `&RootDir`) to
  `refuse(&Path, …)`: two `E0308`s and an `E0507` when the Reviewer extracted and built it. Because
  `ATOMIC` was a `const bool` rather than a `cfg`, `dead_code` stayed silent, and the only two callers
  of `parent_contained` sat inside the block it guarded — a security check with no coverage behind a
  fallback that could not exist. **Deleted together**: the fallback `sys` module, `ATOMIC`, the
  `if !ATOMIC` block, and `parent_contained`. `open_beneath` is now `#[cfg(any(unix, windows))]` with
  no fourth arm, so an unsupported target fails the build loudly. (`confined_to_resolved_root` stays —
  it is `pub` in `fsutil` with its own test, not orphaned.)

- **2026-08-27 (Worker) — N1/F1: refuse a NAME SURROGATE, not every reparse point.** Confirmed by the
  Auditor's measurement on both sides: on `origin/main` a destination holding `dst/real/` and a
  junction `dst/link -> dst/real` copies `link/x.txt` with `ok = true`; on round 1 the same entry came
  back `ok = false`. Correct for the junction — it is a surrogate and refusing it is the point, even
  pointing back inside the root — but `FILE_ATTRIBUTE_REPARSE_POINT` is equally set by **OneDrive
  Files-On-Demand**, NTFS dedup/HSM, WOF and ProjFS, and a backup destination inside a OneDrive folder
  with Known Folder Move is an ordinary setup. It now reads `FILE_ATTRIBUTE_TAG_INFO` and tests
  `ReparseTag & IO_REPARSE_TAG_NAME_SURROGATE (0x2000_0000)` — same one handle query, different
  information class. **The non-surrogate side is not staged in a test and I say so at the function**: a
  cloud placeholder needs OneDrive/dedup/ProjFS to create and cannot be staged on a CI runner. The
  Auditor also showed this branch is *diagnostics*, not containment — neutering it entirely still
  refused the CPE-1889 junction, because NT answers `ERROR_CANT_RESOLVE_FILENAME` on the next
  component — so failing open here is safe. The new refusal is documented for the user in
  `src/docs/safety-undo.md`.

- **2026-08-27 (Worker) — F2: the residual's stated reason was wrong, and the truth differs per
  platform.** Round 1 said the attacker "can only relocate a directory the backup itself owns … and
  `rename` cannot put a directory onto an existing non-empty one" — one reason offered for all
  platforms. Rewritten as three:
  - **Windows: unreachable.** Windows refuses to rename a directory with an open descendant whatever
    the share mode. Auditor-instrumented (rename timestamp vs the instant `apply_backup_plan`
    returned), moving the leaf's parent and grandparent during 192 MiB and 768 MiB copies: **218 and 30
    attempts, every one `Access is denied (os error 5)`, zero mid-flight escapes.** Also recorded at
    the residual: this is Windows' own open-descendant rule, **not** the held handle — `SHARE_ALL`
    deliberately includes `FILE_SHARE_DELETE` (so a run cannot stop the user deleting their own backup
    folder), so the handle grants no veto. A reader must not conflate the two.
  - **Linux with `openat2`: immune** — re-resolves from the root fd every call.
  - **Unix fallback walk (macOS, and Linux when the fast path declines): genuinely open.** POSIX
    `rename` has no such restriction, and the old reason is **false** here: renaming into a *new name
    inside* a pre-existing sensitive directory succeeds, and both the directory name and the filename
    come from the source tree, so an actor controlling the source picks the whole landing path. Only
    `landed_inside` saves this case.
  - Also recorded, because it produces a false positive: a residual test without an ordering proof
    reports an escape that is really the racer winning *after* the plan returned (the Auditor's first
    un-instrumented attempt reported "100,663,296 bytes outside the root, ok:true" this way).

- **2026-08-27 (Worker) — F3: "`landed_inside` catches it on every platform" was an over-claim.** It
  short-circuits to the path answer alone when `handle_facts` returns `None` or the identity
  `is_degenerate()` (network redirectors returning a zeroed file index). On such a destination the
  residual is **silent, not loud** — and a backup destination is often exactly that kind of volume. Now
  says "every platform with a usable file identity", names the two short-circuits, and cross-references
  CPE-1895 and CPE-1915.

- **2026-08-27 (Worker) — F4/N4: the handle description was wrong in the safe direction.** `held =
  Some(dir)` drops the previous handle, so exactly **two** handles are live (the root and the deepest
  component so far), not one per level. No accumulation on deep trees, and the residual's blast radius
  is one directory rather than the chain. Corrected in both files.

- **2026-08-27 (Worker) — the smaller notes.** **N2** (Win32-unaddressable names — `sub.`, `sp `,
  `NUL`, `a/b/c.txt:stream`): NT does no Win32 normalisation, so these create real, contained, but
  unaddressable objects; today only `backup::safe_join`'s `win32_name_is_unstable` filter prevents it,
  and that is in the *caller*. Documented at `create_beneath` with the specific hazard a second caller
  would hit — the handle written and the object `landed_inside` inspects would be different files.
  (Not moved into the module: one owner for the vocabulary.) **N3**: the two `remove_file(dst)` calls
  in `fsutil` are now named as the only path writes left, with why they are bounded (they only run on a
  name this call exclusively created moments ago) and what closing them would cost. **N5**: the two
  `unwrap_or` conversions in `nt_child` now return `STATUS_NAME_TOO_LONG` instead of silently
  substituting a truncated `Length`, and the `unsafe` block is split so only `mem::zeroed`, the
  `NtCreateFile` call and `File::from_raw_handle` are inside one. **`open_root` needs read access** on
  the destination, so a write-only destination directory now fails the whole plan where `main`
  succeeded — rare and loud, recorded at the function with what the fix would be (`FILE_TRAVERSE` /
  `O_PATH`, no macOS equivalent).

- **2026-08-27 (Worker) — round 2 guardrails.** `cargo test` in `crates/server`: **2402 passed, 0
  failed, 10 ignored**, plus every integration binary green. Windows race probe re-run after all of the
  above: **400 trials, 0 escapes**. Unix harness on WSL2 kernel 6.6.87.2: **5 of 5 pass**.
  `cargo clippy --all-targets -- -D warnings` clean in plain, `--features index`, `--features specta`.
  No new dependency crate. No `specta::Type` touched. **Honest note:** one full `cargo test` run during
  this round reported a single lib failure that did not reproduce across two subsequent full runs and
  whose name I did not capture; the machine is shared with other agents and scratch-dir contention is
  the likely cause, but I cannot prove that, so it is recorded rather than dismissed.

- **2026-08-27 (Worker) — SHARED MACHINE.** `rustup target add x86_64-unknown-linux-musl` was run and
  **left installed** — it is additive, and removing it could break a sibling agent mid-run. Nothing
  else machine-global was touched: no `apt`, no `winget`, no PATH edit, no global git config. The WSL
  Ubuntu distro was used read-only (a static binary was executed in it; nothing was installed there —
  it still has no C toolchain).

## 2026-08-27 — ROUND 3 on PR #1043: the last two code gaps, and four corrected claims

**Standing evidence at the start of this round.** The Security Auditor re-ran on `830fd5f0` — 400 more
trials, **0 escapes** (398 refused, 2 normal), all 11 deterministic containment tests green: **1,500
trials across all rounds, zero escapes.** The Reviewer wrote its **own** extraction harness (rather than
reusing mine), cross-built static musl and ran it under WSL2 6.6.87.2, independently confirming B1's
premise, B2's `EINVAL`/`mode` behaviour, and `walk syscalls/file new=1 overwrite=2`. It audited `link_at`
line by line (no panic path, no fd, no leak, `parent` provably live, and it type-checks on Darwin where
`mode_t` is `u16`) and confirmed the "lexically disjoint" wording fix holds across the whole four-shape
`why` message space — and that the new negative test **would have failed under the old boilerplate**,
which is what makes it a real guard. It also corrected *itself*: its round-1 claim that `FILE_SHARE_DELETE`
made the Windows residual real was wrong.

- **2026-08-27 (Worker) — "cannot be staged" was wrong, and there is now a test instead of a comment.**
  `name_surrogate_at` claimed a non-surrogate directory reparse point needed OneDrive, dedup or ProjFS
  and so could not be staged on a CI runner. The Auditor disproved it: `FSCTL_SET_REPARSE_POINT` with a
  `REPARSE_GUID_DATA_BUFFER` and a **non-Microsoft** tag (bit 31 clear) needs **no privilege, no filter
  driver, no OneDrive**. New test-support helper `fsutil::make_guid_reparse_point`, and two new tests
  that stage both cases **on the same volume in the same run**, because the claim is that they diverge:

  | component | tag | surrogate | outcome |
  |---|---|---|---|
  | junction → inside the root | `0xA000000C` | set | refused |
  | junction → outside the root | `0xA000000C` | set | refused |
  | non-MS, directory bit set | `0x10001234` | clear | **traversed**, write landed inside |
  | non-MS, directory bit clear | `0x00001234` | clear | refused by NT, not by the check |

  The decisive detail for the stated motivation is measured, not inferred: `IO_REPARSE_TAG_CLOUD`
  (`0x9000001A`) is **N=0, D=1** — the same shape as the traversed row — so OneDrive Files-On-Demand
  *directory* placeholders are genuinely unblocked. **Recorded at the function: the check is NECESSARY,
  NOT SUFFICIENT.** For a non-surrogate tag the outcome belongs to NT and the filter driver; with the
  directory bit clear the descent fails whatever the check returns.

- **2026-08-27 (Worker) — F5: the leaf was still refusing on the bare bit, so the OneDrive motivation
  was only half-delivered.** `fsutil::copy_file_onto_destination_handle` still tested
  `facts.is_reparse_point` — the exact bit F1 rejected for the directory walk. A **dehydrated OneDrive
  Files-On-Demand file** is that shape, and OneDrive dehydrates precisely the files an incremental
  backup's `update` list writes onto, so "backups to OneDrive stop working" was still true at the final
  component while the prose on `open_beneath` read as though it were settled. Fixed.

  The bit and the query now have **one owner**, `batch_media::reparse_name_surrogate`, returning
  `Option<bool>` so that neither caller's default is baked in — because the two need **opposite**
  defaults and a default in the helper would silently give one of them the wrong one:
  - the **leaf** guard `unwrap_or(true)` — fails **closed**; nothing downstream catches it;
  - the **directory walk** `unwrap_or(false)` — fails **open**; a genuine surrogate is caught one
    component later by NT (`ERROR_CANT_RESOLVE_FILENAME`, measured by neutering the check).

  It costs nothing in the ordinary case: the tag is only queried when the reparse attribute is already
  set. **Red/green, both new tests:** reverted to the bare attribute bit → both FAIL; with the surrogate
  check → both pass.

  **The honest limit, measured rather than glossed:** reading a synthetic placeholder back by ordinary
  path fails with `ERROR_CANT_ACCESS_FILE (1920)` — a tag with no registered filter driver has nothing
  to service an ordinary open. That is a property of the *fixture*, not the code; a real OneDrive
  placeholder has the cloud filter in the path. The test therefore reads back through a no-follow
  handle and says so at the assertion. What is measured is that the tag discriminates and the bytes
  reach the object; what is *inferred* from Microsoft's documented meaning of the bit is that a real
  placeholder behaves like the synthetic one.

- **2026-08-27 (Worker) — F6: "Linux with `openat2`: immune" was too broad in two ways, one of them
  attacker-controlled.** Now says "immune for entries whose parent chain already exists", because
  `openat2` returns `ENOENT` whenever a parent is missing and the fast path falls through on **any**
  failure — so **every first-entry-into-a-new-directory takes the walk**, which on a first full backup
  is every directory in the tree. Measured on 6.6.87 (mine and the Reviewer's independently, and again
  in this round's harness): chain present → 1 syscall for a create, 2 for an overwrite; **new chain → 6**.
  And with a thread churning `rename(p ↔ q)`, **184,854 of 400,000 `openat2` calls (46%) returned
  `ENOENT`** — an actor with write access can force the Linux run off the immune path onto the
  residual-bearing walk, per entry, on demand. Not an escape, but "immune" read as a platform property
  when it is one an attacker can revoke. `ENOENT` is correctly outside the latch set for exactly this
  reason, and that is now stated at the latch. Corrected in `backup.rs` **and** in
  `src/docs/safety-undo.md`, which had said "on macOS, and on older Linux systems".
  (The Auditor also demonstrated the Unix residual live: renaming a directory with an open descendant
  **succeeds** on POSIX, and bytes written through the held fd landed in a pre-existing
  `dot-ssh/authorized_keys` — which is what the round-2 Unix bullet already said.)

- **2026-08-27 (Worker) — three stale or false claims killed.**
  1. `backup.rs` still said checks (1) and (2) "are compiled only where `[crate::open_beneath::ATOMIC]`
     is `false`". That is the claim B3's deletion removed, it contradicted the new comment 190 lines
     below it, and `ATOMIC` no longer exists — a **broken intra-doc link** that would never red CI
     (there is no `cargo doc` step and no `deny(rustdoc::broken_intra_doc_links)`). Rewritten as
     "DELETED, not disabled".
  2. Two orphans from the B3 deletion: `open_beneath`'s module doc still listed `backup::parent_contained`
     among the crate's containment guards, and the deleted fallback module's banner comment had survived
     and was sitting directly above `#[cfg(test)] mod tests`, labelling the **test module** "Anything
     else: no handle-relative open exists".
  3. **A false mechanism claim I introduced in round 2**, and my own measurement disproves it: I wrote
     that the `mode` bug latched the fast path off process-wide. It cannot — the latch is only reachable
     from the `O_CREAT|O_EXCL` arm, and the open-existing `EINVAL` is discarded by `.ok()`. The
     arithmetic settles it: a latch would have given **6** walk-syscalls per overwrite (the walk alone)
     and I measured **8** — the 6 plus 2 doomed `openat2` calls that `SUPPORTED = 1` kept re-paying
     forever. Corrected; the surrounding guidance about only latching on kernel-property errors is kept,
     and the latch now also records that only one of its two call sites can reach it.

- **2026-08-27 (Worker) — the `fstatat` clause.** Its doc said the second look was "harmless… the only
  thing at stake is which sentence the user reads". True of the *decision*, understated for the rest:
  swap the symlink for a regular file in the post-failure window and both give `ENOTDIR`, so **the
  attack signature disappears from the message**, and `link_at` biases the same way when `fstatat`
  itself fails. For an operator triaging a backup, "is a link" versus "not a directory" is the
  difference between seeing an attack and seeing a typo. Stated, along with the direction of the bias
  (toward under-reporting an attack, never toward inventing one).

- **2026-08-27 (Worker) — the unreproducible test failure: candidate identified, NOT fixed here.** The
  Reviewer named it and it is not this PR's code. `crates/server/src/archive.rs`: `EXTRACT_SEQ`
  (`AtomicU64`) and `SESSION_ROOT` (`OnceLock<PathBuf>`) are **process-global** while libtest runs lib
  tests in parallel in one process. `row1_a_squatted_temp_directory_is_stepped_over_not_written_into`
  snapshots `EXTRACT_SEQ`, then pre-creates `e{seq}` for a 64-wide block inside the live session root
  while sibling threads increment the same counter and create the same names, with only
  `STAGE_ATTEMPTS = 5` retries; its own doc comment already admits the sharing.
  `cpe_1786_many_extractions_add_one_directory_to_the_shared_root` contends on the same root. Ruled out:
  `fsutil::scratch_dir` (already `tag-<pid>-<counter>`), `shell_menu.rs`'s `HOME_ENV_LOCK`-guarded
  Linux-only leg, and `transfer.rs`'s pure path math. **`archive.rs` is untouched by this PR** —
  recorded for the Foreman to file separately.

- **2026-08-27 (Worker) — round 3 guardrails.** `cargo test` in `crates/server`: **2404 passed, 0
  failed, 10 ignored** (2402 before; the two new ones are the reparse-point tests), every integration
  binary green. `cargo clippy --all-targets -- -D warnings` clean in plain, `--features index`,
  `--features specta`. Unix harness re-run after every round-3 edit: **6 of 6 pass** on kernel
  6.6.87.2, now including a direct measurement of F6's claim (new chain → 6 syscalls, overwrite → 2).
  The extracted arm also `cargo check`s clean for **both** CI Unix targets, `x86_64-unknown-linux-gnu`
  and `x86_64-apple-darwin`. No new dependency crate. No `specta::Type` touched. Nothing machine-global
  was added this round (the musl target from round 2 remains installed).

## 2026-08-27 — ROUND 4 on PR #1043: a test that proved nothing, and why the suggested fix was not enough

**Security Auditor: SEC PASS. 1,900 race-probe trials across three heads (`46f6a02a`, `830fd5f0`,
`0496a271`), 0 escapes**, including 300 trials of its own deeper three-level-chain variant. Clean on
every route it tried: hard link at the leaf, root swap mid-run, 39 hostile path shapes, the disclosed
rename residual (unreachable on Windows across **248 instrumented attempts, all `Access is denied`**;
live on Linux's fallback walk under WSL2 and caught loudly by `landed_inside`), neutering the reparse
refusal, and forcing the shared predicate to both `None` and a lying `Some(false)`. One boundary is
recorded as unmeasured: it could not produce a genuine `None` on this machine, so the forced-predicate
experiment stands in for the unanswerable-volume case.

- **2026-08-27 (Worker) — the defect: my leaf test proved nothing, and it is this repo's signature
  disease one layer down.** The Reviewer sabotaged each guard independently instead of trusting my
  red/green claim. Disabling the leaf surrogate refusal outright (`if false && facts.is_reparse_point`)
  left `a_non_surrogate_reparse_point_at_the_leaf_is_written_not_refused` **passing** and the whole
  suite green: **2404 passed, 0 failed**. The refusal had *zero* coverage anywhere in the crate. Its
  "surrogate half" was a symlink, which is refused ~50 lines earlier by the unrelated
  `symlink_metadata(dst).is_symlink()` path check — so the two halves went on diverging with the tag
  check hard-wired to `false`, which is **exactly the condition the test's own doc claimed to
  exclude**. The half was also wrapped in `if make_file_link(...)`, `require_staged(supported_here =
  false)` on Windows, so a runner without `SeCreateSymbolicLinkPrivilege` skipped it **silently** — no
  `skip_notice!`, unlike the non-surrogate half above it.

- **2026-08-27 (Worker) — the suggested fix does not work on its own, and the measurement says why.**
  The proposal was to swap the symlink for `make_guid_reparse_point(&leaf, 0x2000_1234, false)` —
  "refused by the tag check and nothing else". Built it, and the test went **red on the wrong guard**:

  ```text
  and the SURROGATE refusal must be the one that fired — not the symlink path check …:
  …\surrogate.txt: this name is a link, and a restore never writes through one
  ```

  `std::is_symlink` tracks the **same name-surrogate bit** the tag check reads (the Auditor's own
  finding, arriving the same hour), so the path check catches every surrogate first. **No fixture can
  make the tag check the decider while a path question answering on the same bit runs ahead of it.**
  That is also the real explanation for the Auditor's forced-`Some(false)` result letting nothing
  through.

- **2026-08-27 (Worker) — so the path check now runs AFTER the handle checks.** That is the ordering
  the rest of the function argues for: `w` is the object the bytes will enter and cannot be substituted
  after the open; `dst` is a name that can. **On Unix nothing changes at all** — `is_reparse_point` is
  always `false` there, so the path check remains the only net on Linux and macOS. On Windows the only
  change is *which* guard reports a surrogate, and therefore which one a test can pin. Both still
  refuse; both still remove a file this call created.

  **Red-then-green on the exact sabotage that was green before:** with the leaf surrogate refusal
  disabled, `cargo test --lib` is now **2404 passed, 1 failed** — the leaf test, failing with the
  message above because it detects that the *second* net caught it instead of the tag check. Committed
  before probing; reverted with `git checkout --` against the clean commit.

- **2026-08-27 (Worker) — the second net is now a tripwire, not an inference.**
  `cpe_1896_std_is_symlink_tracks_the_name_surrogate_bit` asserts `is_symlink` is `true` for
  `0x2000_1896` and `false` for `0x0000_1896`, tags differing only in bit 29. The Auditor was careful
  that this is **three data points on one Windows build**, consistent with keying on the bit but with
  the mechanism inferred rather than read out of the OS — and std promises nothing, so a future release
  could narrow it to the two documented tags without breaking its contract and silently remove the net
  the leaf guard's fail-closed argument leans on. The comment says "measured, tracked the bit", not
  "std guarantees"; the test is what makes a change to it loud.

- **2026-08-27 (Worker) — F7: the OneDrive claim split into measured and not-established.**
  - **Measured, and the absent filter driver cannot affect it — the classification.**
    `GetFileInformationByHandleEx(FileAttributeTagInfo)` is serviced by the **filesystem**, not by the
    tag's owner: the tag and its surrogate bit live in the NTFS reparse data and come back identically
    whether or not `cldflt` is loaded, and cloud tags are N=0 by construction.
  - **Not established: that the resulting file is correct.** The fixture gives three reasons not to
    assume it. The copy reports `Ok(16)` and the object is then unopenable by ordinary path
    (`ERROR_CANT_ACCESS_FILE`), so the fixture cannot distinguish "wrote a correct file" from "wrote
    into an unreadable object" — and on the fixture it is demonstrably the latter. Attributes
    afterwards are `0x420`: **the reparse bit survives `set_len(0)` and the whole copy**, and
    `FILE_OPEN_REPARSE_POINT` exists precisely to tell the owning filter *not* to hydrate, so on a real
    placeholder this could leave an object OneDrive still believes is a placeholder over replaced data.
    And `landed_inside` re-opens by ordinary path, which **fails on the fixture and would succeed on a
    real placeholder** — the two diverge on the final verdict too, in opposite directions.
  - Containment is untouched either way (the bytes cannot leave the destination), and the prior
    behaviour refused *every* dehydrated file, so this is better in expectation. But it is not a
    demonstration that backups into OneDrive work. **`src/docs/safety-undo.md` no longer tells users it
    does**: the folder case is stated as tested, the file case as accepted-but-unverified with an
    attended check pending, and the reassurance is scoped to "nothing can land outside your chosen
    destination".

- **2026-08-27 (Worker) — the three precision corrections, applied as asked.** (1) The `is_symlink`
  note is worded as measured across three synthetic tags, not as a property of std's contract.
  (2) The walk's cost is stated accurately: `name_surrogate_at` runs on **every** directory component
  and the attribute check happens *inside* the shared helper, so the ordinary case costs **one extra
  `GetFileInformationByHandleEx` per directory component** — the leaf's "only when the bit is set"
  gating is the leaf's alone. `tick()` fires before the call, so AC5's 5/6 and 6/2 figures already
  count it and nothing is re-measured. (3) `0x3000_1896` (surrogate + directory together) is recorded
  as an **observed staging limit** — `make_guid_reparse_point` returned `false` while the other three
  combinations staged fine, and nobody determined why — not as "Windows forbids".

- **2026-08-27 (Worker) — the two nice-to-haves.** The comment naming the helper
  `handle_is_name_surrogate` now names `reparse_name_surrogate`. And the shared helper records that
  **both defaults rest on reading, not measurement**: the `None` arm is untestable by construction —
  no fixture can make `GetFileInformationByHandleEx` fail on a handle that has just opened
  successfully, the only state either caller reaches it from — so it is argued rather than
  demonstrated, which is worth saying in a module where everything else is measured.

- **2026-08-27 (Worker) — round 4 guardrails.** `cargo test` in `crates/server`: **2405 passed, 0
  failed, 10 ignored**, all 11 targets green (2404 before; the new one is the `is_symlink` pin).
  `cargo clippy --all-targets -- -D warnings` clean in plain, `--features index`, `--features specta`.
  No new dependency crate, no `specta::Type` touched, nothing machine-global added. No flake observed
  this round, and none in the Reviewer's 131.66 s run either; the `archive.rs` `EXTRACT_SEQ`/
  `SESSION_ROOT` candidate recorded in round 3 remains untouched and unfiled.

## The generalisable finding from CPE-1896: two guards answering on one bit

Worth lifting out of the round-4 narrative, because it is transferable and it is not obvious. Stated
generally:

> **A guard cannot be given test coverage while an earlier guard answers on the same underlying fact.**
> No fixture can make the later one the decider, because every input that would trip it trips the
> earlier one first. The later guard is then simultaneously *safe* (nothing gets through) and
> *unverifiable* (nothing can prove it works) — and those two properties are easy to mistake for each
> other.

Concretely here: `fsutil::copy_file_onto_destination_handle`'s handle-based surrogate check reads the
reparse tag's `IO_REPARSE_TAG_NAME_SURROGATE` bit, and `std::fs::FileType::is_symlink` on Windows
**tracks that same bit** (measured independently and within the same hour by this ticket's Security
Auditor, from three synthetic tags, and by the Worker, from a test that went red naming the wrong
guard). While the `symlink_metadata(dst).is_symlink()` path check ran first, every surrogate was
refused by it, so:

- disabling the tag refusal entirely left **all 2,404 tests green** — it had zero coverage anywhere;
- forcing the shared predicate to a lying `Some(false)` still let nothing through — which reads as
  reassuring and was actually the *symptom*;
- and the fix proposed in review ("a fixture refused by the tag check and nothing else") **could not be
  built at all** while the ordering stood.

**The tell to look for elsewhere in this crate:** a sabotage that leaves the suite green *and* a
fault-injection that changes no behaviour, on the same guard. Taken separately each looks like evidence
of safety; together they mean the guard is unreachable, and the next question is which earlier check is
shadowing it. `batch_media::open_output_verified` has the same shape (a path check and a handle check
answering about links at one name) and has not been examined for this.

**The resolution here was to reorder, not to delete**, because the two guards are not redundant — they
answer the same question by different means, one substitutable after the open and one not — and this
ticket's whole thesis is *ask the handle, not the path*. A path question answering ahead of the handle
checks was the inconsistency; putting it second made the handle check the decider and left the path
check as an independent second net, which the new `cpe_1896_std_is_symlink_tracks_the_name_surrogate_bit`
tripwire now protects against silently disappearing under a future `std`.

## Status 2026-08-27 — the reorder: DECIDED, KEEP, and under narrow review

The Foreman's decision, recorded with its reasoning:

- **It moves in the direction the ticket argues for.** The handle cannot be substituted after the open;
  the path can. A path question deciding ahead of the handle checks was the inconsistency, not the fix.
- **It creates no new window.** Both guards still run before `set_len(0)`, so nothing is written between
  them; on Unix nothing changes at all (`is_reparse_point` is always `false` there); on Windows both
  still refuse and only *which* one reports changes.
- **The alternative was shipping a guard with zero coverage in 2,404 tests** — the defect this repo keeps
  rediscovering, and the reason the 3-attempt cap was overridden to reach it.

Because it is a security-guard reorder landing at round 4, that hunk specifically is out with the
Reviewer and the Security Auditor for a narrow look: is the new ordering sound, does it admit anything
the old ordering refused, and does the S3 sabotage now red for the right reason. Nothing further is
pending from the Worker unless one of them raises something.

## 2026-08-27 — ROUND 5: APPROVED + SEC PASS. The reorder cleared by measurement, and the drift it caused

**PR #1043 is approved by the Reviewer and passed by the Security Auditor.** Both cleared the round-4
reorder by measuring it rather than reading it, and both went further than the question asked.

- **Reviewer, on the window between the two guards.** Audited the whole `open_dst()` → `set_len(0)`
  sequence and confirmed **no write, no truncate, no create, no rename and no path resolution** occurs
  between the handle checks and the path check on any arm — every refusal is a terminal `return Err`.
  It also checked two things neither the Foreman nor I raised: `written` is assigned before the path
  check, but every arm that reaches it returns `Err`, so no caller ever sees a changed value; and
  `remove_file(dst)` gained no exposure, still gated on `created`.
- **Reviewer, on whether the tag check is now load-bearing.** Forced the shared predicate to lie on the
  **reordered** code: **2401 passed / 4 failed**, with the leaf test failing on the *wording* assertion
  and **not** on `is_err` — i.e. the surrogate was still refused, by the second net, exactly as the
  design says. Compare round 3, where the identical experiment left all 2,404 green. The tag check is
  load-bearing at both call sites and pinned at both.
- **Security Auditor, on the cost of the window.** The path question now runs **4.4 µs** later, and it
  is itself ~4× more expensive (**16.2 µs**) than the block now placed ahead of it. Not exploitable in
  either direction: making it fire spuriously unlinks the attacker's own symlink, and making it not
  fire is pointless because the handle check has already refused. It then raced it — flipping `dst`
  between an ordinary file and a symlink continuously — for **1,200 trials, ~25,500 leaf flips, 0
  clobbers, 0 silent successes.**
- **Its judgement, recorded because it is the durable form of the argument:** *putting the unspoofable
  question first and the spoofable one second is the correct ordering on the merits, independently of
  the coverage argument.*

**Running total across four rounds: 2,700 race-probe trials, 0 escapes.**

- **2026-08-27 (Worker) — the reorder caused exactly the drift this review has spent four rounds
  chasing, and all four instances are fixed.** Every one is a comment the reorder itself invalidated:
  two direction words (`is_symlink` "above", "the path check above is the whole defence" — both now
  below), a doubled blank line where the moved block was cut out, and a historical note in the leaf test
  saying a symlink "is refused ~50 lines earlier", which is now ~60 lines *later*. The last one is
  re-tensed rather than re-numbered: it describes the state at the time of the finding, and a line count
  would go stale again on the next edit.

- **2026-08-27 (Worker) — the tripwire now pins three tags, and the third is the one that carried the
  argument.** `cpe_1896_std_is_symlink_tracks_the_name_surrogate_bit` looped `0x2000_1896` / `0x0000_1896`,
  which isolates the surrogate bit and is sufficient for that claim — but the comment cited three tags
  and **`0x1000_1896` was the unpinned one**. It is bit 28 set / bit 29 clear: the exact shape of
  `IO_REPARSE_TAG_CLOUD` (`0x9000001A`), and therefore the tag the whole OneDrive Files-On-Demand
  motivation rests on. Added; it stages and classifies as non-surrogate as expected. (Note it stages
  fine on a *file* despite carrying the directory-capable bit, unlike `0x3000_1896`, whose refusal to
  stage remains an observed limit with cause undetermined.)

- **2026-08-27 (Worker) — the user-visible sentence for the commonest case: CHANGED, deliberately.**
  The Auditor flagged that the reorder moves a plain symlink at the leaf — the shape a user is most
  likely to hit — from the path check's sentence to the tag check's, and the inherited one opened on
  "reparse point": precise, but the wrong first word for a symlink. The Foreman grepped `src/`,
  `src-tauri/`, `crates/` and `gui-smoke/` for downstream matchers on the old text and found none on
  this path (the other hits are `archive.rs`, `batch_media.rs` and `split_join.rs` with their own
  distinct strings), so it was safe to change and it is changed rather than left:

  > this name is a link that stands in for another name — a symlink, junction or mount point — and a
  > backup never writes through one: a link's target can be re-pointed after any check

  It opens on "a link", keeps the **"stands in for another name"** clause the tag check is named for
  (and which the leaf test asserts on to prove *which* guard fired), and restores the re-pointing
  explanation the old path-check sentence carried and the new one had dropped. Deliberate, not
  incidental — recorded here because the Foreman asked for the decision either way.

- **2026-08-27 (Worker) — the shadowed-guard finding is filed as CPE-1929**, with the
  `batch_media::open_output_verified` lead recorded as a **lead, not a finding** — it has the same
  shape (a path check and a handle check answering about links at one name) and has not been run
  through the two-sabotage check. The diagnostic tell (a green sabotage **and** a no-op fault injection
  on the same guard) is in the sprint history.

- **2026-08-27 (Worker) — round 5 guardrails.** `cargo test` in `crates/server`: **2405 passed, 0
  failed, 10 ignored**, all 11 targets green. `cargo clippy --all-targets -- -D warnings` clean in
  plain, `--features index` and `--features specta`. Comment-and-wording changes plus one extra loop
  iteration; no control flow touched, no new dependency, no `specta::Type` touched, nothing
  machine-global added.

## What the atomic half should build first

A `#[cfg(test)]` synchronous injection hook between the containment check and the destination open, so a
test can perform the swap with no thread and no timing. That converts this whole class from a
probability into a certainty and is the only way to test a between-two-syscalls swap deterministically.

And read **CPE-1915** before choosing an approach: `GetFinalPathNameByHandleW` / `F_GETPATH` on the open
handle is strictly better than identity — it answers containment directly rather than by proxy, works on
volumes that report no usable file index, and would close **CPE-1912** for free by letting the handle's
real path be compared against the *plan* path rather than only against the root.

Related, all filed from this work: **CPE-1912**, **CPE-1913**, **CPE-1915**.
