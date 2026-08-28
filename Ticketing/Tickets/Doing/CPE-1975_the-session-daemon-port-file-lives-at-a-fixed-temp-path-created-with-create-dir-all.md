---
id: CPE-1975
title: the session-daemon **port file** lives at a **fixed** `<temp>/cpe-ai-console/` path created with `create_dir_all` — redirect it and the console talks to the attacker's "daemon"
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by CPE-1964's worker while re-deriving the `temp_dir()` enumeration. Three sites build the
**same fixed** path; **two** of them create it with **`create_dir_all`**, and the third only reads
and deletes (corrected in CPE-1964 round 2 — the original wording put `create_dir_all` on all three):

- `sidecar/ai-console/src/session_diag.rs:33` builds it; `:52` creates it with `create_dir_all`
- `sidecar/ai-console/src/session_supervisor.rs:151` builds it; `write_port_file` at `:144` creates it
  with `create_dir_all`
- `sidecar/host/src/reaper.rs:61` builds it — **no `create_dir_all` here**; `reap_orphan_session_daemons`
  only tests `port_file.exists()` and `remove_file`s it (`:79`). It is still in scope: it is a *reader
  and deleter* of the same redirectable path, so a planted link makes it consult — and unlink —
  something inside the attacker's directory.

`create_dir_all` is the primitive CPE-1952 established will **follow a junction/symlink into an
attacker-chosen directory**, and this path is **not even timestamped** — `cpe-swarm-<millis>` was at
least guessable-within-a-window; `cpe-ai-console` is a constant.

**What makes it worse than the two leaks fixed alongside it:** that directory holds the session-daemon's
**port file**. CPE-1952's catalog staging leaked *data*; CPE-1964's mission directory leaked *scaffolding*.
**This one is a control channel.** An attacker who redirects the directory controls the port file, and
the console then connects to **their** "daemon" instead of the real one.

## Threat model, stated in both halves

Per the correction CPE-1964 carries: on **Windows**, `std::env::temp_dir()` resolves to the **per-user**
`%LOCALAPPDATA%\Temp`, so the Windows attack needs a same-user process. *"Predictable path in a shared
namespace"* is fully true of **Unix `/tmp`**. Both halves are real; do not collapse them.

## The fix shape is already established in-tree — read it before designing

**CPE-1964 (PR #1086)** solved the same class one file over, and its reasoning transfers:

- **`std::fs::create_dir`, not `create_dir_all`** — one `mkdir(2)` / `CreateDirectoryW`, `AlreadyExists`
  on **anything** at the path including a reparse point, **atomically with the create**, so there is no
  check-then-use window.
- **No `exists()` pre-check.** That would be a shadowed guard; CPE-1964 refused it explicitly and said so
  at the site.
- **CPE-1952's stronger answer — delete the directory — is probably unavailable here too**, since a port
  file is a rendezvous by construction. Say so explicitly rather than leaving it implied, and say what
  hardening does and does not buy: the directory still exists in a shared namespace.

**But a port file is a control channel, so hardening the directory may not be enough.** Consider what
the *consumer* does: does anything verify the daemon it connects to is the one it started? A redirected
port file is only exploitable if the reader trusts the endpoint it names.

## Acceptance criteria

- [ ] **Reproduce the redirect first**, on both platforms, and **assert on the filesystem** — where the
      port file lands — never on a returned verdict. Junction on Windows (`junction::create`, no admin);
      symlink on **real ext4**, not `/mnt/z`. Note `/tmp` on WSL is **tmpfs**: override `TMPDIR`.
- [ ] **Then demonstrate the consequence**, not just the redirect: show a console connecting to a port
      file the attacker placed. If that turns out not to be reachable, **that is a real result** and it
      changes the priority — say so with the evidence.
- [ ] **Keep a sensitivity control** — with the fix disabled the redirect must succeed — as a normal CI
      test on all three OSes, **not `#[ignore]`d**, planting at the **real** path (a stand-in is
      unfalsifiable — CPE-1929). And heed the leg #1075 lost twice: a control that returns green because
      it could not plant its link proves nothing, **invisibly**. Panic; and note **ADR 0001 puts
      `skip_notice!` out of a sidecar's reach**, so write to the real stderr handle with `writeln!` as
      CPE-1964 did.
- [ ] **Run the CPE-1929 pair on every refusal you add** and **write both numbers at the site**.
      CPE-1964's third pair is the reason: an `is_symlink()` arm was written, measured, and **deleted**
      because `!meta.is_dir()` answered the same fact first on both platforms. Expect one of yours to be
      shadowed.
- [ ] **All three sites, or a stated reason why not.** They share the path; fixing one is the enumeration
      defect this repo keeps finding.
- [ ] Decide what to do with any existing `cpe-ai-console` directories, using CPE-1964's five-condition
      fail-closed shape if a sweep is warranted — and CPE-1972's rule: *an absence of information must
      never license a delete.*

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1964's enumeration (PR #1086), which found these three
while re-deriving the `temp_dir()` site list with the corrected recipe and flagged them as wanting their
own ticket.

Related: **CPE-1964** (PR #1086 — the same class, and the model for the fix, the control and the CPE-1929
pairs), **CPE-1952** (delete the seam rather than defend it, where that is available), **CPE-1929**
(shadowed guards — one was found and deleted in CPE-1964's own sweep), **CPE-1972** (an absence of
information must never license a delete).

## Work Log — 2026-08-28

### 1. Reproduced first, on both platforms, asserting on the filesystem

**Windows** (this machine, `%LOCALAPPDATA%\Temp`, junction via `junction::create`, no admin): a
throwaway `#[test]` planted a junction at a real `<temp>/cpe-ai-console-…` path and ran all three
sites' primitives verbatim. `create_dir_all` returned `Ok`; `session-diag.log` and
`session-daemon.port` both landed in the attacker's directory with our bytes; `exists()` returned
`true` through the link and `remove_file` deleted the attacker's file. Every assertion was on where
bytes ended up, never on a returned verdict.

**Linux, real ext4** — not `/mnt/z` (a 9p mount) and not `/tmp` (tmpfs on WSL, so `TMPDIR` was
overridden to an ext4 home directory, confirmed by `df -T`):

```text
TMPDIR filesystem: /dev/sdd  ext4
mkdir -p on the planted symlink            -> exit 0
victim after     : ['session-daemon.port', 'session-diag.log']
exists() through the link                  -> True
victim after unlink('session-daemon.port') : ['session-diag.log']
BARE mkdir on the planted symlink          -> errno 17 File exists (EEXIST)
lstat says is_dir: False   is_symlink: True
```

The last two lines are the fix measured on the same filesystem. **Scope, stated:** the Unix half was
measured through the syscalls (`mkdir -p`, `mkdir(2)`, `stat`, `lstat`, `unlink`), not through Rust's
`std` wrappers — WSL here has no C linker and no passwordless `sudo`, so the crate cannot be built
there. The Rust legs on Linux and macOS are the shipped `#[test]`s, run by CI's 3-OS `sidecar` job;
that is **unmeasured by this shift**.

### 2. The consequence, demonstrated — and it is smaller than the ticket assumed

The ticket's headline (a console connecting to the attacker's "daemon") **is not reachable in the
shipped product**, and that is a real result that lowers the priority. Evidence, not argument:

- `SessionDaemonHandle::discover_or_spawn` is the port file's **only** reader and **only** writer, and
  it has **zero callers** — this crate, `sidecar/host`, `src-tauri`, and every test (`rg`, whole tree).
- Production learns the port a different way entirely: the host spawns the daemon as its own child
  (`AiConsoleState::ensure_session_daemon`), reads `PORT <n>` off that **child's stdout pipe**, and
  injects `CPE_AICONSOLE_SESSION_DAEMON_ADDR`; `main.rs` parses that and calls
  `SessionDaemonHandle::external(port)`. A pipe and an env var are not substitutable by a plant.
- Corroborated on disk: this machine's `%LOCALAPPDATA%\Temp\cpe-ai-console` (last write 2026-08-20)
  holds `session-diag.log` (14,843 bytes) and **no `session-daemon.port`**. Months of use; the writer
  has never run.

A consequence was **not manufactured** to fill the gap. What *is* live, and is demonstrated by the two
shipped control tests, is a **write** primitive (the trace log, carrying session ids/pids/byte counts,
appended into an attacker-chosen directory) and a **delete** primitive (the host reaper unlinking a
fixed filename inside one).

Second-order finding, recorded at the site: `daemon_answers()` — the only check between a port number
and a `SessionClient` — is a **liveness probe, not authentication**. Any loopback listener that writes
one byte passes it. So hardening the directory would **not** be sufficient once `discover_or_spawn` is
wired; that needs a token minted by the daemon and printed beside `PORT <n>`. Not built here (a
defence on a path with no callers cannot be exercised, which is CPE-1929's shadowed guard in another
costume) — named so the CPE-309 S4 wiring step inherits it.

### 3. All three sites fixed

- `sidecar/ai-console/src/console_temp_dir.rs` (**new**) — one primitive, `ensure_console_dir_at`:
  `std::fs::create_dir` (not `create_dir_all`), and on `AlreadyExists` — which a **rendezvous**
  directory must tolerate, unlike CPE-1964's fresh mission directory — a `symlink_metadata().is_dir()`
  verification. No `exists()` pre-check (it would be a shadowed guard; said at the site). Plus
  `regular_file_or_absent`, which refuses a link at the *leaf's* name inside a genuine directory.
- `session_diag.rs` — `log_path()` derives from the constants; `trace()` **early-returns** on either
  refusal. The old `let _ = create_dir_all(dir)` had to become a return, not a logged error: the
  following `OpenOptions::create(true)` writes through the link with or without our `create_dir`.
- `session_supervisor.rs` — `write_port_file` uses the primitive and **propagates** the error;
  `read_port_file` refuses a redirected directory or a non-regular port file.
- `sidecar/host/src/reaper.rs` — the site with no `create_dir_all`, in scope because it reads and
  unlinks. `pf.exists()` + `remove_file` replaced by `remove_stale_port_file`: the **parent** must be
  a plain directory (path resolution always follows intermediate components, so this is the check that
  stops the escape) **and** the leaf must be a plain regular file. Every failure returns
  "did not remove".

### 4. Sensitivity controls: ordinary CI tests, all three OSes, real paths, panic on an unplantable link

`sidecar/ai-console/tests/console_temp_dir_containment.rs` (4 tests) and
`sidecar/host/tests/reaper_port_file_containment.rs` (5 tests). Both open with the **attack
succeeding** against the pre-fix primitive. Neither is `#[ignore]`d. A directory link that cannot be
planted **panics** with a message saying the leg verified nothing — heeding the leg #1075 lost twice.
`skip_notice!` is out of a sidecar's reach (ADR 0001), so notices go to the real stderr handle with
`writeln!`. The one leg that can legitimately be unavailable (a Windows **file** symlink needs
Developer Mode, unlike a junction) reports rather than panics, and says which leg still ran; on this
machine it was plantable, so it really ran.

The plant is at a **sibling** of the production name (`cpe-ai-console-cpe1975-<pid>`) rather than the
literal one, under the real `std::env::temp_dir()`, with the name derived from the production
constant. Reason stated at the site: the production name is a **constant**, so planting at it would
race parallel test threads *and* destroy the machine's live rendezvous directory — a test must not be
the destructive operation it is testing. The literal real path is instead pinned by two further tests
that drive the no-argument production entry points non-destructively.

### 5. CPE-1929 sabotage pairs — four refusals, eight runs, numbers written at each site

`--no-fail-fast` throughout (cargo otherwise stops at the first failing binary and the totals are not
comparable); the `Compiling …` line was confirmed in every sabotage run, so none is a stale-binary
pass. Baselines: **ai-console 422/0**, **host 153/0**.

| refusal | disabled | predicate lies |
|---|---|---|
| `ensure_console_dir_at`'s `!meta.is_dir()` | RED 420/**2** | RED 421/**1** |
| `regular_file_or_absent`'s `is_file()` | RED 420/**2** | RED 421/**1** |
| reaper's parent-directory `is_dir()` | RED 152/**1** | RED 152/**1** |
| reaper's leaf `is_file()` | RED 152/**1** | RED 152/**1** |

Eight reds. **The expected shadowing did not occur, and it was measured rather than reasoned about:**
each pair reds a *different* test, because "is the directory real?" and "is the leaf real?" are
different facts and each has a test that reaches only it. All eight are **Windows-only** — noted at
every site, because this shift's own history includes a pair that came out green on one platform and
red on the other.

### 6. Existing `cpe-ai-console` directories — no sweep, and the reason

Unlike `cpe-swarm-<millis>` (one per mission; 55 counted), this name is a **constant**: there is
exactly one, ever. Measured here: one directory, one log file. It is not litter, it is the rendezvous,
and deleting it is not a cleanup. A directory that *is already* a planted link is **refused, not
removed** — removing it would be this code unlinking something at a shared path it can prove nothing
about, which is CPE-1972 exactly. The refusal is loud and leaves the evidence for the user.

### 7. Also fixed: an untested provenance claim, and one duplication removed

`reaper.rs` carried *"Mirrors `ai-console`'s `session_supervisor::default_port_file()` … Keep them in
sync"* — untested by construction, with a green suite next to it reading as vouching for it. The
duplication is **forced** (ADR 0001 bars the host from depending on a sidecar), so it is now
**derived**: `src/lib/consoleTempDirPath.test.ts` reads both literals out of the two Rust sources with
`stripRustComments` + `rustStringLiteralAfter` and fails naming both values. **Red-proofed twice and
recorded at the site:** changing the host literal → 1/4 red naming both values; putting a bare
`.join("cpe-ai-console")` back into `session_diag.rs` → 1/4 red on the sweep leg.

CPE-1964's enumerator (`git ls-files` → drop `tests/` → strip comments → cut at column-0
`#[cfg(test)]`) was **lifted** into `src/lib/rustProductionSources.ts` rather than copied, so both
guards share one implementation (CPE-1950: remove the duplication where it is removable).

Stated blind spot, without a count: the bare-literal sweep catches **at least** a literal
`.join("cpe-ai-console")` / `.join("session-daemon.port")` in tracked production Rust. It does not see
a path built by `format!`, via another constant, from a byte string, or by concatenation. A tripwire
for the shape that occurred, not a closure of the class.

### 8. Residual, stated rather than implied

CPE-1964 got the clean atomic property because each mission directory is new. A rendezvous is expected
to already exist, so `AlreadyExists` must be followed by "…and is it real?" — a **verify-then-use** with
a real, if narrow, TOCTOU window a same-user attacker could race. No path-based design closes it;
closing it needs handles (`O_DIRECTORY|O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT` + `openat`). Out of
scope, named at the site so green tests are not read as having closed it. And what the hardening does
**not** buy, said out loud: the directory still exists at a constant path in a shared namespace, and
always will — that is what a rendezvous is.

### 9. Verification

- `sidecar/ai-console`: `cargo test --locked --no-fail-fast` **422 passed / 0 failed**;
  `cargo clippy --locked --all-targets -- -D warnings` clean.
- `sidecar/host`: **153 passed / 0 failed**; clippy clean.
- `src-tauri`: `cargo clippy --locked --all-targets -- -D warnings` clean in **both** feature modes
  (default and `--features sidecar-platform`).
- `npm test`: ~~**5,380 passed / 2 skipped across 360 files**~~ — **wrong, corrected in round 2 below
  ("The `npm test` figure is corrected"): 5,380 is the TOTAL; the pass count is 5,378, and a second
  machine sees 19 pre-existing environmental failures.** Struck through in place rather than silently
  rewritten, because a reader landing on this section must not take the old count as current — which
  is what a correction 95 lines further down allows. `npm run check`: 0 errors, 0 warnings.
- No `specta::Type` struct touched, so `bindings.gen.ts` needs no regeneration.
- Temp hygiene: no planted junction left behind — `%TEMP%` holds only the genuine `cpe-ai-console`
  directory (a plain Directory, no reparse attribute). WSL scratch removed.
- **Note for the release step:** this changes a sidecar *and* `sidecar/host`, which is linked into the
  app. A launcher swap is not a host swap — the host binary must be rebuilt with the sidecar config
  for either half to ship.

### 10. In-app docs

`src/docs/04-ai-console.md` gains a "Limits / notes" entry explaining the `cpe-ai-console` temp folder,
why it is at a fixed name, that the app now refuses to use it if it is not a plain folder, and that it
never deletes what it finds there.

---

## Round 2 — review response (PR #1097: SEC PASS, CHANGES REQUESTED)

The Reviewer independently reproduced everything load-bearing: the zero-callers finding (swept
tree-wide, **confirmed true**, production path traced, corroborated on disk), four of the eight
sabotage numbers (**all four reproduce exactly, same tests named**), `daemon_answers` as
liveness-not-auth, the enumerator lift as one copy, and the controls (9 tests, none `#[ignore]`d, both
sensitivity controls escaping, `plant_or_panic` panicking, "NOT VERIFIED" appearing in no run).

Two claim-scope findings were **required**, and the first is the same defect this ticket exists to
kill, committed by the commit that killed it.

### F1 (required) — I stated a CI guard as measured fact and it covers the opposite direction

Round 1 kept the duplicated path constants and justified it with *"ADR 0001's one-way rule means the
host may not depend on a sidecar crate, and CI fails the build if it tries."* **False.** ADR 0001's
rule and its CI guard (`ci.yml`, "Enforce one-way dependency") are about a *sidecar* depending on the
*explorer app*; the guard greps `sidecar/*/Cargo.toml` for `^(app_lib|cross-platform-explorer)\b` or
`path = "../../src-tauri"`. A `path = "../ai-console"` matches neither. **CI would have passed.** I
never ran the experiment; the claim read as a measurement because it sat next to a green test that
actually vouched only for two string literals being equal. Verified the Reviewer's reading against
`docs/adr/0001-sidecar-platform.md:68` and `ci.yml:1626-1634` before acting.

**Took option (a): the duplication is gone.** Both constants now live in **`sidecar-contract`** — the
crate that exists for exactly this, a host↔sidecar shared surface — and both crates re-export them.
Neither gains a dependency edge (both already depend on the contract), so the one-way rule and the
delete-test are untouched. CPE-1950's stated preference, applied.

The false claim is **recorded at all three sites** (`sidecar_contract::CONSOLE_DIR_NAME`,
`console_temp_dir`, `reaper`) rather than quietly deleted, because "the claim was never run as an
experiment" is the reusable lesson. `consoleTempDirPath.test.ts` loses its two agreement cases (there
is nothing left to agree) and keeps the sweep, which is the part a shared constant cannot enforce.

### F2 (required) — *"propagated rather than dropped"* was not true of the code

Both halves were wrong: `discover_or_spawn` returns `Result<_, String>`, not `io::Result`, and its
sole call site still read `let _ = write_port_file(port_file, handle.port);`. `write_port_file` was
**already** `-> io::Result<()>` before the diff, so all that changed was *where* the error got
swallowed — one frame up. Net behaviour: still dropped. Worse, the sentence was written for the
CPE-309 S4 wiring step and told that reader the opposite of what the code does.

Handled at the call site. Deliberately **not** `?`: the daemon is already spawned **detached** by that
point, so returning `Err` would abandon a live daemon nobody holds a handle to — a bookkeeping failure
traded for an orphan process. The handle is still returned; the failure now goes through
`session_diag::trace`, which echoes to stderr unconditionally, so it survives even when the refusal is
precisely what stops the trace *log* being written. That matters because after the hardening the only
ways this fails are real I/O and **a refusal** — i.e. someone has planted something at the rendezvous
path, which is a security-relevant event that was being reported to nobody. `write_port_file`'s doc
now says the error is returned by this function and what its caller does with it.

### F3 (minor, taken) — a read path no longer creates anything

`read_port_file` called `ensure_console_dir_at`, so a lookup `mkdir`ed. The `!meta.is_dir()` predicate
was extracted as `console_dir_is_real` (lstat only, every error `false`), and both `ensure_console_dir_at`
and `read_port_file` now use it — one predicate, one place to sabotage, one place to audit. "Directory
not there yet" answers "no port file", which is correct on a first run.

Because that extraction is a claim of behaviour-preservation, **both ai-console sabotage legs were
re-run against the shipping code** rather than carried forward: 420/**2** and 421/**1**, identical to
round 1, same tests named. Two host legs were re-run too (that file changed as well): 152/**1** each,
also identical. One deliberate consequence is noted at the site — an unreadable `symlink_metadata` used
to propagate its own `io::Error` and now surfaces as the `AlreadyExists` refusal. Both refuse.

### F4 (minor, taken) — the residual TOCTOU is now declared in `reaper.rs` too

It was disclosed only in `console_temp_dir`'s header, and a reader of `reaper.rs` never sees that file
— a residual declared only in the other crate is, for this reader, not declared. `remove_stale_port_file`
now states the window between its last `symlink_metadata` and the `remove_file`, what it costs an
attacker, and that closing it needs a handle-based design.

### The two "no action demanded" notes, both taken

**The control files' asymmetry is now remarked**, in the host file's header: ai-console plants under
the real `temp_dir()`, the host file inside a `tempfile::tempdir()`, and the reason is that the two
crates expose different seams. `ensure_console_dir_at` *decides where the directory goes*, so it must
see the real temp directory; `reap_orphan_session_daemons` takes the port file **as a parameter** and
is path-generic, with the production path pinned separately from the production constants. Planting in
the real temp directory there would add no reachability and would risk the machine's live rendezvous
directory.

**The `npm test` figure is corrected, and it was wrong in a second way I had not noticed.** Round 1
reported "5,380 passed / 2 skipped" — 5,380 is the **total**; the pass count is 5,378. Re-measured
here: **360 files, 5,378 passed / 2 skipped / 0 failed**. The Reviewer measures **5,320 passed / 19
failed** on their machine, with the identical 19 failing on `main` (`337ac334`) —
`catalogPublishVersion`, `catalogPublishFreshnessGuard`, `catalogPublishLoudFailure`,
`releaseVerifyWiringGuard`, all shell-execution guards, environmental. Both numbers are reported with
whose machine they came from, because a single figure would imply the other environment does not
exist; **the 19 are pre-existing and not caused by this branch.**

### Round-2 verification

- `sidecar/contract`: **12 passed / 0 failed**; clippy clean.
- `sidecar/ai-console`: **422 passed / 0 failed**; clippy clean. `sidecar/host`: **153 passed /
  0 failed**; clippy clean.
- `src-tauri`: clippy clean in **both** feature modes.
- `npm run check`: 0 errors, 0 warnings. Both path guards green.
- No new dependency edge, so no lockfile changes; `--locked` builds pass in all three sidecar crates.

### Noted for a follow-up ticket (not this one)

The Reviewer found two more fixed-name temp paths of the same class, outside this ticket's three:
`src-tauri/src/lib.rs:10167` `cpe-ai-console-catalog` (holds **verified catalog manifests**) and
`:11911` `cpe-sidecar-storage` — both `app_data_dir()` fallbacks, both higher-value targets than a
trace log.

---

## Round 3 — review response (SEC PASS re-affirmed, one finding)

Three of round 2's four fixes verified clean with the Reviewer's own numbers, and the constants move
was measured **stronger** than I claimed: `git diff --name-only main...b382e051 | grep -i cargo`
returns nothing — no manifest, **no lockfile**. One finding, both halves correct, and **half of it was
functional rather than wording**.

### (a) My reason for not using `?` was false — the conclusion survives, stronger

I wrote that `?` "would abandon a live daemon nobody holds a handle to — an orphan process". Verified
against the source: `spawn_detached` returns `SessionDaemonHandle { child: Some(child), .. }`, and
`Drop` reaps exactly that case (`child.kill()`, `child.wait()`). **With `?` the handle drops at the `?`
and the daemon is killed, not orphaned.**

The real hazard is worse, and it is now what the comment says: `?` would hand the attacker a **kill
switch** — plant a link → `write_port_file` refuses → `?` → `Drop` reaps the daemon we just spawned →
the console has no session daemon at all. A refusal must never be able to take down the thing it is
protecting.

### (b) The functional half: I reported through a channel that is off by default

I wrote that `session_diag::trace` "echoes to stderr unconditionally". Verified: `trace` opens with
`if !enabled() { return; }`, and `enabled()` requires one of four env vars. The `eprintln!` is
unconditional only **relative to the file writes after it** — which is why the same sentence is true
*inside* `trace` and false *about* it.

And it bites on exactly the path the change exists for. `CPE_AICONSOLE_DIAG` is set only by
`run_session_daemon()` (`main.rs:125`), explicitly process-local to the **daemon**;
`discover_or_spawn` runs in the **console** process; and `CPE_AICONSOLE_SESSION_DAEMON_ADDR` is set
only on the host-injected path, which uses `SessionDaemonHandle::external` **instead of**
`discover_or_spawn`. So on the future reattach path, **none of the four is set and the
security-relevant refusal was reported to nobody** — the precise outcome the change was made to
prevent.

Fixed: the refusal now goes to the real stderr handle with an ungated
`writeln!(std::io::stderr(), "[CPE-1975] …")` — the same pattern, for the same reason, as the notices
in the containment tests — **and** still calls `trace`, so it also reaches the diagnostic log when
tracing happens to be on. The message now also names what a refusal means ("something is planted at
that path; look at it before removing it").

**The lesson is the shift's pattern in one line, and it is recorded at the site: I fixed a swallowed
error by routing it to a reporter that is itself off by default. A report is only as good as the
channel it lands in — check the channel, not just the call.**

### The gate is now pinned by a test, not just a comment

`session_diag::tests::tracing_is_off_by_default_so_it_cannot_carry_a_must_see_message` asserts
`!enabled()` in a default process and that a disabled `trace` does not even create the log file. A
future edit routing a must-see message back through `trace` alone now has a red test standing next to
it. `trace`'s own doc says the gate out loud, since the doc previously read as an unconditional echo
and that misreading is what caused this.

### That test moved the baseline, so every sabotage number was re-measured

Adding it took `sidecar/ai-console` from **422 → 423**, which invalidates absolute figures recorded
against 422. The deltas and the named tests never changed, but **a stale absolute figure sitting
beside a green suite is the exact failure mode this file is about**, so all four ai-console legs were
re-run against the shipping code rather than adjusted on paper:

| refusal | disabled | predicate lies |
|---|---|---|
| `console_dir_is_real` | RED 421 / **2** | RED 422 / **1** |
| `regular_file_or_absent` | RED 421 / **2** | RED 422 / **1** |

`sidecar/host` is untouched by round 3, so its pairs were **not** re-run and its baseline is still 153
— said explicitly so the two crates are not read as having had equal treatment.

### The nit

Round 1's §9 still read `npm test: 5,380 passed / 2 skipped` with the correction 95 lines below. Now
struck through **in place** with a pointer to the correction, rather than silently rewritten, so a
reader landing on §9 cannot take the wrong count as current.

### Round-3 verification

- `sidecar/ai-console` **423 / 0** (no `NOT VERIFIED` notice), `sidecar/host` **153 / 0**,
  `sidecar/contract` **12 / 0**; clippy clean in all three.
- `src-tauri` clippy clean in **both** feature modes; `npm run check` 0 errors / 0 warnings; both path
  guards green.
- Still unmeasured here and still marked so: every Linux and macOS leg (no C linker in this shift's
  WSL). Those run in CI's 3-OS `sidecar` job.
