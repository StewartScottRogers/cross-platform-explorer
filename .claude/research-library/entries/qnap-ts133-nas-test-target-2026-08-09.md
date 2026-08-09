# Reference — QNAP TS-133 NAS as the Network program's real test target (2026-08-09)

**Device:** QNAP **TS-133-US**, 1-bay desktop NAS, ARM Cortex-A55 quad-core, running **QTS** (QNAP's OS).
Ordered by the user via Amazon ASIN `B0GTX16PW4`; **arrives / installed 2026-08-10**. First real remote host
for E2E-verifying the Network filesharing program (until now everything was faked/local).

## Protocols QTS exposes (Control Panel → Network & File Services), with default ports
| Protocol | Port(s) | CPE status |
|----------|---------|------------|
| **SMB/CIFS (Samba)** | TCP 445 | Primary QNAP service. In-app: epic **CPE-1504** (unbuilt); Windows native-UNC + OS-mount **CPE-1500** are the zero-new-crate v1 — testable now. |
| **AFP** | TCP 548 | Not covered; Apple-deprecated in favour of SMB. Low priority (noted in CPE-1517 follow-ups). |
| **NFS** | TCP/UDP 111 + 2049 | Epic **CPE-1505** (unbuilt); OS-mount via CPE-1500. |
| **FTP / FTPS** | TCP 21 (+ TLS) | **Shipped** — `crates/ftp`. Verify via **CPE-1518**. |
| **WebDAV / WebDAVS** | HTTP/HTTPS | **Shipped** — `crates/webdav`. Verify via **CPE-1518**. |
| **SFTP (SSH)** | TCP 22 (enable SSH) | **Shipped** — `crates/sftp` + host-key TOFU CPE-1512. Verify via **CPE-1518**. |
| **rsync** | TCP 873 | Not covered; low-priority future protocol (noted in CPE-1517). |
| **iSCSI** | TCP 3260 | Block storage — **out of scope** for a file explorer (mounts as a local disk, not a share). |

## LAN discovery the NAS advertises (feeds new epic CPE-1517)
mDNS/DNS-SD (UDP 5353): `_smb._tcp`, `_afpovertcp._tcp`, `_nfs._tcp`, `_webdav._tcp`/`_webdavs._tcp`,
`_ftp._tcp`, `_sftp-ssh._tcp`, `_http._tcp`, plus QNAP's `_qnap._tcp`. Also SSDP/UPnP (UDP 1900) for
DLNA/media, and SMB/NetBIOS browse (UDP 137). Capture the live records with `dns-sd -B` / an mDNS probe while
connected (CPE-1518 asks for this) → real fixtures for the discovery listener.

## What this unblocks
- **Now, no new code:** SMB via Windows `\\<qnap>\<share>` (CPE-1504 v1 leg / CPE-1500).
- **Now, attended:** E2E for the shipped SFTP/WebDAV/FTP + keychain + TOFU (**CPE-1518**).
- **Next:** LAN auto-discovery (**CPE-1517**), NFS + non-Windows SMB via OS-mount (**CPE-1500/1504/1505**).

## Sources
- QNAP file-services / service-ports docs: https://www.qnap.com/en/solution/file-server ,
  https://docs.qnap.com/operating-system/qts/5.2.x/en-us/qnap-service-ports-C25795F.html
- Product page (ASIN B0GTX16PW4 → QNAP TS-133-US): https://www.amazon.com/dp/B0GTX16PW4
- Related prior research: [[network-filesharing-program-2026-08-08]], [[competitive-file-managers-2026-08-08]]
