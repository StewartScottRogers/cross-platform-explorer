---
id: CPE-1717
title: Every "loud skip" notice is invisible under CI, so a platform leg can silently cover nothing
type: bug
priority: High
status: Doing
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #895 (CPE-1710) reviewer, 2026-08-13. **This undermines a pattern this sprint has been
adding deliberately across several tickets, so it matters more than its size suggests.**

Several tests stage a condition that cannot always be created — a denied stat, a symlink on an
unprivileged Windows runner — and, when staging fails, print a **loud skip notice** via
`writeln!(stderr)` rather than passing silently. CPE-1705 introduced the pattern; CPE-1710 copied it. The
whole point is that a test must never degrade into proving nothing while still showing green.

**libtest captures stderr for passing tests, and CI never asks for it.** `.github/workflows/ci.yml` runs
`cargo test` for `crates/server` with **no `--nocapture`** — grep confirms `--nocapture` appears only on
the `--ignored` network and keyring legs.

So the sequence is:

1. A Windows runner loses both symlink privilege and junction creation (or an ACL deny stops taking effect).
2. The tests stage nothing, print their notice to a captured stderr, and **pass**.
3. The leg reports green. **Nobody ever sees the notice.**
4. That platform's coverage is zero, and the dashboard says otherwise.

This is precisely the failure mode the family "has spent six rounds on", written into the doc comments as
solved. The claim — *"a loud `writeln!(stderr)` skip, never a silent pass"* — is **true locally and false
under the harness that actually runs it.**

## Scope

`.github/workflows/ci.yml`'s `cargo test` invocations, and every skip notice added by **CPE-1705** and
**CPE-1710** in `crates/server` and `src-tauri`.

## The decision to make

Two shapes, and the second is stronger:

- **(a) Add `--nocapture` to the CI test steps.** One-line, makes every notice visible. Cost: it also
  un-captures every other test's output, which is noisy on a 2,100-test suite and may bury the thing you
  are looking for.
- **(b) Make an uncoverable platform leg *fail* rather than skip**, at least on the platforms where the
  staging is supposed to work. A skip is right on a platform where the condition genuinely cannot exist
  (an ACL test on Linux); it is wrong on Windows, where failing to stage means the runner changed under us
  and we should find out loudly.

(b) is closer to the sprint's own standard: a test that cannot verify its subject should be **red**, not
quiet. (a) may still be worth doing alongside for the genuinely-not-applicable cases.

There may be a third option worth considering: have the skip path record into a machine-readable artifact
the CI step checks, so a skip is visible without un-capturing everything.

## Acceptance criteria

- [ ] A test that cannot stage its condition on a platform where it is *supposed* to work is **visible in
      CI** — either red, or with the notice actually reaching the log. Prove it: force the staging to fail
      and show what CI reports.
- [ ] Enumerate every skip notice currently in the tree (CPE-1705 and CPE-1710 at minimum) and state, for
      each, which platforms it may legitimately skip on and which it must not.
- [ ] The `#[cfg(not(windows))]` skip notices that are *correctly* skipping — an ACL test on Linux — keep
      working and do not become noise or false failures.
- [ ] Record the choice and the reasoning. If `--nocapture` is rejected for noise, say so; if it is
      accepted, check it does not push the log past any size limit on the 3-OS matrix.
- [ ] Breaking the mechanism turns a **distinct** test or CI step red, per the Evidence Rules in
      `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #895 review, 2026-08-13.

**High** despite being small: it silently voids coverage the last two tickets were specifically built to
add, and it does so on the platform (Windows) where most of this family's bugs actually live. Every
"assert your own premise" test added this sprint is only half-working until this is fixed — the assertion
fires, and then nobody hears it.

Related: **CPE-1705** (which introduced the pattern), **CPE-1710** (which copied it), **CPE-1694** (an
earlier instance of tests that never gated CI at all).

---

## Work Log

### 2026-08-13 — measured the premise first, and it was half wrong

The ticket says libtest swallows the notices. **Measured, before writing any code**, with a one-test
harness compiled by `rustc --test` (the same libtest harness `cargo test` runs) and run with **no
flags at all**:

```text
running 1 test
VIA-WRITELN-STDERR: this is the CPE-1705/1710 shape
VIA-WRITELN-STDOUT: control
test passing_test_that_announces_a_skip ... ok
```

The test also called `eprintln!("VIA-EPRINTLN: …")` and `println!("VIA-PRINTLN: …")`. Both are
**absent**. libtest's capture is installed inside the `print!`/`eprint!` macros (a thread-local swap),
so a direct write to the process's stderr handle goes around it.

So the tree contains **two eras** and they behave oppositely:

- **`writeln!(std::io::stderr(), ..)`** — CPE-1678 onwards, including every CPE-1692/1696/1705/1710
  notice. **Already reaches the CI log.** `dispatch.rs`'s comment calling that choice "load-bearing"
  is correct.
- **`eprintln!(..)`** — the older era. **56 sites. Genuinely invisible**, exactly as the ticket
  describes. The largest concentration is `vault_manager.rs` (23 notices covering the vault
  symlink/hard-link security family).

That reframes the fix. Visibility was never the whole problem for the family the ticket names — a
*passing* leg with a notice inside a 2,100-test log is a green board over zero coverage, and nobody
reads a green log. So: **shape (b)**, plus a targeted piece of (a)'s intent for the invisible era.

### The decision

**Chosen: (b) — a leg that cannot stage its condition on a platform where the mechanism is supposed to
work goes RED under CI.** Rejected: `--nocapture`.

- `--nocapture` **rejected**, and on evidence rather than taste: it would add *nothing* to the notices
  this ticket is about (measured above — they already print), while un-capturing 2,100 tests' output on
  three OSes and burying the very notices it was meant to surface. Its one real benefit — making the
  legacy `eprintln!` era visible — is obtained instead by converting those 56 sites, which costs no log
  volume at all.
- Strictness is scoped to **CI**, not to platform alone. CI is where the board lies; a developer may
  legitimately be in a root Docker shell, on a network share, or on a Windows account with neither
  Developer Mode nor a working junction fallback, and must keep the loud-skip behaviour there.
  `CPE_STAGING_STRICT=1`/`=0` overrides in both directions, so the escape hatch exists if a runner
  image regresses before a fix can land.
- The third shape (a machine-readable artifact a CI step checks) was considered and dropped: it adds a
  file format, an upload and a parser to arrive at the same place a panic already reaches, and it still
  leaves the leg reporting green.

### Enumeration — every skip notice in the tree, and where it may legitimately skip

79 `writeln!(stderr)` notices + 56 `eprintln!` notices. Grouped by the **staging mechanism**, because
that is what decides the verdict; per-site verdicts follow the group.

| # | Mechanism | May skip on | Must NOT skip on | Now enforced? |
|---|-----------|-------------|------------------|---------------|
| A | `fsutil::deny_stat_of` — deny `(R)` on target + `(RD)` on parent (Windows) / `chmod 0o000` parent (Unix) | nowhere | **Windows, Linux, macOS** | yes — `require_staged("deny_stat_of", true, ..)` |
| B | `fsutil::deny_dir_traversal` — traversal deny for `fs::metadata` sites | **Windows** (measured in PR #874: `CreateFileW` with access mask 0 + Everyone's "Bypass traverse checking" defeats it) | **Linux, macOS** | yes — `supported_here = cfg!(unix)` |
| C | `fsutil::make_dangling_link` — symlink, else NTFS junction | nowhere (the junction fallback needs no privilege) | **Windows, Linux, macOS** | yes |
| D | `dispatch::deny_read` — `(RD)` on Windows / `chmod 0o000` on Unix, unreadable-but-stattable | nowhere | all three | yes |
| E | compile-time `#[cfg(not(windows))]` / `#[cfg(unix)]` "on this platform" arms | the platform named in the `cfg` — permanently | n/a | n/a — no runtime staging to enforce; left exactly as-is |
| F | per-site staging not yet routed (`split_join::make_unstattable`, `organize_apply`'s live-symlink leg, `vault_crypto::promote`'s directory link, `src-tauri`'s inline dangling-symlink rename) | unverified | unverified | **no** — loud skip only; see "Left open" |
| G | the `eprintln!` era (56 sites): symlink/hard-link creation, `ffmpeg`/`pdfium`/`getfattr` absence, OS trash round-trip | varies by site; most are genuine capability checks | — | visibility only: converted to `skip_notice!` |

**Group A — must never skip (23 sites).** `batch_execute.rs:2034,2111`; `batch_media.rs:2369,2468,2546`;
`copilot.rs:625`; `disk_usage.rs:237`; `folder_template.rs:356`; `index_watch.rs:612`;
`native_meta.rs:337`; `organize_apply.rs:306,430`; `snapshot_capture.rs:837,913,991`;
`split_join.rs:468`; and the inline duplicates `src-tauri/src/lib.rs` ×4 (the CPE-1696 `do_move_into`
parent-`RD` premise and target deny, CPE-1705 `rename_entry`, CPE-1705 `move_exact`, CPE-1692
`move_exact`) and `crates/sftp/src/lib.rs` ×2 (`open`, `open`'s own handler).

**Group B — Windows-legitimate (3 sites), and only two of them are actually "may skip on Windows".**
`links.rs:284` and `crates/sftp/src/lib.rs` (`opendir`) compile on Windows, run there, and skip
loudly. `dangling_links_scan.rs:343` sits inside a **`#[cfg(unix)]` test**, so it never compiles on
Windows at all — it is not a Windows skip, it is a Windows absence, and the first version of this
table said otherwise. (Corrected out of the PR #898 round-2 review.) These are the "ACL test on Linux"
case the acceptance criteria protect: they must stay quiet skips where the mechanism cannot work and
must **not** become false failures there. Covered by the `(false, false, true, false) →
LegitimateSkip` row of the policy table, and — since round 2 — proved in CI by running `links.rs`'s
leg sabotaged and requiring **red on Linux/macOS, green on Windows** in the same step.

**Group C — must never skip (8 sites).** `copilot.rs:677,722`; `organize_apply.rs:362`;
`vault_crypto.rs:667`; `fsutil.rs`'s own `rename_slot_refusal` test; `src-tauri/src/lib.rs`
`cpe_1710_{rename_entry,move_exact,board_move}_*`.

**Group E — permanently legitimate (15 sites).** `batch_execute.rs:2082`;
`batch_media.rs:2340,2430,2512`; `copilot.rs:601`; `folder_template.rs:334`;
`organize_apply.rs:268,405`; `snapshot_capture.rs:814,877,958`; `split_join.rs:440`;
`src-tauri/src/lib.rs:15248,15601,15836`. These sit inside `#[cfg(not(windows))]` arms and describe a
mechanism that cannot exist on that platform at all — no staging is attempted, so there is nothing to
enforce and nothing that could become noise.

### What was built

- `fsutil::staging_verdict` — the policy as a **pure** function of (supported-here × staged × strict ×
  sabotaged), so it is table-tested without env-var races.
- `fsutil::require_staged` — the one gate every staging attempt passes through. `#[track_caller]`, so a
  red build names the **call site's** file and line, not the helper's.
- `fsutil::staging_is_strict` (`CI`, overridable) and `fsutil::staging_is_sabotaged`
  (`CPE_STAGING_SABOTAGE=1`) — the sabotage hook exists so the guard can be **neutralised on demand**
  and shown to bite, per the Evidence Rules.
- `skip_notice!` macro + 56 `eprintln!` → `skip_notice!` conversions, and the
  `skip_notices_never_use_eprintln` scan that fails the build on a new one.
- Two CI steps (`crates` job and `backend` job, all three OSes) that run a filtered `cargo test` with
  staging sabotaged and **fail if the tests pass**, then verify the failure carries `CPE-1717` so a
  failure for some unrelated reason cannot be mistaken for evidence.

### Left open — and the scope of that claim, stated properly

Group F is not routed, and **the first version of this paragraph named three sites when the real
population is 44**. An independent audit counted every staging site still emitting a notice rather
than a verdict; "three" was the number I had inspected, not the number that exists, which is precisely
the unscoped negative Evidence Rule 1 is about. Two the audit called out by name:

- `src-tauri/src/lib.rs:15805` hand-rolls a dangling symlink with **no junction fallback**, while its
  three siblings at 15460/15494/15542 call the routed `make_dangling_link`, which has one. So on a
  Windows runner without Developer Mode that one leg skips where its siblings stage — an inconsistency,
  not a measured decision.
- `copilot.rs:805` (`make_dir_link`, junction-based, privilege-free) is structurally the class routed
  as `supported_here = true`.

Routing 44 sites here would be a different, much larger piece of work, and routing any of them on a
guess risks the false red this ticket's own acceptance criteria forbid. **The whole bucket, with the
44-site count, is CPE-1724.** Nothing regresses meanwhile: all 44 already use the capture-proof
emitter, so their notices reach the log — they are simply not yet consequential.

### 2026-08-13 — round-2 review (PR #898), two blockers fixed

The reviewer reproduced the central finding independently (its own `rustc --test` harness, then a real
Windows runner's raw logs) and then found the ticket's own disease in the fix:

1. **`staging_is_strict` was tested against a hand-written copy of its own `match`.** The reviewer
   inverted the real function completely and every CPE-1717 test still passed. Worse, breaking only
   the `_ => var_os("CI")` arm turned an ordinary broken-staging CI run **green over zero coverage**
   while the guard step still reported OK — because the step pinned `CPE_STAGING_STRICT=1` and routed
   around the very arm that had broken. Fixed by extracting the pure `fsutil::strict_from(var, ci)`
   and table-testing *it* (the `None` rows included), and by **removing** the pinned override from both
   CI guard steps so they now depend on `CI` the way a real run does.
2. **The `eprintln!` scan missed the tree's dominant notice shape.** `eprintln!("[CPE-1692] SKIPPED …")`
   — 56 sites use it — plus sentence-case `"Skipping"`, a leading space, an aliased macro, and
   `eprint!` entirely. Fixed: case-insensitive `contains("skip")` on the literal, all four captured
   macros, an alias check, scoped to test code so production logging that mentions skipping is not
   cry-wolf flagged. The stated caveat now matches what it actually misses.

Also: the `staging_verdict` table is now all 16 rows rather than 10; `CPE_STAGING_STRICT` accepts
`true`/`false`/`yes`/`no`/`on`/`off` and **panics** on an unrecognised value instead of silently
falling through; `dispatch::deny_read` gained the `#[track_caller]` its three siblings had; and the
`wiki.md` edit gained a forward pointer from rule 2's `--nocapture` bullet to §3 and a scoped
statement of what the scan does not catch.
