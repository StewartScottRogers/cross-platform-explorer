---
id: CPE-1742
title: The FTP rig's STOR invents the parent directory chain, which no real daemon does
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by **CPE-1731**, 2026-08-14, while fixing the sibling verbs.

`crates/ftp/src/lib.rs:~526`, inside the `#[cfg(test)]` rig's `STOR` handler:

```rust
let path = real_path(&root, &arg);
if let Some(p) = path.parent() {
    let _ = std::fs::create_dir_all(p);
}
```

RFC 959 `STOR` has **no** directory-creating semantics. A real daemon answers `550` when the parent
directory does not exist; it does not create it. So a client test can upload to `/a/b/c.txt` against
this rig with no `MKD` at all and pass, against behaviour no real server has — the same family as
CPE-1731's `RMD`/`MKD` fixes, one verb over.

## Scope

`crates/ftp/src/lib.rs`, the `STOR` arm.

**This is a client-visible change, which is why CPE-1731 did not do it as a drive-by.** Unlike
`RMD`/`MKD` — where the primitive simply did not match the verb — removing this changes what a *client*
must do before uploading, so the fix is "make `STOR` refuse a missing parent" **plus** whatever the FTP
provider needs to create directories first. Check `cpe_server::transfer::upload_tree` and CPE-1741
before starting; the two interact.

Measured while filing and **re-measured at the current suite size**: removing the `create_dir_all`
leaves `cpe-ftp` **14/14 green**, and this crate has no `upload_tree` test at all — so nothing *in this crate* depends on it. That is a statement about
this crate's coverage, not about the client being correct.

## Acceptance criteria

- [x] `STOR` to a path whose parent does not exist is answered `553` (not `550` — see Work Log), and the
      parent chain is **not** created — asserted on the filesystem.
- [x] `STOR` to a path whose parent exists still works, with the bytes asserted (no over-rejection).
- [x] Any client path that relied on the invention is fixed rather than the rig being loosened back.
- [x] The guard broken on its own turns a distinct test red, real output pasted.

## Notes

Filed from CPE-1731 (PR #905), where the deliberate non-change is recorded **at the `STOR` arm itself**
(`crates/ftp/src/lib.rs:~525`), with the full reasoning in the `MKD` arm below it. An earlier draft of
this line claimed the note was at the site when it was only in `MKD`, thirty lines away. Related:
**CPE-1741** (`upload_tree`'s unconditional `mkdir(&base)`).

## Work Log

**2026-08-21** — Fixed. `crates/ftp/src/lib.rs`'s `STOR` arm now checks `path.parent().is_dir()` before
taking the PASV listener; a missing parent is refused and the data connection is never opened. The
`create_dir_all` call that used to run inside the data-connection block is gone.

**The response code is `553`, not the `550` this ticket's own acceptance criteria guessed** — corrected
by two independent sources, not just the rig:

- **Specification.** RFC 959 §5.4's reply sequence for `STOR` is `125`/`150` → `226`/`250` positive, or
  `532`/`450`/`452`/`553` negative. `550` is never in STOR's set at all — it belongs to a different group
  of verbs (`DELE`/`RMD`/`MKD`/`PWD`/`CWD`/`CDUP`/`SMNT`), which is where CPE-1731's fixes correctly used
  it. Confirmed via web search against the RFC text and a secondary summary
  (freesoft.org/CIE/RFC/959/29.htm), since training data alone isn't good enough evidence for a wire
  contract like this one.
- **A real daemon.** vsftpd — the actual FTP server this repo's own CI runs `crates/vfs`'s
  `real_server_conformance.rs` STOR assertions through (`fauria/vsftpd`, `.github/workflows/ci.yml` /
  CPE-1659) — answers exactly `553 Could not create file.` for a STOR whose parent directory is missing.
  This is also the response widely reported by vsftpd users hitting the same condition (searched, not
  assumed).
- **The QNAP NAS.** Attempted to reach it directly for a first-party measurement (this session runs on
  the user's own LAN, `192.168.1.x`), but I don't have its address on hand and `qnap` / `qnap.local`
  don't resolve from here — no mDNS resolver available in this environment. Did not chase further given
  the ticket's `S` estimate; CPE-1518 (E2E-verify against the QNAP, still Backlog) is the right place to
  capture a first-party QNAP FTP measurement if one is wanted later.

So the fix intentionally does not match the AC's literal `550` — it matches what STOR's own RFC section
and a real daemon both say, which is the stronger claim the AC was reaching for.

**What broke, and what didn't:**

- **Nothing broke.** `cpe-ftp`'s suite was 14/14 when this ticket was filed (2026-08-14) and is 21/21 now
  (2026-08-21) — the crate grew via other tickets in between. Re-measured with the `create_dir_all` call
  removed: still 21/21 green (20 pre-existing + 1 new test for this ticket), no reds, no test needed
  fixing. None of the existing `provider.write(...)` calls in this crate's tests target a path whose
  parent doesn't already exist (they write to the served root, or to a directory seeded by
  `spawn_ftp_server`/`mkdir`/`std::fs::create_dir_all` in the test setup, or are escape attempts that
  never reach the new check at all because `real_path` already refuses them).
- **The production FTP provider was NOT relying on the invented parents.** `crates/ftp/src/lib.rs`'s
  `FileSystemProvider::write` is a bare `put_file` (one `STOR`, nothing else) — it never created
  directories itself. The one production caller that walks a tree and writes files,
  `cpe_server::transfer::upload_tree` (`crates/server/src/transfer.rs`), already creates every directory
  in the chain itself via explicit `provider.mkdir()` calls (through `ensure_dir`, added by CPE-1741)
  *before* it ever calls `provider.write()` on a file inside that directory. So there is no latent
  provider bug here to report — CPE-1741 already closed that gap from the production side, independently
  of this rig's leniency. `crates/ftp/src/lib.rs`'s own `upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist`
  test already proved this by asserting an *empty* directory (`empty/`, which only a real `MKD` chain can
  produce — `create_dir_all` would never call `MKD`) existed after the upload, back when the invented
  `create_dir_all` was still live in the rig.
- **New test added:** `stor_refuses_a_missing_parent_and_still_works_for_one_that_exists` (both AC
  halves): asserts the `553` line (via `suppaftp`'s `FtpError::UnexpectedResponse` `Display`, which
  carries the raw wire reply through), asserts the filesystem does NOT gain `/nosuchdir` after the
  refusal, and as the positive control asserts a `STOR` to the root and to an `MKD`'d directory both
  still succeed with bytes read back correctly.

**Red-proof** (AC4): with the new guard's condition changed to `Some(parent) if false && …` (one line,
`crates/ftp/src/lib.rs`), `stor_refuses_a_missing_parent_and_still_works_for_one_that_exists` went red:

```
thread 'tests::stor_refuses_a_missing_parent_and_still_works_for_one_that_exists' panicked at crates\ftp\src\lib.rs:942:14:
STOR into a missing parent must be refused, not silently succeed: ()
test tests::stor_refuses_a_missing_parent_and_still_works_for_one_that_exists ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.17s
```

Reverted immediately after (single line back to `Some(parent) if !parent.as_os_str().is_empty() && …`);
suite confirmed green again (21/21) before committing.

**Gates:** `cargo test -p cpe-ftp` 21/21 green; `cargo test` in `crates/server` (the crate owning
`upload_tree`, the one production consumer that walks directories) 2287/2287 lib tests + all integration
test binaries green, 0 failed, 4 pre-existing `#[ignore]`d; `cargo test` in `crates/vfs` (the other
consumer, `FtpProvider` routed through `cpe_vfs::open`) 24/24 green, 6 pre-existing docker-gated E2E tests
`#[ignore]`d (need the CPE-1659 rig, not available in this session). `cargo clippy --all-targets -- -D
warnings` clean on `crates/ftp`, `crates/server`, and `crates/vfs` — no warnings, no errors (one
unrelated pre-existing `future-incompat` note about the `russh` dependency, not from this change and not
a clippy warning).

Not moved out of `Backlog/` per the sprint runner's instruction — PR is up for review/merge first.

**2026-08-21, round 2** — Independent review of PR #985 approved the 553-not-550 call (verified
against RFC 959 §5.4 verbatim and vsftpd's own `ftpcodes.h`/`postlogin.c`, more primary-source
confirmation than round 1 had) and confirmed there is no production FTP-remote write path at all, so
the "nothing broke" claim from round 1 stands even more strongly. It found one real coverage gap plus
two cheap corrections, and flagged one adjacent pre-existing bug. All addressed on the same branch,
`crates/ftp/src/lib.rs` only:

1. **Gap — the guard survived an `is_dir()` → `exists()` mutation.** Nothing exercised "parent exists
   but is a plain file" (`STOR /at-root.txt/child.txt` where `at-root.txt` is an ordinary file). The
   guard already used `is_dir()` (correctly refusing this shape), but no test proved it. Extended
   `stor_refuses_a_missing_parent_and_still_works_for_one_that_exists` with that exact case. **Red-proof:**
   changed `if !parent.is_dir()` to `if !parent.exists()` (one line) →
   ```
   thread 'tests::stor_refuses_a_missing_parent_and_still_works_for_one_that_exists' panicked at crates\ftp\src\lib.rs:1013:14:
   STOR whose parent exists but is a plain file must be refused: ()
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.03s
   ```
   Reverted; suite green again.

2. **The "filesystem gains nothing" assertion was graded by the client.** `provider.stat("/nosuchdir").is_err()`
   used `FtpProvider::stat`, which is `list(parent)` + `parse_list_line` and deliberately skips
   unparsable rows — a garbled `LIST` line would pass this assertion even if the directory existed.
   Changed to `!root.path().join("nosuchdir").exists()`, reading the rig's own real temp directory
   directly with `std::fs`, independent of the client under test — this repo's own convention, per
   `crates/vfs/tests/real_server_conformance.rs`'s `assert_mkdir_rename_delete_verified_from_host_disk`
   ("read it back with something other than the client"). `exact_server()`'s `_root` binding is now
   used (renamed to `root`).

3. **Stale rig-wide invariant.** The doc comment on `CPE_1730_ESCAPED_ROOT_REFUSAL` said `553`
   "belongs to CPE-1731's root-destination guard" and that a second `553`-answering guard would mask
   it. This round added exactly such a second guard (`STOR`'s missing-parent refusal). Verified there
   is no actual masking: CPE-1731's `reply.starts_with("553")` rows are driven exclusively through
   `rnfr_rnto`/`raw_commands`, which send only `RNFR`/`RNTO` and never open a data connection, so a
   `STOR`-only `553` reply is structurally unreachable from those tests. Reworded "belongs to" →
   "verb-scoped, not exclusive to this rig's confinement guards," with the reachability argument spelled
   out inline.

4. **Adjacent pre-existing bug, fixed rather than merely recorded:** `STOR` onto a path that is
   **itself an existing directory** (e.g. `provider.write("/madedir", ...)` where `/madedir` was made
   via `MKD`) passed the missing-parent guard (its parent, the root, exists) and reached `fs::write`,
   which fails on a directory — but the error was swallowed (`let _ = ...`) and the rig answered
   `226 Transfer complete` anyway. Fixed by refusing with the same `553` line when `path.is_dir()`.
   Chose to fix rather than only record: this round's own new comments claim "STOR refuses what a real
   daemon refuses," and leaving this shape broken while asserting that would make my own claim false.
   **Provenance is weaker for this sub-case than for the missing-parent one** — I have no first-party
   vsftpd source citation for the exact wire text an existing-directory `STOR` gets (the review's
   `ftpcodes.h`/`postlogin.c` citation was for the missing-parent path specifically), so the code
   comment says so explicitly rather than implying the same evidentiary weight. **Red-proof:** disabled
   the check (`if false && path.is_dir()`, one line) →
   ```
   thread 'tests::stor_refuses_a_missing_parent_and_still_works_for_one_that_exists' panicked at crates\ftp\src\lib.rs:1029:14:
   STOR onto an existing directory must be refused, not silently swallowed: ()
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.29s
   ```
   Reverted; suite green again.

5. **Nits, both taken:** removed the dead `!parent.as_os_str().is_empty()` condition (unreachable —
   `real_path`/`joined_path` always yield an absolute path, so `.parent()` here is never `Some("")`),
   with a comment explaining why it was safe to drop. Added the trailing period to the wire text
   (`"553 Could not create file.\r\n"`), matching vsftpd's literal reply more faithfully; the test still
   uses `contains` so this was cosmetic-safe either way.

**Gates, re-run:** `cargo test -p cpe-ftp` 21/21 green (no new test *functions* — the new assertions
extend the one existing CPE-1742 test rather than adding new ones, so the count is unchanged from round
1's 21). `cargo clippy --all-targets -- -D warnings` on `crates/ftp`: clean, no warnings, no errors.

`git diff --numstat` for round 2: `114  34  crates/ftp/src/lib.rs` — the only file touched.

**2026-08-21, round 3** — Round 2's reviewer returned APPROVE and reproduced all six mutations,
including confirming the new "parent is a plain file" assertion reds under the exact `is_dir` →
`exists` mutation it targets, and confirming the filesystem assertion (item 2 of round 2) is
load-bearing rather than a passenger: the *wrong* probe is deleting the guard (the `expect_err` on the
call itself fires first), the *right* probe is making the refusal branch also run `create_dir_all`
(modelling the real historical bug — "refuse on the wire but invent the chain anyway") — and the
filesystem assertion at what is now line 992 catches that. I did not re-run that probe myself this
round; recording that the reviewer ran it and it worked as intended.

More importantly, it **closed the provenance gap round 2 left open** — round 2's comment said this repo
had "no first-party confirmation of vsftpd's exact text" for the existing-directory `STOR` case,
treating it as weaker evidence than the missing-parent case. That hedge was wrong; the two cases share
the same evidence. The derivation, reproduced here rather than only cited:

- vsftpd's `postlogin.c` `handle_upload_common` opens the destination via
  `vsf_sysutil_create_or_open_file`, and its failure branch is a SINGLE undifferentiated test:
  `if (vsf_sysutil_retval_is_error(new_file_fd)) { vsf_cmdio_write(p_sess, FTP_UPLOADFAIL, "Could not
  create file."); return; }` — reached BEFORE `get_remote_transfer_fd(...)`, i.e. before any `150`.
- vsftpd's `sysutil.c`: `vsf_sysutil_create_or_open_file` is `open(p_filename, O_CREAT | O_WRONLY |
  O_NONBLOCK, mode)`.
- POSIX `open(2)` returns `ENOENT` for a missing path prefix, `ENOTDIR` for a prefix component that is a
  plain file, and `EISDIR` when the named file is a directory and `oflag` includes `O_WRONLY`.

All three are errors, all three hit the SAME branch, all three produce the identical wire reply
`553 Could not create file.` — vsftpd does not discriminate on errno there. So the existing-directory
case has exactly the same evidentiary weight as the missing-parent case; the round-1 citation already
covered it, and the trailing period added in round 2 matches vsftpd's literal string.

**Two limits on this evidence, carried verbatim rather than restated as certainty:**

(a) No live FTP daemon was driven for this derivation — the conclusion rests on reading vsftpd's source
plus POSIX `open(2)` semantics, which is derivation, not observation.

(b) The vsftpd source was read via a third-party GitHub mirror (`dagwieers/vsftpd`), not the canonical
signed tarball, because no fetchable plain-text URL for the official release was reachable in this
session. The three files (`postlogin.c`, `sysutil.c`, `ftpcodes.h`) are mutually consistent, but a
mirror is a mirror, not the release artifact itself.

**Two edits, both in `crates/ftp/src/lib.rs`, both requested by the reviewer:**

1. Replaced the "no first-party confirmation… for *this* sub-case" hedge on the existing-directory
   `553` with the derivation above (condensed inline at the guard's own comment site), stated
   explicitly as derivation-from-source rather than a live wire measurement.
2. Tightened Refusal 3's assertion from `err.contains("553")` to `err.contains("553") &&
   err.contains("Could not create file")`, matching Refusals 1 and 2's stricter form. Not a real gap —
   no other `553` line is reachable through `STOR` — but free and consistent.

**Explicitly NOT changed**, per the reviewer's instruction: the two `553` guards' order. The reviewer
proved by mutation that they are mutually exclusive — a path that IS an existing directory necessarily
has an existing-directory *parent*, so the parent guard's condition is false for it; and a path whose
parent is not a directory cannot itself be an existing directory (it cannot exist at all: `open()`
would fail on that prefix first) — so swapping them leaves 21/21 green and the order is immaterial. The
reviewer also proved CPE-1730's confinement is structurally unmaskable by these new `553`s: `path` does
not exist at all until `real_path` returns `Some`, so nothing reaches either new guard for a path
`real_path` would refuse.

**Gates, re-run:** `cargo test -p cpe-ftp` 21/21 green. `cargo clippy --all-targets -- -D warnings` on
`crates/ftp`: clean, no warnings, no errors.

`git diff --numstat` for round 3: `14  6  crates/ftp/src/lib.rs` — the only file touched, as expected
for a two-edit round.

**2026-08-21, round 3 (continued) — CI went red on macOS, folded into round 3 per the coordinator.**

`Server crates (macos-latest)` failed at head `09b72443` (the round-2 commit, before the two comment
edits above): `stor_refuses_a_missing_parent_and_still_works_for_one_that_exists` panicked on Refusal 2
(parent exists but is a plain file) — `/at-root.txt/child.txt` came back `[550] Path escapes the served
root` instead of `553`. Windows was green for the same case. **This is not a flaky test or a test bug —
it did not exist before round 2, so it is pre-existing rig behaviour on macOS (and, by the same
reasoning, Linux) that nothing had ever probed before. Round 2 did not break macOS; it revealed macOS
was already answering the wrong question.**

**Diagnosis**, from reading `crates/server/src/fsutil.rs`'s `confined_to` and `crates/ftp/src/lib.rs`'s
`real_path`/`joined_path` directly rather than assuming the coordinator's hypothesis:

`real_path(root, arg)` is `joined_path(root, arg)` gated by `confined_to(&joined, root)` — if
`confined_to` returns `false`, `real_path` returns `None` and the caller sends the confinement refusal.
`confined_to` walks: `canonicalize(probe)`; on success, test containment; on failure, if the error kind
is `NotFound`, drop the last path component and retry (this is the deliberate "the file doesn't exist
yet, but its ancestors might, and that's enough to know if it's inside root" logic the STOR/MKD create
case needs); on any OTHER error kind, `return false` immediately — refuse, no retry.

For `probe = <root>/at-root.txt/child.txt` where `at-root.txt` is a real file: `canonicalize` cannot
resolve past `at-root.txt` because it is not a directory. **The two platforms report that failure with
different `io::ErrorKind`s, and that is the entire split:**

- **POSIX** (`realpath(3)`, which is what `std::fs::canonicalize` calls on Linux/macOS): a non-final
  path component that exists but is not a directory is `ENOTDIR`, which Rust maps to
  `ErrorKind::NotADirectory` (stable since Rust 1.83) — a DIFFERENT kind from `ENOENT`'s `NotFound`. The
  old code's guard (`e.kind() == NotFound`) does not match `NotADirectory`, so it falls straight to
  `Err(_) => return false` on the very FIRST canonicalize attempt — refused as an escape, without ever
  walking up far enough to discover `<root>/at-root.txt` itself resolves and is inside `root`.
- **Windows** (`CreateFileW` via `GetFinalPathNameByHandleW`): the same shape returns
  `ERROR_PATH_NOT_FOUND`, which Rust's io error mapping puts under `ErrorKind::NotFound` — the SAME kind
  the genuinely-missing-parent case produces. Windows does not distinguish "component doesn't exist"
  from "component exists but isn't a directory" at this API surface, so the existing `NotFound` branch
  already let the walk-up run, found `<root>/at-root.txt` canonicalizes and is inside `root`, and
  correctly answered "contained" — which is why the STOR guard's own `553` was reachable on Windows
  before this fix and never on macOS/Linux.

Round-1's reviewer had flagged exactly this shape of risk in advance and said so honestly ("Not proven
locally") — its reasoning was right for a MISSING directory prefix (`NotFound` walks up correctly on
every platform already) and did not extend to a PLAIN-FILE prefix, which is a genuinely different errno
or the caller's platform mapping of one. The three-OS CI gate caught what a Windows-only local run
structurally could not.

**The defect is not the test — it is `confined_to` reporting the wrong VERDICT on two of three
platforms.** Before this fix, macOS/Linux told an FTP client "Path escapes the served root" about a
path that does not escape the served root at all (`/at-root.txt/child.txt` is squarely inside it); the
real problem is that `at-root.txt` is a file, which is a different question `confined_to` has no
business answering — that belongs to the caller (`cpe-ftp`'s `STOR`, which is exactly what its `553`
guard is for). A wrong-reason refusal is the same defect class this whole ticket exists to close: a
reply that misdescribes what actually happened.

**Fix, `crates/server/src/fsutil.rs`'s `confined_to`:** the retry guard now also treats
`ErrorKind::NotADirectory` as "nothing conclusive yet, keep walking up", the same as `NotFound` already
was:
```rust
Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {}
```
This makes POSIX's walk-up behave the way Windows's already accidentally did for this shape — find the
nearest ancestor that DOES canonicalize (`<root>/at-root.txt`), and test THAT for containment. It does
not weaken CPE-1730: the walk-up only ever answers "is the nearest resolvable ancestor inside root",
which is exactly the same question already asked for the `NotFound` case, and a genuinely escaping path
(`..`, absolute, symlinked-out) still resolves its walked-up ancestor to somewhere outside root and is
still refused — confirmed by re-running the MUT F red-proof below.

**Did NOT make the test platform-conditional.** Accepting either `550` or `553` depending on OS would
have papered over a reply that is factually wrong on two of three platforms rather than fixed it — the
coordinator's stated last resort, not taken, because the fix was findable and clean.

**New regression coverage**, `crates/server/src/fsutil.rs`'s
`confined_to_refuses_all_three_escape_shapes_and_admits_ordinary_paths` (the test that already pins
`confined_to`'s escape/admit taxonomy directly, independent of any rig): added
`confined_to(&root.join("a.txt/new.txt"), &root)` must be `true` — the exact shape that was `false` on
macOS/Linux before this fix. This is the platform-portable place to pin it (`crates/server`'s own suite
runs on the 3-OS matrix, unlike a rig-specific probe), and it fails loudly at the SOURCE of the bug
rather than only at one of its call sites.

**Red-proof, MUT F (re-run per the coordinator's instruction, to confirm the fix did not weaken
CPE-1730):** `STOR`'s `let Some(path) = real_path(&root, &arg) else { … }` mutated to
`let Some(path) = Some(joined_path(&root, &arg)) else { … }` — bypassing confinement entirely. Both of
CPE-1730's filesystem-asserting STOR tests went red on Windows (the only platform available locally):
```
thread 'tests::cpe_1730_an_absolute_request_path_cannot_replace_the_root' panicked at crates\ftp\src\lib.rs:1847:9:
assertion `left == right` failed: an absolute STOR path must not replace the served root (outcome was Ok(()))
thread 'tests::cpe_1730_a_request_path_that_escapes_the_served_root_is_refused' panicked at crates\ftp\src\lib.rs:1742:9:
assertion `left == right` failed: STOR to /../cpe-ftp-srv-32816-0-cpe-1730-victim/victim.txt must NOT reach a file outside the served root (outcome was Ok(()))
test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 18 filtered out; finished in 0.03s
```
Reverted; `cargo test -p cpe-ftp` confirmed 21/21 green again before committing.

**What I could not verify locally, stated rather than implied:** I cannot run macOS or Linux in this
environment. The diagnosis above is a source-level derivation (POSIX `realpath(3)`/`ENOTDIR` semantics
vs. Windows `CreateFileW`/`ERROR_PATH_NOT_FOUND`), not a local observation on either platform — the
verification step is pushing this commit and reading the actual ubuntu-latest and macos-latest CI run
results by SHA (below), not trusting a Windows-only pass.

**Gates, re-run (Windows, this session):** `cargo test -p cpe-ftp` 21/21 green; `cargo test` in
`crates/server` 2287/2287 lib tests passed, 0 failed, 4 pre-existing `#[ignore]`d, all integration test
binaries green; `cargo clippy --all-targets -- -D warnings` clean on both `crates/ftp` and
`crates/server`.

`git diff --numstat` for this fix: `28  1  crates/server/src/fsutil.rs` — the only file touched (the two
comment edits above already landed in the round-3 commit; this fix is the file the coordinator's CI
report pointed at, not `crates/ftp`).
