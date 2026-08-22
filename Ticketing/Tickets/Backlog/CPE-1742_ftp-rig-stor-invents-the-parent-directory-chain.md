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
