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

- [ ] `crates/vfs/tests/real_server_conformance.rs` exists, is one shared conformance fn run against
      sftp / webdav / ftp / ftps, and routes through `cpe_vfs::open` — not the provider constructors.
- [ ] All 8 slice-1 assertion groups pass against the three real server images.
- [ ] Mutations (mkdir/rename/delete/write) are verified against the **host-mounted directory on disk**,
      not only via the client's own `list`.
- [ ] SFTP host-key `Trusted` **and** `Changed`-is-refused both proven against a real OpenSSH key.
- [ ] Slice 2: a `cpe-net` client↔server exchange over a non-loopback container IP, asserted.
- [ ] `CI` job `Network E2E (ubuntu-latest, real servers)` is green on the PR, **blocking**, < 10 min.
- [ ] A plain `cargo test` in `crates/{sftp,webdav,ftp,vfs,net}` on Windows/macOS is **unchanged** — the
      new tests are `#[ignore]`d and the existing in-process tests all still run.
- [ ] **Negative control, required before this is believed** (`history.md`, repeatedly): break one
      provider deliberately — e.g. force FTP into ASCII mode, or strip percent-decoding from the WebDAV
      href parser — and show the new suite goes **red** while the existing in-process suite stays
      **green**. Paste both results in the Work Log. Without this the rig is unproven.
- [ ] `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` rows #7 and #13 flipped to ✅ naming this job,
      and the MVD header count decremented by 2.
- [ ] `CPE-1518` edited down to its genuine residue (QTS-specific behaviour, the live LAN discovery
      records for CPE-1517, and the sidebar state-dot check against a real device) with a note pointing
      at this job for everything now covered.

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
