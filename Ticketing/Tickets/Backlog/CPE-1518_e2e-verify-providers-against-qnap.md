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
and unit-tested**, but their end-to-end path has only ever been exercised against fakes/local fixtures — the
"two-host network E2E" acceptance has been owed the whole program. The user now has a **QNAP TS-133** NAS
(arriving 2026-08-10) whose QTS exposes exactly these protocols, giving the first real remote to verify against.

## Scope (attended, needs the NAS + the user's LAN)
On the QNAP, enable the relevant file services (Control Panel → Network & File Services), then from the built
sidecar app verify each shipped protocol against it:
- **WebDAV / WebDAVS** — add a connection to the QNAP WebDAV share, connect, browse, read a file, confirm the
  secret round-trips through the keychain ("remember" toggle) and reconnect works after restart.
- **FTP / FTPS** — same round trip; confirm explicit-TLS (FTPS) negotiates and the anonymous/user paths both
  behave.
- **SFTP** — enable SSH on the QNAP; confirm **host-key TOFU** records the QNAP's key on first contact
  (app known_hosts, not ~/.ssh), and that a *changed* key is correctly refused (CPE-1512), plus key + password
  auth.
- Confirm the Network section (CPE-1513 / CPE-1516) shows connection **state dots** correctly (connected /
  disconnected / error) against the live device, and that bounded/streamed reads behave over a real link.

## Verify / acceptance
- A short checklist result per protocol (connected ✓, browsed ✓, read ✓, secret persisted ✓, reconnect ✓),
  captured in the Work Log. Any defect found → its own fix ticket.
- Note the QNAP's advertised discovery records (for CPE-1517) while connected (`dns-sd -B` / a mDNS probe) so
  the discovery epic has real fixture data.

## Notes
Attended (user + hardware) — this is the honest E2E the program has owed; it can't be faked headless. Turns the
"two-host E2E" owe into a concrete, hardware-backed pass. Sits on merged CPE-1510/1511/1512/1513/1514/1515.
Real-NAS reference: [[network-filesharing-program-2026-08-08]] research entry + the CPE-1517 discovery epic.
