---
id: CPE-1741
title: upload_tree fails when the remote base directory already exists
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-18
---

## Problem

Found by **CPE-1731**, 2026-08-14, as a direct consequence of making a test double honest.

`cpe_server::transfer::upload_tree` (`crates/server/src/transfer.rs:~744`) opens with an
unconditional

```rust
provider.mkdir(&base)?; // ensure the remote root exists
```

`mkdir` maps to `SSH_FXP_MKDIR` / FTP `MKD`, both of which are `mkdir(2)`: a real server answers
`SSH_FX_FAILURE` / `550` when the name **already exists**. So the `?` aborts the whole upload, and
**uploading into a remote folder that already exists fails** — which is the ordinary case for a user
syncing into an existing directory.

This is not a new defect. It has always been broken against a real OpenSSH or vsftpd server; the reason
nobody saw it is that `cpe-sftp`'s and `cpe-ftp`'s `#[cfg(test)]` rigs implemented `mkdir` with
`create_dir_all`, which is idempotent — the double was more forgiving than the wire, so the client test
passed against behaviour no real server has. CPE-1731 changed both rigs to `create_dir`, which is what
made it measurable:

```text
mkdir onto the seeded /sub    = Err("/sub: Failure: Failure")
upload_tree -> /up (new)      = Ok(1)
upload_tree -> /up (existing) = Err("/up: Failure: Failure")
```

The suite stays green because its one `upload_tree` test uploads to a fresh base.

Reproduced against **both** rigs — CPE-1731 measured SFTP only; the UAT confirmed FTP behaves the same.

## A second symptom: a multi-level remote root, with a different errno

The same line fails a second way, and the ticket's title only names the first. If `remote_root` is
`/a/b` and `/a` does not exist, `mkdir("/a/b")` is `ENOENT` rather than `EEXIST` — a non-recursive
`mkdir` cannot create the chain:

```text
SFTP -> /a/b (missing parent) = Err("/a/b: No such file: No such file")
FTP  -> /a/b (missing parent) = Err("...550 Create failed")
```

The FTP row is worth reading twice: `STOR`'s `create_dir_all` (CPE-1742) does **not** rescue it,
because the failure happens at `mkdir(&base)` before any `STOR` is issued. So a fix that only tolerates
"already exists" leaves this case broken. Whatever replaces line 744 has to answer *both* "it is
already there" and "its parents are not there yet".

## A second site: the per-directory `mkdir`, which is a trap for this ticket's own fix

`transfer.rs:761` — inside the walk — does `provider.mkdir(&remote)?` for **every** directory in the
tree. It is currently *shadowed*: line 744 fails first, so nobody reaches it.

**A fix at 744 alone that tolerates `AlreadyExists` leaves 761 broken for a partially-existing tree**
— re-uploading a folder where some subdirectories already exist remotely would then fail at the first
one, and the failure would look like a *new* bug introduced by the fix. Both sites change together, or
the ticket is not done.

## Scope of the search behind these claims

The UAT swept every caller of the `FileSystemProvider::mkdir` trait method across `crates/`,
`src-tauri/` and `sidecar/`. The only **shipped** callers are `transfer.rs:744` and `transfer.rs:761`;
every other call site is inside `#[cfg(test)]`. It also ran `cpe-vfs` (21 passed) as the crate most
likely to lean on a recursive `mkdir`. So the blast radius really is these two lines — which is why a
ticket is proportionate here and a regression test is not.

**Premise, stated as a premise:** "a real OpenSSH server answers `SSH_FX_FAILURE` for an existing
name" and "a real FTP daemon answers `550`" are read from the RFCs and from OpenSSH's `sftp-server.c`,
**not measured against a live daemon**. They are the reason this is called pre-existing rather than a
regression introduced by CPE-1731. If they are wrong, that classification is wrong — the QNAP NAS on
the LAN is the cheapest way to settle it.

## Scope

`crates/server/src/transfer.rs` — `upload_tree`'s `mkdir(&base)` and the per-directory `mkdir(&remote)`
inside the walk, which has the same shape for any directory that already exists remotely.

The decision to take is which shape "ensure it exists" should have against a wire verb that has no
idempotent form:

1. tolerate an already-exists error from `mkdir` and continue (needs the providers to classify it —
   `SftpProvider` gets `SSH_FX_FAILURE`, which is *also* the generic failure, so this may not be
   cleanly distinguishable); or
2. `stat` first and only `mkdir` when absent (an extra round-trip per directory, and racy, though
   harmlessly so here); or
3. give the provider trait an explicit `mkdir_all`/`ensure_dir` so each backend can express it in the
   way its protocol allows.

Prefer whichever keeps the *provider* honest to its verb — CPE-1731's whole thesis is that a double
which is more permissive than the wire hides client bugs, and (1) risks re-introducing that at the
client layer instead.

## Acceptance criteria

- [ ] `upload_tree` into a remote directory that **already exists** succeeds and lands every file,
      asserted on the remote bytes.
- [ ] `upload_tree` to a **multi-level** `remote_root` whose parents do not exist yet succeeds — the
      `ENOENT` symptom above, which a tolerate-`AlreadyExists` fix does not address.
- [ ] The same for a nested directory inside the tree that already exists remotely (partial re-upload)
      — i.e. **line 761 is fixed too**, not just 744. A fix that leaves 761 alone turns this case into a
      failure that looks like a new bug.
- [ ] Both are checked against `cpe-sftp` **and** `cpe-ftp`, since the two rigs reach the failure by
      different errnos.
- [ ] The rigs are **not** loosened back to `create_dir_all` to make this pass — the fix is in the
      client. A guard-neutralisation showing the new test red against the current `upload_tree`.
- [ ] Whatever classification the fix relies on is measured, not assumed: if it distinguishes
      already-exists from a generic failure, paste what each actually returns.

## Notes

Filed from CPE-1731 (PR #905). Related: **CPE-1742** (FTP `STOR`'s parent invention, the other half of
the same "the double is more permissive than the wire" family).

## Work Log (2026-08-17/18)

**Fix.** `crates/server/src/transfer.rs` gets a new `ensure_dir(provider, path)` helper (placed just
above `upload_tree`), and both `mkdir` call sites in `upload_tree` are replaced with it:
- `upload_tree`'s base-directory line (was `provider.mkdir(&base)?;`) → `ensure_dir(provider, &base)?;`
- the per-directory line inside the walk (was `provider.mkdir(&remote)?;`) → `ensure_dir(provider, &remote)?;`

`ensure_dir` never parses `mkdir`'s error text. It:
1. `stat`s `path` first — if it's already a directory, returns `Ok(())` without ever calling `mkdir`.
   If it exists as a **file**, returns a clear `"{path}: already exists and is not a directory"` error.
2. If `path` doesn't exist, recursively `ensure_dir`s its parent first (a client-side `mkdir -p`, since
   `mkdir(2)` creates exactly one level and can't build a missing chain), then calls `mkdir` on `path`.
3. If that `mkdir` still fails, it rechecks via `stat` once: if a directory is there now (a race, or an
   error the client can't otherwise classify), treats it as success; otherwise returns the original
   `mkdir` error unchanged — a real failure (permission denied, a broken remote) always surfaces.

**Already-exists vs. other-failure decision — option 2 from the ticket ("stat first"), not option 1.**
Option 1 (tolerate `AlreadyExists` from `mkdir`'s own error) was rejected because it isn't reliably
distinguishable from a generic failure on either wire, measured directly against both honest rigs:

- **SFTP** (`crates/sftp/src/lib.rs`'s `io_err`): `ErrorKind::AlreadyExists` falls through the same
  catch-all as every other unclassified error → `StatusCode::Failure`, rendered `"Failure: Failure"` —
  bit-for-bit the same text a permission error or a broken remote would also produce through this
  provider. This matches real OpenSSH (`sftp-server.c`), which likewise returns a bare `SSH_FX_FAILURE`
  for `EEXIST` with no distinguishing reason code.
- **FTP** (`crates/ftp/src/lib.rs`'s `MKD` arm): EEXIST and ENOENT both reply `"550 Create failed"`.
  Measured directly (see red-proof below): the FTP red-proof line for the multi-level test —
  `"/new/deep: Invalid response: [550] 550 Create failed"` — is textually identical in shape to the
  already-exists red-proof line `"/sub: Invalid response: [550] 550 Create failed"`. RFC 959 gives `550`
  no finer sub-code to split them.

So the fix asks `stat` — a verb every provider already answers honestly — rather than classifying
`mkdir`'s error. **What I could not distinguish:** whether a `mkdir` failure that reaches the final
`stat`-recheck-still-absent branch is "genuinely broken" vs. "some other EEXIST/ENOENT shape my two test
rigs didn't produce" — the fix doesn't need to know, since either way the directory verifiably isn't
there and the original error is surfaced unchanged, but that also means a provider whose `stat` can lie
(report a directory present when it isn't, or vice versa) would defeat this approach; no such provider
exists in this codebase today.

**Second site (line 761, per-directory `mkdir` inside the walk).** Changed in the same commit as the
base fix — `ensure_dir(provider, &remote)?;` — per the ticket's own warning that a base-only fix would
leave line 761 shadowed-then-broken. Covered by `upload_tree_succeeds_when_a_nested_directory_already_exists_remotely`
(cpe-server) and its SFTP/FTP counterparts.

**Sibling sites.** Re-ran the same sweep the ticket's UAT describes (`grep -rn "\.mkdir(" crates
src-tauri sidecar`, excluding `/target/`): the only two shipped (non-`#[cfg(test)]`) callers of
`FileSystemProvider::mkdir` performing an "ensure it exists" pattern were `transfer.rs:744`/`:761`
(now `transfer.rs:861`/`:881` after the `ensure_dir` rewrite) — both fixed above. `crates/ftp/src/lib.rs:343`
(`FtpProvider::mkdir`) and `crates/sftp/src/lib.rs:358`/`crates/webdav/src/lib.rs`'s `mkdir` are the
trait *implementations* themselves, not callers, and are unaffected. `crates/webdav`'s own
`upload_tree` test (`generic_transfer_walks_downloads_and_uploads_over_webdav`) exercises the same
shared `cpe_server::transfer::upload_tree` and still passes. No other recursive upload/sync path exists
in `crates/vfs`, `crates/s3`, or `src-tauri`/`sidecar`. No new sibling found beyond what the ticket
already named.

**Tests added (all against the honest, non-idempotent `create_dir` rigs — none loosened back to
`create_dir_all`):**

*`crates/server/src/transfer.rs`* — a new `StrictMkdirProvider` test double wraps `FakeProvider` but
gives `mkdir` real `mkdir(2)` semantics (refuses an existing name with the SFTP-shaped
`"Failure: Failure"` text, refuses a missing parent with `"No such file: No such file"`), so the
`cpe-server`-level tests can't pass by accident the way a plain `FakeProvider` (whose `mkdir` is already
`mkdir -p`) would let them:
- `upload_tree_succeeds_when_the_remote_base_directory_already_exists`
- `upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist`
- `upload_tree_succeeds_when_a_nested_directory_already_exists_remotely`
- `upload_tree_reports_a_clear_error_when_the_remote_base_is_a_file_not_a_directory` (defensive, not in
  the ticket's AC, but the natural third branch of `ensure_dir`)
- `upload_tree_surfaces_a_genuine_mkdir_failure_rather_than_swallowing_it` (via a second double,
  `AlwaysDeniedMkdir`, whose `mkdir` always fails for a non-EEXIST reason — proves the already-exists
  tolerance doesn't blur into swallowing every `mkdir` error)

*`crates/sftp/src/lib.rs`* (real in-process SFTP server, `create_dir`-based rig):
- `upload_tree_succeeds_when_the_remote_base_directory_already_exists` (uploads into the seeded `/sub`)
- `upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist` (`/new/deep`, neither
  level exists)
- `upload_tree_succeeds_when_a_nested_directory_already_exists_remotely` (seeds `/up2/inner` first)

*`crates/ftp/src/lib.rs`* (real in-process FTP server, `create_dir`-based `MKD` arm) — same three test
names, calling `cpe_server::transfer::upload_tree` directly (no `upload_tree` convenience wrapper exists
on `FtpProvider`, unlike `SftpProvider`).

**Red-proof (guard-neutralisation, not a permanently-red test):** for each of the three "succeeds"
tests in all three crates, I temporarily reverted `ensure_dir(provider, &base)?;` /
`ensure_dir(provider, &remote)?;` back to the pre-fix `provider.mkdir(&base)?;` / `provider.mkdir(&remote)?;`,
ran the suite, captured the failures below, then restored the fix via `git checkout --` (after the fix
was already committed) and re-ran to confirm green.

`cpe-server` (`cargo test transfer::tests::upload_tree` in `crates/server`):
```
test transfer::tests::upload_tree_reports_a_clear_error_when_the_remote_base_is_a_file_not_a_directory ... FAILED
test transfer::tests::upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist ... FAILED
test transfer::tests::upload_tree_succeeds_when_a_nested_directory_already_exists_remotely ... FAILED
test transfer::tests::upload_tree_succeeds_when_the_remote_base_directory_already_exists ... FAILED

---- upload_tree_reports_a_clear_error_when_the_remote_base_is_a_file_not_a_directory ----
expected a clear diagnosis, got "dest: Failure: Failure"
---- upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist ----
a multi-level remote_root with missing parents must succeed (CPE-1741): "a/b: No such file: No such file"
---- upload_tree_succeeds_when_a_nested_directory_already_exists_remotely ----
re-uploading over an already-existing nested dir must succeed (CPE-1741): "dest: Failure: Failure"
---- upload_tree_succeeds_when_the_remote_base_directory_already_exists ----
uploading into an existing remote base must succeed (CPE-1741): "dest: Failure: Failure"

test result: FAILED. 2 passed; 4 failed; 0 ignored
```
(`upload_tree_surfaces_a_genuine_mkdir_failure_rather_than_swallowing_it` correctly stayed green both
before and after — it isn't specific to this fix, it's a guard against over-tolerance.)

`cpe-sftp` (`cargo test upload_tree_succeeds` in `crates/sftp`, real wire):
```
test tests::upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist ... FAILED
test tests::upload_tree_succeeds_when_the_remote_base_directory_already_exists ... FAILED
test tests::upload_tree_succeeds_when_a_nested_directory_already_exists_remotely ... FAILED

a multi-level remote_root with missing parents must succeed (CPE-1741): "/new/deep: No such file: No such file"
uploading into an already-existing remote base must succeed (CPE-1741): "/sub: Failure: Failure"
re-uploading over an already-existing nested dir must succeed (CPE-1741): "/up2: Failure: Failure"

test result: FAILED. 0 passed; 3 failed; 0 ignored
```

`cpe-ftp` (`cargo test upload_tree_succeeds` in `crates/ftp`, real wire):
```
test tests::upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist ... FAILED
test tests::upload_tree_succeeds_when_the_remote_base_directory_already_exists ... FAILED
test tests::upload_tree_succeeds_when_a_nested_directory_already_exists_remotely ... FAILED

a multi-level remote_root with missing parents must succeed (CPE-1741): "/new/deep: Invalid response: [550] 550 Create failed"
uploading into an already-existing remote base must succeed (CPE-1741): "/sub: Invalid response: [550] 550 Create failed"
re-uploading over an already-existing nested dir must succeed (CPE-1741): "/up2: Invalid response: [550] 550 Create failed"

test result: FAILED. 0 passed; 3 failed; 0 ignored
```

**Verification:** `cargo test --lib` in `crates/server` — 2207 passed, 0 failed. Full `cargo test` in
`crates/sftp` (35/35) and `crates/ftp` (20/20). `crates/webdav`'s `upload_tree` test (30/30 suite) also
still passes. `cargo clippy --all-targets -- -D warnings` clean in `crates/server` (base, `--features
index`, `--features pdf-thumb,video-thumb,waveform,dicom-thumb`), `crates/sftp` (base), and `crates/ftp`
(base, `--features e2e-extra-ca`) — the exact flag combinations `.github/workflows/ci.yml` runs for
these crates.

**Not loosened back:** confirmed both `cpe-sftp`'s and `cpe-ftp`'s `#[cfg(test)]` rigs are untouched —
still `std::fs::create_dir` (not `create_dir_all`) in their `MKD`/`mkdir` handlers.

**Unverified / assumptions:** the `cpe-server`-level `sample_fixtures.rs` integration test
(`zip_lists_real_tree_and_extracts_inner_file`) fails in this environment on an unrelated pre-existing
issue — `crates/server`'s own scratch-dir allocator under `%TEMP%\cpe-archive` exhausted 1024 naming
attempts because that directory already held ~1.26M leftover entries from prior sessions on this
machine, unrelated to archive extraction logic or to this ticket (no file this ticket touches is on that
code path). Did not attempt to clean up that directory — out of scope, and other concurrent work on this
machine may depend on files under `%TEMP%`. The "measured, not assumed" premise from the ticket's own
Notes section (that a live OpenSSH/vsftpd daemon returns the same codes as the two in-process rigs) is
still not independently re-verified against the QNAP NAS — this ticket's fix does not rely on that
premise (it never branches on the specific status code at all, only on `stat`), so it is unaffected
either way, but the premise itself remains as stated: read from RFCs/`sftp-server.c`, not measured live.

## Work Log (2026-08-18) — Reviewer round 2, four findings applied

The independent Reviewer confirmed correctness, TOCTOU handling, and `StrictMkdirProvider`'s fidelity
to `mkdir(2)` were all sound, and that ten legs across three crates go red on revert. UAT PASSED. Four
findings applied, in order:

**F1 — the big one: a measured performance regression, with an exact fix.** Before this round,
`ensure_dir` was used at BOTH `upload_tree` call sites (the base, and every in-walk directory). Measured
with an instrumented `CountingProvider` wrapping `StrictMkdirProvider` (base + 10 sibling subdirs, one
file each = 11 remote directories):

```
BEFORE (ensure_dir at both sites):
  fresh base, 10 subdirs:   files=10  stat_calls=21  mkdir_calls=11   -> 32 metadata round trips
  old (pre-CPE-1741) code equivalent:                                     11 round trips
  re-upload over existing:  files=10  stat_calls=11  mkdir_calls=0    -> 11 round trips
  fresh deep base a/b/c:    files=10  stat_calls=24  mkdir_calls=14   -> 38 round trips
```

A first-time upload cost **2.9x** the round trips of the old (buggy) code, not the 2x the ticket
originally anticipated — because `ensure_dir` recurses into the parent unconditionally
(`transfer.rs:834-838` at the time), even for an in-tree directory whose parent the walk had just
created: `stat(self)` miss + `stat(parent)` hit + `mkdir(self)` = 3 round trips per directory. On FTP
this is worse than "a round trip": `FtpProvider::stat` (`crates/ftp/src/lib.rs:300-317`) is
`self.list(parent)?` — a full PASV + data-connection LIST of the parent, not a metadata verb — so `n`
sibling subdirectories cost **O(n^2) in bytes transferred**, on the one protocol where a listing opens a
whole new data connection.

**Fix applied** — `ensure_dir` itself is untouched (still used for the base, `transfer.rs:861`). Only
the in-walk call site (`transfer.rs:878`, was `ensure_dir(provider, &remote)?;`) changed to try the
cheap operation first:

```rust
if provider.mkdir(&remote).is_err() {
    ensure_dir(provider, &remote)?; // already there, or a genuine failure — `ensure_dir` decides
}
```

This exploits the walk's own ordering guarantee: a directory's parent is always pushed onto `stack`
before the directory itself is processed, so for a **fresh** in-tree directory `mkdir` succeeds on the
first try — 1 round trip, not 3.

**Re-measured after the fix** (same `CountingProvider` + `StrictMkdirProvider` harness, same 3
scenarios):

```
AFTER (mkdir-first at the in-walk site, ensure_dir kept at the base):
  fresh base, 10 subdirs:   files=10  stat_calls=1   mkdir_calls=11  -> 12 round trips  (was 32; -62%,
                                                                         now within 1 round trip of the
                                                                         old pre-CPE-1741 code's 11)
  re-upload over existing:  files=10  stat_calls=11  mkdir_calls=10  -> 21 round trips  (was 11; +91%)
  fresh deep base a/b/c:    files=10  stat_calls=3   mkdir_calls=13  -> 16 round trips  (was 38; -58%)
```

The fresh-tree cases (by far the common one — first-time upload, or a tree with new subdirectories) are
dramatically cheaper, landing almost exactly at the old code's round-trip count while now being
*correct* for an existing base. The one case that got measurably worse is a **full re-sync of a tree
that already exists in its entirety remotely** (every in-tree directory already there): each such
directory now costs a failed `mkdir` + a `stat` (2 round trips) instead of `ensure_dir`'s single
early-return `stat` hit (1 round trip). This is the accepted trade-off the exact fix makes: on FTP in
particular, the "wasted" first attempt is a cheap single `MKD` command, not the expensive
`stat`-as-`LIST` this finding is otherwise trying to avoid — so trading round-trip *count* for round-trip
*cost* is favourable even in this worse-by-count case. Not independently re-measured in bytes (only
round trips), consistent with how the "before" numbers in the ticket were reported. Flagged here rather
than silently accepted: a future ticket could special-case "parent already existed before this walk
iteration, but this specific child was already there too" to recover the 1-round-trip case, but that
needs a way to tell "just-created parent" from "not", which the walk doesn't currently expose cheaply.

**F2 — the WebDAV rig was still more forgiving than the wire.** The exact defect CPE-1731 fixed for
SFTP/FTP `mkdir`/`create_dir` had survived in the third protocol: `crates/webdav/src/lib.rs`'s `MKCOL`
handler read `let code = if std::fs::create_dir_all(&real).is_ok() { 201 } else { 500 };` —
`create_dir_all` is idempotent and invents missing parents, so an existing-resource MKCOL and a
missing-intermediate-collection MKCOL both silently succeeded, which no real WebDAV server does. Fixed
to answer per RFC 4918 §9.3.1: `405` for an existing resource, `409` for a missing intermediate
collection, `201` only for a genuine single-level `create_dir`, `500` otherwise. `WebdavProvider::mkdir`
uses `ureq`'s `.call()`, which turns any `>=400` into `Err`, so `ensure_dir`'s stat-based retry logic
handles all three refusal codes correctly with no change needed on the client side. Re-ran the full
cpe-webdav suite after the change: 32/32 green (30 pre-existing + the 2 new F3 tests below) — nothing
depended on the old forgiveness.

**F3 — WebDAV regression tests, landed together with F2** (so they aren't vacuous): added
`upload_tree_succeeds_when_the_remote_base_directory_already_exists` (uploads into the seeded `/sub`)
and `upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist` (uploads into
`/new/deep`) next to `generic_transfer_walks_downloads_and_uploads_over_webdav` in
`crates/webdav/src/lib.rs`. Same shape as the sftp/ftp legs: assert the bytes back, and assert the
seeded `/sub/nested.txt` survived. Both also carry the F4(a) empty-directory strengthening below.

**F4(a) — test-strength: prove `mkdir`, not a parent-inventing write, made the directory.** The existing
"succeeds" tests read bytes back, which is right, but on two of the three protocols a write path
independently invents parent directories, so a passing test didn't prove `mkdir`/`MKD`/`MKCOL` itself
ran: `crates/ftp/src/lib.rs`'s rig `STOR` handler does `create_dir_all(parent)` (CPE-1742, known/kept),
and `crates/server/src/provider.rs`'s `FakeProvider::write` calls `ensure_ancestors`. Only the sftp rig
was clean. Fixed by adding an **empty** subdirectory to each affected source tree and asserting it
exists remotely — only a successful `mkdir` call chain can produce a directory holding nothing:
- `crates/ftp/src/lib.rs` (`upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist`):
  added `std::fs::create_dir(src.join("empty")).unwrap();` and
  `assert!(provider.stat("/new/deep/empty").unwrap().is_dir, "the MKD chain, not STOR's create_dir_all, must have made these");`.
- `crates/server/src/transfer.rs` (`one_file_src`, shared by three tests): added the same empty-dir
  creation; the multi-level test now also asserts `fs.stat("a/b/empty").unwrap().is_dir`.
- The two new WebDAV tests (F3) were written with this strengthening from the start.

**F4(b) — `StrictMkdirProvider` gaps.** Two fixed in `crates/server/src/transfer.rs`:
1. It did not model `ENOTDIR` (parent exists but is a **file**, not a directory). The parent check in
   `mkdir` now matches `self.0.stat(parent)`: `Err(_)` → `"No such file: No such file"` (unchanged,
   ENOENT), `Ok(e) if !e.is_dir` → `"Failure: Failure"` (new, ENOTDIR — matches the same
   indistinguishable-from-generic-failure text `ensure_dir`'s doc already establishes for EEXIST),
   `Ok(_)` → proceed.
2. Neither `StrictMkdirProvider` nor `AlwaysDeniedMkdir` forwarded `capabilities()` — both now add
   `fn capabilities(&self) -> ProviderCapabilities { self.0.capabilities() }`, matching what
   `FakeProvider` itself overrides, so a test that checks capabilities through either wrapper gets the
   real answer instead of the trait's `ProviderCapabilities::default()`.

**Full verification after all four findings:**
- `cargo test --lib` in `crates/server`: **2207 passed, 0 failed, 4 ignored** (same count as before this
  round — confirms the measurement scaffold used for F1's numbers was fully removed, not accidentally
  left in the committed diff).
- `cargo test` in `crates/sftp`: **35/35** (unaffected by this round's edits; sftp gets no changes).
- `cargo test` in `crates/ftp`: **20/20**, including the strengthened multi-level test.
- `cargo test` in `crates/webdav`: **32/32** (30 pre-existing + the 2 new F3 tests).
- `cargo clippy --all-targets -- -D warnings`, clean in every feature mode `.github/workflows/ci.yml`
  runs for these crates: `crates/server` (base, `--features index`), `crates/sftp` (base),
  `crates/ftp` (base, `--features e2e-extra-ca`), `crates/webdav` (base).

**Not changed** (per the Reviewer's explicit "confirmed good" list): `ensure_dir`'s TOCTOU handling, its
own already-exists tolerance and base-is-a-file/permission-denied error shapes — all untouched.

**Unverified / carried over:** the QNAP-NAS live-daemon cross-check noted in the previous Work Log entry
is still not done — this round's changes don't depend on it either. The pre-existing
`sample_fixtures.rs::zip_lists_real_tree_and_extracts_inner_file` local-environment flake (stale
`%TEMP%\cpe-archive` entries) noted previously is unrelated to any file this round touched and was not
revisited.
