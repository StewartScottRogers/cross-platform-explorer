---
id: CPE-1741
title: upload_tree fails when the remote base directory already exists
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
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
