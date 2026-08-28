---
id: CPE-1952
title: the catalog staging dir is a predictable path outside the project, and `create_dir_all` succeeds straight onto a pre-existing junction
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`do_fetch_catalog` stages downloaded catalog content in `temp_dir()/cpe-catalog-stage-<pid>`. Three
properties compound:

- **predictable** — `temp_dir()` plus a pid, both guessable by a local process;
- **outside the project**, so none of the containment work of CPE-1896 / CPE-1913 / CPE-1937 covers it;
- **`create_dir_all` succeeds onto a pre-existing junction**, so a local attacker who plants one at
  that path before the fetch has the staged writes land wherever it points.

Raised by PR #1058's Security Auditor as pre-existing and untouched by that PR, and re-raised by PR
#1063's worker, which judged it out of scope there because it is a **different attacker model** —
local filesystem write, no signing key — and the remedy is containment machinery rather than a
validation rule.

## Two corrections to the obvious plan

The Foreman initially suggested reaching for `open_beneath`. PR #1063's worker checked and both halves
of that suggestion were wrong at the time:

1. **`open_beneath` is `pub(crate)` inside `cpe-server`.** `do_fetch_catalog` lives in `src-tauri`, so
   it **cannot reach it at all** without an export decision that is its own piece of design.
2. **`remove_file_beneath` did not exist** when that was suggested — it lands with CPE-1937 / PR #1059.

So this is not a five-line change. Whoever takes it should decide the seam first: export a narrow
containment API from `cpe-server`, move the staging logic into `cpe-server`, or contain it without
`open_beneath` at all.

## Acceptance criteria

- [x] **Demonstrate it first.** Plant a junction at `temp_dir()/cpe-catalog-stage-<pid>` before a
      fetch and show the staged bytes landing outside. Assert on the **filesystem** — where the bytes
      ended up — not on a verdict. If it turns out something upstream already prevents this, record
      that and close the ticket honestly rather than fixing an imaginary bug.
- [x] Decide the seam, and record it: does `src-tauri` get a narrow containment API exported from
      `cpe-server`, does the staging move into `cpe-server`, or is it contained another way? This
      decision is most of the ticket.
- [x] Prefer a **freshly created, exclusively owned** staging directory over a predictable one —
      create-new-or-fail rather than `create_dir_all`, so an existing entry at that path is a refusal
      rather than something to write through.
- [x] Clean up on every exit path, including refusal. The current code `remove_dir_all`s staging on a
      verify failure; keep that property, and make sure the cleanup cannot itself follow a link out
      (`remove_dir_all` on a junction is exactly the CPE-1937 family).
- [x] **Red-proof by racing it**, not by reading it. The containment work this belongs beside
      (CPE-1896, CPE-1913, CPE-1937) all found that static fixtures understate the problem by one to
      two orders of magnitude — CPE-1937's static case showed 1 destroyed file where the race showed
      141 per 200 trials.
- [x] Check whether any **other** temp-dir staging path in the app has the same shape. Enumerate
      rather than recall (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1058's Security Auditor (residual 3) and PR #1063's
worker, which supplied both corrections above.

Family: **CPE-1896** (the handle-gate origin), **CPE-1913** (the containment gates), **CPE-1937**
(`remove_file_beneath`, PR #1059), **CPE-1940** / **CPE-1949** (the catalog trust engine this staging
serves).

## Work Log

### 2026-08-27 — reproduced, then removed the staging directory entirely

**1. Demonstrated first, on the filesystem, on both platforms.** A standalone repro ran the three
lines transcribed out of `do_fetch_catalog` after planting a link at
`temp_dir()/cpe-catalog-stage-<pid>` — a **junction** on Windows (`mklink /J` semantics, no admin
rights needed) and a **symlink** on Linux, on a real **ext4** path, not `/mnt/z`:

```
ARM A create_dir_all: OK, staging = .../temp/cpe-catalog-stage-34500
ARM A ON-DISK: .../victim/index.json exists = true
ARM A ON-DISK bytes: "STAGED-CATALOG-INDEX"
```

Same result on ext4 (pid 9784). `create_dir_all` walked the reparse point, the staged bundle landed
in the attacker's directory, and the code returned `Ok`. Assertions are on where the bytes ended up,
never on a verdict.

**Field evidence that the path really is predictable:** the developer machine's real `%TEMP%` holds
**9 leftover `cpe-catalog-stage-<pid>` directories** — eight dated 2026-07-14 to 2026-07-19, and one
dated **2026-08-27 18:29** carrying a real 155-byte `index.json`, i.e. the shipped app was still
leaking staging *while this fix was under review*. They are the `?` early-return paths. The attacker
never had to guess anything; the names are sitting there. (The count was recorded as 8 in this log's
first draft; the reviewer recounted and found 9. Corrected here rather than left to be re-derived.)

**How strong the "shared namespace" half is, stated separately per platform** — the round-1 text
inherited one severity for both, and they are not the same:

* **Unix** — `/tmp` is genuinely a *shared* namespace: any local principal can create
  `/tmp/cpe-catalog-stage-<pid>` ahead of the fetch. This is the full-strength version of the threat.
* **Windows** — `std::env::temp_dir()` resolves to the **per-user** `%LOCALAPPDATA%\Temp` (confirmed
  empirically: the leaked directories above are all under `C:\Users\...\AppData\Local\Temp`), so
  the attack needs a process already running as the same user. Predictability is unchanged; the set
  of principals who can exploit it is smaller.

The verdict is unchanged either way — the fix removes the path on both — but the next reader should
not inherit the Unix severity for the Windows case.

**2. Two things the ticket expected that turned out not to be true.** Recorded rather than "fixed":

* **The cleanup leg is not a CPE-1937 destroyer.** `remove_dir_all` on the planted link deleted **the
  link**, leaving the victim directory and both its files intact (`victim entries 2 -> 2`,
  `victim dir still there = true`) on Windows *and* on ext4. Rust's std has refused to recurse into a
  reparse point / symlink at the top since 1.77.
* **`open_beneath` cannot fix this**, and not only for the two reasons the ticket names (it is
  `pub(crate)` in `cpe-server`; `remove_file_beneath` was new). Its own module doc disqualifies it:
  *"It does not defend the root itself. The caller resolves the root once and passes it in; if the
  root was already a link to somewhere unexpected, every write goes there and this module agrees."*
  The component under attack **is** the root. `create_beneath` contains what is below an
  already-trusted root; the defect is the root's own creation.

**3. The seam decision — none of the three options in the ticket. The staging directory is deleted,
not defended.** `sidecar_host::catalog` grows a `BundleSource` trait with two implementations:
`MemBundle` (a `BTreeMap` — no filesystem at all) and a private `DirBundle` (the old on-disk shape,
still backing the published `apply_bundle_at`, so `catalog_republish_downgrade.rs` and the
release-side round-trips are untouched). `apply_bundle_with` now reads from a `&dyn BundleSource`;
the new public `apply_bundle_source_at` is the memory entry point. `do_fetch_catalog` assembles the
bundle in memory — every `catalog_http_get` already returned a `Vec<u8>`; the only reason the bytes
ever hit the disk was that the apply engine could not read them from anywhere else — and calls
`apply_bundle_source_at`. No `temp_dir`, no `create_dir_all`, no `create_dir`, no `remove_dir_all`,
no `fs::write`.

*Why this rather than a hardened staging directory* — an unguessable name plus create-new-or-fail
was the drafted fix, and it measures correctly (`create_dir` onto the planted junction is
`AlreadyExists`, code 183 on Windows / 17 on Linux, with zero bytes escaping). Three reasons to
delete instead, **strongest first**:

1. **Unverified bytes off the wire never reach the disk at all.** This is a *new* property: the
   hardened-directory design would not have provided it in any form — it would still write attacker-
   supplied content to a real directory and only then ask the trust gate about it.
2. **It ends the leak** the 9 stale directories evidence. Nothing is created, so nothing can be left
   behind on an early return; there is no cleanup path to get wrong.
3. Leaf writes under a hardened directory would still resolve **by path**. This is the weakest of the
   three and was wrongly leading in the first draft: a directory created by `create_dir` with default
   permissions is **not** writable by another principal on either platform, so this is a
   defence-in-depth argument about a narrowed window, not a live hole. Stated at its real strength.

Underneath all three: **a directory that is never created cannot be redirected**, and that needed no
new containment machinery at all.

**4. Properties the fix actually provides**, stated plainly:

| property | provided? |
|---|---|
| No predictable path in a shared namespace | **yes** — there is no path |
| No `create_dir_all`, so nothing to write through | **yes** — no directory is created |
| No cleanup that could follow a link out | **yes** — nothing to clean up; the `MemBundle` drops with its scope |
| Unverified bytes never reach the disk | **yes**, new |
| The *destination* `catalog_dir(app)` is contained | **no** — see the enumeration; it is still resolved and `create_dir_all`'d by path, which is exactly the question `open_beneath` declines about its own root |

**5. Enumeration, derived not recalled (CPE-1932).** `git ls-files '*.rs'`, excluding `tests/` and
everything after a file's first `#[cfg(test)]`, grepped for
`temp_dir()` / `env::var("TEMP"|"TMP"|"TMPDIR")` — 15 non-test sites — plus `.github/workflows/*` for
`mktemp` / `$RUNNER_TEMP`, and `sidecar/host/src/bin/catalog_sign.rs`:

| site | verdict |
|---|---|
| `src-tauri/src/lib.rs:10313` catalog staging | **the bug — FIXED** (staging removed) |
| `src-tauri/src/lib.rs:10155` `catalog_dir` fallback `temp_dir()/cpe-ai-console-catalog` | **same shape, NOT changed.** It is the *destination*, not staging, and only reached when `app_data_dir()` errors. Making it fail closed changes the catalog subsystem's behaviour on a path nothing here exercises; that deserves its own ticket rather than a drive-by. Recorded as CPE-1952's residual. |
| `src-tauri/src/lib.rs:11837` `temp_dir()/cpe-sidecar-storage` fallback | same shape as the above, same reasoning, same residual |
| `src-tauri/src/lib.rs:14349` `cpe_bindings_export.ts` | **build-time only** (`export_bindings`, dev/CI): writes a generated TS file it immediately re-reads and deletes. Not in the shipped runtime. No change |
| `crates/server/src/archive.rs:1672` extraction session root | **already contained (CPE-1786)**: refuses a link at the shared root on every call, then claims a per-session directory with an exclusive `create_dir` |
| `crates/server/src/ffmpeg_util.rs:113` `create_scratch_dir` | **already contained**: `fs::create_dir` (create-new-or-fail) with a bounded retry — never `create_dir_all` |
| `crates/server/src/fsutil.rs:4457` `ScratchDir::adopt` | test-fixture guard; canonicalises before arming a recursive delete. No change |
| `crates/server/src/fsutil.rs:4606` `scratch_dir(prefix)` | `create_dir_all` on a predictable name, but it is the **test-fixture** helper (`pub` only because `#[cfg(test)]` is per-crate and downstream crates need it from their own test builds). No production caller. No change |
| `sidecar/ai-console/src/console.rs:733,796` swarm mission dir | predictable (`cpe-swarm-<millis>`), `create_dir_all`. **Same shape, different subsystem** — recorded, not changed here |
| `sidecar/ai-console/src/session_diag.rs:33`, `session_supervisor.rs:151`, `sidecar/host/src/reaper.rs:61` | a diagnostic log and a port file under `temp_dir()/cpe-ai-console`, both best-effort and neither security-bearing. No change |
| `crates/net/examples/security_demo.rs:22`, `crates/net/src/bin/cpe-server-ref.rs:27` | example / reference binaries, not shipped. No change |
| `sidecar/host/src/bin/catalog_sign.rs:141` | `create_dir_all(out)` where `out` is the operator-supplied output directory on the release runner — not a temp path, not predictable-outside-project. No change |
| `.github/workflows/*` (`mktemp` x5, `$RUNNER_TEMP` x~25) | ephemeral single-tenant runners — no second local principal — and `mktemp` is unpredictable by construction. No change |

**6. Guards.** `sidecar/host/tests/catalog_staging_containment.rs` — 3 deterministic tests, run by the
`sidecar` CI job on **all three OSes**:

* `the_old_staging_primitive_writes_through_a_planted_link` — the **sensitivity control**. Runs the
  pre-fix primitive against the planted link and asserts the bytes land in the attacker's directory.
  If it ever passes by *not* escaping, the answer is to find out what changed, not to delete it.
* `the_fetched_bundle_never_touches_the_filesystem` — the same scene, the real production entry
  point, a really-signed bundle; asserts the attacker's directory stays empty, the link still has
  nothing behind it, **and** that the catalog installs correctly (so "refuses everything" cannot
  pass).
* `the_memory_arm_still_enforces_anti_rollback` — the refactor did not soften the trust engine.

The link is planted at the **real** `temp_dir()/cpe-catalog-stage-<pid>`, not a stand-in inside a
`tempfile::tempdir()`, because a stand-in is unreachable by any regression of the code under test, so
every assertion about it would be unfalsifiable — safe-looking and worthless at once (CPE-1929). The
two tests share a `Mutex`, and a `Drop` guard cleans the link up on every exit path including a
panic. **A link that cannot be planted is a RED, not a skip** — see round 2 below for the argument
and for the repo guard that forced it.

`src/lib/catalogStagingContainment.test.ts` covers the caller, which needs a Tauri `AppHandle` and a
network fetch and so cannot be reached from a Rust test. It **reads `src-tauri/src/lib.rs`**
(CPE-1933: derive, do not claim), strips Rust comments first — the fix's own explanation names
`create_dir_all` three times, and a raw-text guard would fail on the fix it guards — and asserts that
`do_fetch_catalog` contains none of the five staging primitives *and* still contains
`MemBundle::new()` / `apply_bundle_source_at`, so deleting or renaming the function cannot pass by
vacuity. The stripper is `src/lib/rustSource.ts`'s `stripRustComments` — CPE-1950 had already lifted
it out of `MacroRunConfirm.test.ts` for exactly this reason, so this ticket is its second consumer
rather than a second copy of it. (Landed on main mid-flight; this branch's own extraction was
dropped on the rebase in favour of theirs.)

**7. Red-proof — by attacking, not by reading.**

* *The vitest guard*: reinserted the two deleted lines into `do_fetch_catalog` →
  `x does not call temp_dir`, `x does not call create_dir_all`; reverted → green.
* *The Rust containment test*: put the pre-fix staging back **inside `apply_bundle_source_at`** (in a
  throwaway Linux copy) → `the_fetched_bundle_never_touches_the_filesystem` FAILED with
  `the attacker's directory must stay empty; found ["index.json"]` — on-disk evidence, not a verdict.
  The other two stayed green, so the sabotage was specific.
* *No race harness*, and deliberately. The ticket asked for one because CPE-1937's static fixture
  understated a race. There is no race here to understate: the attack is a **pre-plant** — the path
  was computable before the fetch started, so the attacker never needed to win a window — and the fix
  removes the path rather than narrowing a window. A trial count would be decoration on a
  deterministic result.

**8. Verification.** `cargo clippy --locked --all-targets -- -D warnings` clean for `sidecar/host` on
Windows and on Linux/ext4; `src-tauri` clippy clean in **both** feature modes (`--features
sidecar-platform` and default). `cargo test --locked` green for `sidecar/host` (129 tests). Full
vitest: **348/348 files, 4980 passed, 2 skipped, 0 failed.** (Round 1 reported "4883 passed with 19
pre-existing failures"; that was **wrong** — the reviewer re-measured green, and so did round 2. The
19 were this box's shell, not the tree: the shell-script-executing suites need a POSIX toolchain on
`PATH`, which Git Bash supplies and the shell round 1 ran from did not. A "pre-existing failure"
claim is a claim like any other and this one was never checked against the merge base.)
`npm run check`: 0 errors, 0 warnings. Every
planted junction/symlink cleaned up; `%TEMP%` and `/tmp` verified free of stray staging links
afterwards.

**Residual, stated so it is not mistaken for covered:** `catalog_dir(app)`'s and the sidecar-storage
`temp_dir()` fallbacks, and `ai-console`'s `cpe-swarm-<millis>` mission directory, are the same shape
and are unchanged. Each is a destination rather than staging, and each needs its own decision about
what failing closed costs.

### 2026-08-27 (round 2) — the skip path was the hole, not a nicety

**1. The failure.** `fsutil::tests::skip_notices_never_use_a_captured_print_macro` — an existing repo
guard, not a new test — failed the Windows and macOS legs on the two `eprintln!` skip notices this
file shipped with. The reviewer had independently found the same lines by reading and graded them
*"latent, not active"*; the guard was already running and says otherwise. Both were pointing at the
same hole; the guard was right about the severity.

**2. Decision: `require_staged`'s policy, applied by hand and one notch stricter — not the function.**
The guard's message prefers `fsutil::require_staged` over `skip_notice!`, and that preference is
correct here, but the call itself is unavailable: **`require_staged` lives in `cpe-server`, and a
sidecar may not depend on `cpe-server` (ADR 0001)** — the same rule that has `sidecar/agent-board`
carrying its own copy of the board logic. So the policy travels and the call does not, and it is
written out in `Scene::planted`:

* `plant_dir_link -> bool` became `stage_dir_link -> Result<(), &'static str>`, naming the failing
  step (link creation vs. the link not resolving to the target) — the shape `require_staged_reason`
  exists for, because a red on a runner nobody can log into should say which half broke.
* `Scene::planted()` returns `Self` and **panics** when staging fails. Both `let Some(scene) = … else
  { eprintln!("SKIP: …"); return; }` blocks are gone; there is no skip path left to be silent on.
* **Stricter than `require_staged`**, deliberately: that helper is lenient off CI, for mechanisms a
  developer's environment might legitimately lack (a deny ACE, a root Docker shell). Creating a
  junction in one's own `%TEMP%` is not such a mechanism — no admin rights, no Developer Mode — and
  neither is `symlink(2)` on ext4. There is no environment to be lenient about, so there is no
  `LegitimateSkip` arm and no `CPE_STAGING_STRICT` knob to reproduce.
* **The third platform is gated explicitly rather than sharing the silence.** The
  `#[cfg(not(any(windows, unix)))]` arm now carries a comment saying this crate builds for exactly
  Windows/macOS/Linux and that a fourth needs its own recorded decision — not a shared early return.

Why the stakes are different from an ordinary leg: the first test is the **sensitivity control** for
a security fix. Its whole job is to show the escape still happens with the fix disabled. A control
that returns green because it could not plant its link proves nothing, and proves it invisibly, which
is worse than not having it — the green reads as coverage. That is the same argument that put the
link at the real predictable path rather than a stand-in, taken one step further out.

The `Drop` guard's leaked-junction warning was the one notice worth *keeping*, so it now writes
`writeln!(std::io::stderr(), …)` directly — what `cpe_server::skip_notice!` expands to, longhand for
the same ADR 0001 reason. The emitter is load-bearing: libtest installs its capture inside the print
macros and discards it when the test passes.

**3. Red-proof, both directions, run not read.**

* *The new panic*: forced `stage_dir_link` to report failure → both link-planting legs **FAILED**
  with `[CPE-1952] … could not be planted, so this leg verified NOTHING`, naming the failing step,
  the link and the target; `the_memory_arm_still_enforces_anti_rollback` (which plants nothing)
  stayed green, so the sabotage was specific. **The same sabotage against the round-1 file produced
  two passing tests and no output** — that is the whole defect, measured.
* *The guard*: reinserted one `eprintln!("SKIP: …")` into this file → `fsutil::tests::
  skip_notices_never_use_a_captured_print_macro` **FAILED**, naming the line; removed it → **ok**
  (`1 passed; 0 failed; 2435 filtered out`). The guard genuinely re-reads the tree, so its green is
  worth something.
* *A defect the red-proof itself exposed*: the sabotaged run **leaked a live junction** in `%TEMP%`,
  because the panic fires before the `Scene` exists and so before its `Drop` guard is armed — and
  `stage_dir_link`'s second arm can fail with the link already created. The panic path now
  `remove_dir_all`s the link first. Found by running the sabotage, not by reading the code; it would
  have shipped otherwise, in the file whose subject is stray junctions in the shared temp directory.

**4. F5, the unremarked memory trade — capped rather than argued away.** The bundle is now held in
RAM in full where each blob was previously written and dropped, so the peak is the *sum* of the
responses rather than the largest one, and `catalog_http_get` had no size cap on `read_to_end`.
Added `CATALOG_MAX_ASSET_BYTES = 8 MiB`, enforced with `take(cap + 1)` so reaching the cap is an
error and never a silent truncation (a truncated asset would fail signature verification with a
message about the *key*, which is a worse failure than a large one). The total is bounded by two
facts now stated at the site: each response is capped, and the *number* of responses comes from
`VerifiedIndex::open`, so only an index signed by a trusted key can name entries to fetch — a wire
attacker can make each response big, up to the cap, but cannot make there be more of them.

**5. F1 — the enumeration recipe, corrected so it can actually be re-run.** The count (15) and the
site list in round 1's section 5 are right, but the recipe as written does not reproduce them. Two
defects, both found by running it rather than reading it:

* *"everything after a file's first `#[cfg(test)]`"* also matches the **indented** in-function form
  and doc comments that quote the string, so it truncates production files early and amputates real
  sites. The working rule is **"after the first *column-0* `#[cfg(test)]`"**.
* the `temp_dir()` grep must be applied to **code only**. Three of this tree's `temp_dir()` mentions
  are in doc comments explaining the very defect (`archive.rs`'s CWE-377 note, `fsutil.rs`'s
  `de_verbatim`, and `catalog.rs`'s own `BundleSource` doc), and counting them inflates the total.
  The fix's explanation is itself one of them, which is the same trap `catalogStagingContainment.
  test.ts` already strips comments to avoid.

Both applied, over `git ls-files '*.rs'` minus `tests/`, the recipe reproduces exactly:

| rule | at the merge base | on this branch |
|---|---|---|
| first `#[cfg(test)]` **anywhere** (as written) | 10 | 9 |
| first **column-0** `#[cfg(test)]`, comments stripped (corrected) | **15** | **14** |

The branch is one lower on both rows for the right reason — `src-tauri/src/lib.rs`'s staging call is
the site this ticket deleted; what is left at that line is the comment explaining its absence, which
the comment strip correctly declines to count. The five sites the as-written rule silently drops are
`fsutil.rs:4606`, **both** `console.rs` `cpe-swarm` sites, and two `src-tauri/src/lib.rs` fallbacks —
i.e. it loses the residual that became CPE-1964. Recorded at this length because CPE-1932 is about
deriving rather than recalling, and a derivation nobody else can re-run is halfway back to recall.

**6. F2 / rejection ordering / leak count.** All three folded into the round-1 sections above rather
than left as corrections at the end: the threat model now states the Unix and Windows halves
separately (Windows `temp_dir()` is the **per-user** `%LOCALAPPDATA%\Temp`, so that half needs a
same-user process); the rejected alternative's three reasons are reordered strongest-first, with the
by-path one demoted to defence-in-depth and marked as such; and the leak count is **9**, not 8.

**7. Not fixed here, by direction.** The **55** leaked `cpe-swarm-<millis>` directories — the
residual leaking roughly six times harder than the site this ticket fixed — are **CPE-1964**.

**8. Verification (round 2).** `cargo clippy --locked --all-targets -- -D warnings` clean for
`sidecar/host` and for `src-tauri` in **both** feature modes (`--features sidecar-platform` and
default). `cargo test --locked` green for `sidecar/host` (119 lib + all integration suites);
`catalog_staging_containment` 3/3. `crates/server`'s
`fsutil::tests::skip_notices_never_use_a_captured_print_macro` — **the guard that went red** — now
**ok** (`1 passed; 0 failed; 2435 filtered out`), and red-proofed in both directions above. Full
vitest **349/349 files, 4995 passed, 2 skipped, 0 failed** (on this branch rebased onto `origin/main` 8a778dc7); `npm run check` 0 errors, 0 warnings.
Every planted junction cleaned up, including the one the sabotage leaked: `%TEMP%` is back to the 9
pre-existing stale staging directories and no links.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1075, after two rounds.

**It deleted the vulnerable seam rather than defending it.** The catalog staging directory was a
predictable path outside the project that `create_dir_all` follows a junction into; reproduced with
on-disk evidence on **both** platforms (junction on Windows, symlink on real ext4) —
`"STAGED-CATALOG-INDEX"` written into the attacker's directory with the code returning `Ok`. The fix
assembles the bundle **in memory**: no path formed, no directory created, no cleanup to follow a link
out, and — the property worth the most — **unverified bytes off the wire never reach the disk at all.**
Its Reviewer traced the whole path and confirmed the ordering in both directions: index bytes enter the
bundle only after `VerifiedIndex::open` succeeds, and each manifest reaches `write_entry` only on
`EntryVerdict::Accept`.

**It disproved two of the ticket's own assumptions.** `remove_dir_all` on a planted link removes **the
link**, not the target's contents (2 entries before, 2 after, verified independently with a standalone
probe) — so the cleanup leg was never the destroyer the ticket feared. And **`open_beneath` cannot fix
this**: its own doc says it does not defend the root itself, and if the root was already a link every
write goes there and the module agrees. **Here the root is what is under attack** — disqualified by its
own contract, not by visibility.

**It rejected a design that measured clean.** Unguessable-name plus create-new-or-fail scored 183/17
with **zero escapes**. It chose the more invasive fix anyway, because leaf writes would still resolve by
path in a `readdir`-watchable directory — the opposite of the usual failure mode. Its Reviewer judged
that lead argument the weakest of the three and the other two decisive; the reordering is in the record.

**Field evidence nobody was looking for:** **9** leaked `cpe-catalog-stage-<pid>` directories in the real
`%TEMP%`, one dated **the same evening** with a live 155-byte `index.json` — the shipped app still
leaking while the fix was under review. Closed in the strongest available way, since nothing is written
at all. The sweep also found **55** leaked `cpe-swarm-<millis>` directories — the residual this ticket
deferred is leaking **six times harder** than the site it fixed, while the two fallbacks it discussed at
length have never been reached on this machine. Filed as **CPE-1964**.

**An existing repo guard out-graded a careful reviewer.** That Reviewer raised the `eprintln!` skip
notice as **"latent, not active"**; `fsutil::tests::skip_notices_never_use_a_captured_print_macro`
**failed the Windows and macOS legs** on exactly it. The test is the **sensitivity control** for a
security fix — one that returns green because it could not set itself up proves nothing, *invisibly*.

**Round 2 refused the obvious fix for a good reason.** It could not call `require_staged`: that helper
lives in `cpe-server`, and **a sidecar may not depend on `cpe-server` (ADR 0001)**. So it took the policy
without the call — the planting helper returns a `Result` naming the failing step, the scene **panics**,
and no skip path remains. Stricter than `require_staged` deliberately: a junction in one's own `%TEMP%`
needs no privileges, so there is no environment to be lenient toward.

**And its own red-proof found a new defect:** sabotaging the planting **leaked a live junction in
`%TEMP%`**, because the panic fires before the scene exists and therefore before its `Drop` is armed. In
the file whose entire subject is stray junctions in the shared temp directory. *"Found by running the
sabotage, not by reading — it would have shipped."*

**It corrected the enumeration recipe twice**: column-0 **and** comments-stripped, because three
`temp_dir()` mentions in this tree live in doc comments explaining the defect — including the fix's own.
As written the rule finds **10** sites; corrected, **15**. The five it drops are exactly the residual
that became CPE-1964.

**Merged past two verified reds** — shard 2 (CPE-1960) and its verdict job — after proving by
`git cat-file` that this branch predates that fix.
