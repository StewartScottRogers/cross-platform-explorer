---
title: Network Connections
order: 31
category: Network & Remote
categoryOrder: 8
---

# Network Connections

The **Network** section in the left-pane sidebar is the entry point for browsing a remote server —
SFTP or WebDAV — right alongside your local drives. It's a **permanent section**, always shown as a
peer of **Drives**, right below it — the same way Drives is always shown even with just one drive.
Before you've added anything, the section still appears, just with an empty body: its header, plus a
**＋ Add a connection** row and a one-line hint. Adding your first connection happens right there, in
the Network section itself.

## Adding a connection

1. Click **＋ Add a connection** in the Network section body (shown when nothing's saved yet), or the
   **+** button on the Network section header once you have a connection or two already.
2. Fill in the form: a **name** for the connection, the **protocol** (SFTP, WebDAV, or FTP to start —
   SMB is also selectable, mainly so a **Discovered on your network** row below can pre-fill it for
   you), the **host**, and optionally a **user**, **port**, and a starting **remote path**.
3. Choose how it authenticates — a **password** or a **key file** (with **Browse…** to pick the key).
4. Click **Add**. The connection is saved (no password/passphrase is stored yet — see below) and
   appears as a row under Network.

Editing a saved connection (via its right-click menu) reopens the same form, pre-filled.

## Connecting

Click a saved connection to browse it — the explorer navigates into it exactly like any local folder.
A small **status dot** on the row shows whether it's connected (green), saved but not yet connected
(gray), or hit an error on its last attempt (red, with the reason in the row's tooltip — e.g. a
changed host key).

If the connection needs a password or key passphrase and none is stored yet, you're prompted for it
inline. Check **Remember** to save it in your operating system's keychain so you won't be asked again
next time; leave it unchecked to use it for this session only.

## The row menu

Right-click a saved connection for:

- **Connect** / **Disconnect** — connect, or clear the row's connected status.
- **Edit…** — reopen the add form, pre-filled, to change any field.
- **Forget** — remove the saved connection and its stored password/passphrase (if any) from the
  keychain.

## OS-discovered shares

Below your saved connections, the Network section also lists network drives/shares your operating
system already has mapped or mounted (the same list as Home's **Shared** tab), skipping any that
duplicate a saved connection. Click one to browse it; manage disconnecting or removing it from the
Shared tab.

## Discovered on your network

A third tier — **Discovered on your network** — lists servers and shares found on your local network,
skipping any that duplicate a saved connection or an already-mapped share above it. This tier is
**cross-platform**: it combines two independent scans that run in parallel every time it loads —

- **Windows-native discovery** (Windows only) — the same neighborhood Windows Explorer's **Network**
  folder shows (SMB servers/shares only).
- **mDNS/DNS-SD discovery** (every OS — macOS, Linux, and Windows too) — a local-network broadcast scan
  that finds SMB, SFTP, WebDAV/WebDAVS, FTP, and NFS servers advertising themselves over
  Bonjour/Avahi/mDNS. On macOS and Linux this is the *only* discovery mechanism; on Windows it's a
  **superset** of the native scan — it can surface SFTP/WebDAV/FTP/NFS hosts the SMB-only Windows scan
  never sees. A host found by both scans appears once, not twice.

A discovered row isn't connected to anything yet — click it to open the **＋ Add a connection** form,
pre-filled with that server as the host (and, for a Windows-discovered SMB share, its share as the
path). SFTP/WebDAV/FTP rows found via mDNS pre-fill their actual protocol and port; you only need to
fill in a name and, if the server isn't open, credentials. NFS rows are informational only for now —
there's no NFS client yet, so an NFS row can't be turned into a saved connection.

**Caveats, honestly:**

- Both scans only show what's **already broadcasting** — the Windows scan needs the OS's own **Network
  discovery** setting turned on; the mDNS scan needs the target device to be advertising itself over
  mDNS (most modern NAS boxes do this automatically, but not every server does). A device that isn't
  advertising itself won't appear here either way.
- Each scan is bounded to a few seconds, so a slow or unreachable segment of the network can't hang the
  app — it just means fewer results that round. The two scans run independently: if one fails or finds
  nothing (e.g. mDNS on a network that blocks multicast), the other's results still show.
- On first use, some operating systems' firewalls prompt to allow the app multicast/network access —
  that's a one-time OS-level dialog outside the app's control, not something the app itself asks for.

## Limits

Reconnecting after an app restart may ask for your password/passphrase again even if you didn't
check **Remember** — the app only holds a not-remembered secret for the current session. There is
currently no way to mount a remote connection as a drive letter/volume, or to forcibly close an
open remote session from the sidebar — both are on the roadmap.
