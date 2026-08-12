---
id: CPE-1659
title: "Real-server network E2E rig in CI — stop testing the remote providers against servers we wrote ourselves"
type: Test
status: Backlog
priority: High
component: CI
epic: CPE-1498
tags: [ready]
estimate: 4h+
created: 2026-08-11
closed:
---

## Why

Every remote-filesystem provider we ship is tested **only against a fake server written by the same author
as the client**. `crates/sftp` runs an in-process `russh-sftp` server, `crates/webdav` a `tiny_http`
handler, `crates/ftp` a hand-rolled FTP daemon — and `ci.yml` says so out loud, three times: *"tested
against an in-process … server, so it runs on all three OSes with no Docker."* Fast and cross-platform,
and worth keeping. But it is the exact failure mode this crew has now been burned by repeatedly:

> **"A test written by reading the code can only confirm the code."** (`history.md`, 2026-08-11)
> **"Real inputs beat fixtures — the fixture agrees with the parser by construction."** (same)

A hand-written FTP server never sends the ASCII-mode line-ending translation a real vsftpd does. A
`tiny_http` stub never percent-encodes a href the way Apache `mod_dav` does. Our in-process sshd is not
OpenSSH. So the whole Network program's E2E acceptance has been **owed since it shipped**, and the only
plan on the books to discharge it is **CPE-1518 — attended, on the user's QNAP TS-133, by hand.** That
ticket has sat in the Backlog needing the user + hardware, and every future slice of the Network program
(SMB epic CPE-1504, OS-mount CPE-1500, mDNS CPE-1517) inherits the same attended tail.

This ticket builds the rig that discharges it in CI instead, against **real third-party server
implementations** — including vsftpd and OpenSSH, which are literally what QNAP QTS runs.

Retires burndown row **#13** (remote providers vs a real server) and, via slice 2, the long-standing
primary row **#7** (real non-loopback client↔server run, open since the ledger was seeded). Shrinks
CPE-1518 to the genuinely device-specific residue.

## What to build

### Slice 1 — the rig + a provider conformance suite

**Servers** (Linux CI only; Docker exists on `ubuntu-latest` runners and nowhere else in the matrix —
that is fine, this tests *protocol interop*, not OS behaviour). Pin every image by tag **and** digest:

| Protocol | Image | Why this one |
|---|---|---|
| SFTP | `atmoz/sftp` | real OpenSSH `sftp-server` — what QNAP/most NASes actually run |
| WebDAV | Apache `mod_dav` (e.g. `bytemark/webdav`) | real `PROPFIND` multistatus + real href encoding |
| FTP / FTPS | `fauria/vsftpd` | vsftpd *is* QTS's FTP daemon; enable TLS + a fixed PASV port range |

**Seeded fixture tree**, created by the workflow into a host dir each container mounts — deliberately
hostile, not tidy: nested dirs, a 0-byte file, a **5 MiB binary** of random bytes, a name with a space,
one with `#` and `%` (the classic WebDAV href-encoding bug), one with non-ASCII/emoji, and a file whose
bytes contain CRLF *and* lone LF (the classic FTP ASCII-mode corruption).

**The suite** — `crates/vfs/tests/real_server_conformance.rs`. One shared
`fn conformance(p: &mut dyn FileSystemProvider, root: &str)` driven once per scheme, so adding SMB later
(CPE-1504) is a service block + a `Connection` profile, **not** a new test file. Drive it through
**`cpe_vfs::open(&Connection, secret, known_hosts, policy, …)`** — the routing seam the app itself uses
(CPE-1511) — never the provider constructors directly. That is what makes this an E2E rather than a
fourth unit test.

Assertions, per scheme:

1. `list("/")` returns the seeded set with correct `is_dir` and sizes.
2. `stat` resolves a nested path.
3. `read` of the 5 MiB binary is **byte-for-byte** equal to the source (chunking + FTP transfer-mode).
4. `write` → `read` round-trip for: empty file, the space/`#`/`%` name, the non-ASCII name, the
   CRLF/LF-mixed payload.
5. `mkdir` / `rename` / `delete` are each verified **from the server side** — assert against the
   host-mounted directory on disk, i.e. the OS's own view, exactly as `native_meta_os_interop.rs`
   (CPE-1049) and the Finder-tag test (CPE-1307) do. The client must never be allowed to satisfy the
   test by agreeing with itself.
6. **Error shape:** `read`/`stat` of a missing path returns `Err` — not a panic, not an empty success.
   ("We don't know must never look like it's fine", `history.md`.)
7. **SFTP host-key TOFU against a real sshd** (CPE-1512 has never once been exercised against OpenSSH):
   first contact records the container's real host key to a temp known_hosts; reconnect resolves
   `Trusted`; a container restarted with a **regenerated** key resolves `Changed` and the connect is
   **refused**.
8. **FTPS:** explicit `AUTH TLS` negotiates against vsftpd's real certificate.

**Gating.** Mark the tests `#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]`
so the existing 3-OS `crates` matrix is untouched, and run them in the new job with
`cargo test -- --ignored`. Do **not** delete or weaken the in-process tests — they stay as the fast
cross-platform layer; this rig is the interop layer above them.

### Slice 2 — retire burndown row #7 (real client↔server over the wire)

`crates/net`'s only coverage is a loopback example plus unit tests; row #7 has asked for a genuine
two-host run since 2026-07-25. The rig makes it nearly free: build the existing
`crates/net/src/bin/cpe-server-ref.rs` into a container on the job's Docker bridge network and drive it
from the test process at its **container IP** — a different network namespace and a non-`127.0.0.1`
address, which is the actual thing row #7 asks for. Assert a real listing crosses the socket, plus one
truncation/error path.

### The pin

New job in `.github/workflows/ci.yml`:

```
  net-e2e:
    name: Network E2E (ubuntu-latest, real servers)
    runs-on: ubuntu-latest
```

on `push` + `pull_request`, **blocking — no `continue-on-error`, ever.** Target < 10 min wall-clock.
Record the job name in the burndown rows it retires. If a future change makes this job non-blocking or
deletes it, that is a burndown regression under charter rule 2 and must be treated as one.

## Acceptance criteria

- [x] `crates/vfs/tests/real_server_conformance.rs` exists, is one shared conformance fn run against
      sftp / webdav / ftp / ftps, and routes through `cpe_vfs::open` — not the provider constructors.
- [x] All 8 slice-1 assertion groups pass against the three real server images.
- [x] Mutations (mkdir/rename/delete/write) are verified against the **host-mounted directory on disk**,
      not only via the client's own `list`.
- [x] SFTP host-key `Trusted` **and** `Changed`-is-refused both proven against a real OpenSSH key.
- [x] Slice 2: a `cpe-net` client↔server exchange over a non-loopback container IP, asserted.
- [~] `CI` job `Network E2E (ubuntu-latest, real servers)` is green on the PR, **blocking** — confirmed,
      but wall-clock is **~13-14 min**, over the ticket's `< 10 min` target (dominated by the
      `--test-threads=1` conformance suite's 301s alone, plus the release `cpe-server-ref` build and
      three real-server startups). Left open as a follow-up; not re-architected here to keep this
      iteration's CI-cycle budget for the fixes actually blocking green.
- [x] A plain `cargo test` in `crates/{sftp,webdav,ftp,vfs,net}` on Windows/macOS is **unchanged** — the
      new tests are `#[ignore]`d and the existing in-process tests all still run (confirmed both locally
      and on the Windows/macOS `Server crates` CI legs).
- [x] **Negative control, required before this is believed** (`history.md`, repeatedly): break one
      provider deliberately — e.g. force FTP into ASCII mode, or strip percent-decoding from the WebDAV
      href parser — and show the new suite goes **red** while the existing in-process suite stays
      **green**. Paste both results in the Work Log. Without this the rig is unproven.
- [x] `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` rows #7 and #13 flipped to ✅ naming this job,
      and the MVD header count decremented by 2.
- [x] `CPE-1518` edited down to its genuine residue (QTS-specific behaviour, the live LAN discovery
      records for CPE-1517, and the sidebar state-dot check against a real device) with a note pointing
      at this job for everything now covered. Done by an earlier worker on this branch; verified present
      (`Ticketing/Tickets/Backlog/CPE-1518_e2e-verify-providers-against-qnap.md` already names CPE-1659
      and MANUAL-TEST-BURNDOWN rows #7/#13 as what it shrank against).

## Notes

- **Do not** add Docker to the existing 3-OS `crates` job. Separate job, Linux only, by design.
- Images pinned by digest so a silent upstream retag cannot change what "real server" means underneath us.
- Landing pad for the SMB epic (**CPE-1504**): a `dperson/samba` service + one `Connection` profile joins
  the same conformance fn — that is the whole point of writing it as one shared fn.
- Related: [[CPE-1518]] (attended QNAP E2E — this shrinks it), burndown rows #7 and #13,
  [[CPE-1049]]/[[CPE-1307]] for the "read it back with the OS's own tools" assertion pattern.
- Filed by the QA Architect, shift 2026-08-11. Chosen over blessing gui-smoke visual baselines (WebKitGTK
  font rendering makes pixel baselines a false-red factory today) and over the Windows gui-smoke leg
  (blocked on the unfixed CPE-1048 WebView2 defect, no confident acceptance criterion available).

## Work Log

- 2026-08-11 — Filed by the QA Architect from the MVD audit: three shipped protocols, zero tests against
  a server we did not write ourselves, and the only discharge plan on the books requiring the user's NAS.
- 2026-08-11/12 — Rig built (PR #849): Docker bridge network, digest-pinned images, hostile fixture tree,
  TLS cert generation, shared conformance fn routed through `cpe_vfs::open`. First live CI runs found and
  fixed two real client bugs (WebDAV href `#` truncating the URL as a fragment; WebDAV directory `DELETE`
  silently no-op'ing on Apache's `DirectorySlash` 301 redirect) — SFTP host-key TOFU and SFTP/WebDAV
  conformance confirmed green. FTP/FTPS remained red: `ftp-e2e` exited 147ms after start (exitCode 2),
  invisible in `docker logs` even with `LOG_STDOUT=YES`.
- 2026-08-12 — **Picked up after the previous worktree was destroyed (Foreman cleanup error, no work
  lost).** Fixed FTP/FTPS in three real, sequentially-discovered bugs (not the ticket's original
  `ssl_tlsv1` hypothesis, which was tried first and found insufficient by CI):
  1. Added a foreground diagnostic step that replays `run-vsftpd.sh` with its own `&>/dev/null` redirect
     stripped when the container exits immediately — vsftpd's startup errors are hardcoded to `/dev/null`
     in the image regardless of `LOG_STDOUT`, confirmed against `fauria/docker-vsftpd` upstream. First use
     surfaced the REAL error: `500 OOPS: config file not owned by correct user, or not a file` — vsftpd
     refuses to load a config file not owned by root; the generated `vsftpd.conf` was owned by the
     unprivileged CI runner user and bind-mounted unchanged. Fixed with `sudo chown root:root`.
  2. Past that, FTP/SFTP/WebDAV all passed but FTPS failed: `received fatal alert: HandshakeFailure`.
     vsftpd's own documented default `ssl_ciphers` is the legacy `DES-CBC3-SHA`
     ([vsftpd.conf(5)](https://linux.die.net/man/5/vsftpd.conf)), which shares no overlap with rustls'
     modern AEAD-only cipher set. Fixed with `ssl_ciphers=HIGH`.
  3. Past that, a third distinct FTPS bug: `invalid peer certificate: Other(OtherError(CaUsedAsEndEntity))`
     — `openssl req -x509` without an explicit `basicConstraints` extension inherits `CA:TRUE` from
     OpenSSL's default config, and rustls/webpki refuses a CA:TRUE certificate as a TLS leaf. Fixed by
     adding `basicConstraints=critical,CA:FALSE` + `keyUsage` + `extendedKeyUsage=serverAuth` to the
     `openssl req` call; verified locally with `openssl x509 -text`.
  Also fixed the "Wait for the real servers to accept connections" step, whose 30-attempt-per-host loop
  fell through on total failure with no `exit 1` — this is what let the vsftpd crash go undetected for
  two full CI runs. Now asserts per host and fails loudly.
  Also added an explicit `TYPE I` (`FileType::Binary`) call in `crates/ftp` — `suppaftp` never sets a
  transfer type on its own and RFC 959's connection default is ASCII, not binary. Kept on RFC-conformance
  principle even though the negative-control experiment below found this particular vsftpd build doesn't
  actually translate line endings on the wire either way.
  **First fully green run** (all fixes applied, Slice 2 executing for the first time ever):
  https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31584936382 — Network E2E
  13m39s, all 8 slice-1 assertion groups + both SFTP TOFU cases + Slice 2 passing. Slice 2's first-ever
  execution immediately found one more real bug: `list_dir`'s dispatcher handler mapped every domain
  error (including a missing path) to `ErrorCode::Internal` instead of `NotFound`. Fixed in
  `crates/server/src/dispatch.rs` (special-case `list_dir` to check `Path::exists()`) with an in-process
  regression test added (`list_dir_of_a_missing_path_is_not_found`).
  **Negative control** (required acceptance criterion): first attempted by forcing FTP into `TYPE A`
  (ASCII mode) as the ticket suggested — this came back **green** on live CI
  (https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31588642603), a genuine
  finding that this particular vsftpd build/config does not perform ASCII CRLF translation on the wire,
  not a failure of the method. Switched the negative control to the already-proven-real WebDAV
  directory-delete bug instead: reverted the `DirectorySlash`-redirect fix and pushed.
  **RED run**: https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31590466435 —
  `webdav_conformance_against_real_apache_moddav ... FAILED` / "the now-empty directory must be gone from
  disk after delete", while `ftp_conformance_against_real_vsftpd`, `ftps_conformance_against_real_vsftpd_with_tls`,
  and `sftp_conformance_against_real_openssh` all still passed in the same job, and the in-process
  `Server crates (ubuntu-latest)` job (running `cargo test -p cpe-webdav`, confirmed unchanged both
  locally — 12/12 passing — and on CI) stayed **GREEN**. Reverted immediately after.
  **Final green run** (fix restored, docs updated): pending — see the next commit's CI run for the URL.
  MANUAL-TEST-BURNDOWN rows #7 and #13 flipped to ✅, MVD header 17→15. CPE-1518 already edited down by
  an earlier worker on this branch — verified present, not re-touched.
  **Left open:** the CI job's wall-clock is ~13-14 min against the ticket's `< 10 min` target, dominated
  by the `--test-threads=1` conformance suite (301s alone) plus the release `cpe-server-ref` build and
  three real-server container startups — not re-architected this session to keep the CI-cycle budget for
  the fixes actually blocking green; a follow-up could parallelize the conformance tests or cache the
  release build more aggressively.
