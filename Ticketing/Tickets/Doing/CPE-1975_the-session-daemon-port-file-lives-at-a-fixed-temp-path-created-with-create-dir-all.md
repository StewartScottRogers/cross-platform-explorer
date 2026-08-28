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
- `npm test`: **5,380 passed / 2 skipped across 360 files**. `npm run check`: 0 errors, 0 warnings.
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
