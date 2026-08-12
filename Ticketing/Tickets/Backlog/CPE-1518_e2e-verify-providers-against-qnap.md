---
id: CPE-1518
title: "E2E-verify shipped SFTP/WebDAV/FTP providers against a real QNAP TS-133 NAS"
type: Task
status: Backlog
priority: High
component: Multiple
tags: [ready]
epic: CPE-1498
created: 2026-08-09
---
## Why
The SFTP (crates/sftp), WebDAV (crates/webdav), and FTP/FTPS (crates/ftp) providers, the connection-secret
keychain (CPE-1510), remote `list_dir` routing (CPE-1511), and SFTP host-key TOFU (CPE-1512) are all **merged
and unit-tested**. The user has a **QNAP TS-133** NAS on the LAN whose QTS exposes exactly these protocols.

**Edited down (CPE-1659):** protocol-level correctness — connect/browse/read/write/mkdir/rename/delete, error
shape, and SFTP host-key TOFU (both the `Trusted` path and a *changed* key being refused) — is now proven
**headlessly, in CI, on every push/PR** by the `Network E2E (ubuntu-latest, real servers)` job
(`.github/workflows/ci.yml`, `crates/vfs/tests/real_server_conformance.rs`), against real OpenSSH
`sftp-server`, real Apache `mod_dav`, and real vsftpd — the same daemons QTS itself runs. That headless rig
discharges the "two-host network E2E" / "tested only against a fake server we wrote" debt this ticket used to
carry (MANUAL-TEST-BURNDOWN rows #7 and #13). **This ticket keeps only the residue that rig genuinely cannot
reach**, because it isn't a protocol-interop question at all: the built app's GUI + OS keychain + a specific
piece of physical hardware.

## Scope (attended, needs the NAS + the user's LAN) — genuine residue only
- **QTS-specific behaviour** — from the *built sidecar app* (not a headless test), add a connection to the
  QNAP's WebDAV/FTP/SFTP shares, confirm the secret round-trips through the **real OS keychain** ("remember"
  toggle) and reconnect works after an app restart. This exercises the app-level integration (keychain +
  connection profile UI) CPE-1659's Rust-level rig never touches — it calls `cpe_vfs::open` directly with an
  in-memory secret, not through the keychain or the connections UI. Note anything QTS's real implementation
  does differently from the generic OpenSSH/mod_dav/vsftpd images CPE-1659 tests against (QTS is known to run
  slightly customized builds of some of these daemons).
- **Live LAN discovery records for CPE-1517** — while connected, capture the QNAP's advertised mDNS/DNS-SD
  records (`dns-sd -B` / an mDNS probe) so the discovery epic has real fixture data. No headless substitute
  exists for this — it needs the physical device answering on the real LAN.
- **Sidebar state-dot check against a real device** — confirm the Network section (CPE-1513 / CPE-1516) shows
  connection **state dots** correctly (connected / disconnected / error) against the live QNAP, including what
  happens when the device is powered off/unreachable mid-session. This is a live-hardware presence signal, not
  something a container can stand in for.

## Verify / acceptance
- A short checklist result per protocol (app-level connect ✓, secret persisted ✓, reconnect after restart ✓),
  captured in the Work Log. Any defect found → its own fix ticket.
- The QNAP's advertised discovery records, captured for CPE-1517's fixture data.
- A screenshot/note of the sidebar state dot in each of connected/disconnected/error against the real device.

## Notes
Attended (user + hardware) — this is the residue CPE-1659's headless rig cannot reach: it can't be faked
headless. Sits on merged CPE-1510/1511/1512/1513/1514/1515, and on **CPE-1659** for everything that WAS
faked-headless-able (protocol interop + host-key TOFU) — see that ticket/job for the coverage this ticket used
to carry.
Real-NAS reference: [[network-filesharing-program-2026-08-08]] research entry + the CPE-1517 discovery epic.
