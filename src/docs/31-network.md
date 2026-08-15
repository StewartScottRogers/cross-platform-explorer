---
title: Network Connections
order: 31
category: Network & Remote
categoryOrder: 8
---

# Network Connections

The **Network** section in the left-pane sidebar is the entry point for browsing a remote server —
SFTP, WebDAV, FTP, or an S3-compatible object store — right alongside your local drives. It's a **permanent section**, always shown as a
peer of **Drives**, right below it — the same way Drives is always shown even with just one drive.
Before you've added anything, the section still appears, just with an empty body: its header, plus a
**＋ Add a connection** row and a one-line hint. Adding your first connection happens right there, in
the Network section itself.

## Adding a connection

1. Click **＋ Add a connection** in the Network section body (shown when nothing's saved yet), or the
   **+** button on the Network section header once you have a connection or two already.
2. Fill in the form: a **name** for the connection, the **protocol** (SFTP, WebDAV, FTP, or **S3** —
   SMB is also selectable, mainly so a **Discovered on your network** row below can pre-fill it for
   you), the **host**, and optionally a **user**, **port**, and a starting **remote path**.
3. Choose how it authenticates — a **password** or a **key file** (with **Browse…** to pick the key).
   S3 authenticates differently: it offers **Access key** instead, since that's the only credential an
   object store takes — see **S3 and S3-compatible object stores** below.
4. Click **Add**. The connection is saved (no password/passphrase/secret key is stored yet — see below)
   and appears as a row under Network.

The three fields under the protocol change their labels to match what you picked — for S3 they read
**Endpoint**, **Region** and **Bucket and prefix** rather than Host, User and Remote path.

Editing a saved connection (via its right-click menu) reopens the same form, pre-filled.

## Connecting

Click a saved connection to browse it — the explorer navigates into it exactly like any local folder.
A small **status dot** on the row shows whether it's connected (green), saved but not yet connected
(gray), or hit an error on its last attempt (red, with the reason in the row's tooltip — e.g. a
changed host key).

If the connection needs a password, key passphrase, or S3 secret access key and none is stored yet,
you're prompted for it inline. Check **Remember** to save it in your operating system's keychain so you
won't be asked again next time; leave it unchecked to use it for this session only.

## S3 and S3-compatible object stores

Pick **s3** as the protocol to browse a bucket. Because the S3 API is a de-facto standard, the same
connection works against Amazon S3 and everything that speaks it — **MinIO**, **Backblaze B2**,
**Wasabi**, and **Google Cloud Storage**'s S3-compatible endpoint — by pointing it at that provider's
own endpoint.

*Arriving in stages:* saving and editing an S3 connection works now. The object-store client that does
the listing and fetching ships alongside it — if a saved S3 row reports an unsupported protocol when you
click it, your build has the form but not yet the provider.

The form asks for:

- **Endpoint** — the host to talk to: `s3.us-east-1.amazonaws.com` for AWS,
  `s3.us-west-004.backblazeb2.com` for B2, `minio.lan` for a self-hosted MinIO. This is a hostname, not
  a URL — no `https://`, no bucket in it.
- **Port** — leave blank for the usual HTTPS port 443; type it in for something like MinIO's 9000.
- **Region** — the region the store signs requests for. Leave it blank and you get `us-east-1`, which
  every S3-compatible server accepts and most self-hosted ones ignore.
- **Bucket and prefix** — `/my-bucket` to open the whole bucket, or `/my-bucket/reports/2026` to land
  inside a prefix. A bucket is **required**: unlike a file server there's no "root" to browse, so a
  connection has to say which bucket it's for. One connection per bucket.
- **Access key ID** — the public half of the credential pair (`AKIA…`), stored in the connection
  profile exactly like a username.

### Where the secret access key goes

**The secret access key is never typed into the connection form and never written into the saved
profile.** The profile records only a *reference* to it. The first time you connect, you're asked for
the secret in the same inline prompt that asks for a password, and it goes straight into your operating
system's keychain (Windows Credential Manager, macOS Keychain, or the Secret Service on Linux) — the
same place SFTP passwords and key passphrases live. **Forget** removes it along with the profile.

If no secret is stored, the app asks *before* connecting rather than signing a request with a blank
one — a blank secret produces a request that looks valid and comes back `SignatureDoesNotMatch`, which
sends you hunting for a clock or permissions problem that doesn't exist.

### Honest limits of object storage — how browsing *will* behave

An object store is not a filesystem, and this is where that shows. **This list describes the
object-store client that ships alongside the form** (see *Arriving in stages* above) — so if a saved S3
row still reports an unsupported protocol when you click it, your build has the form but not yet the
client, and none of this applies to it yet.

- **There is no rename.** S3 has no atomic rename or move operation. Rather than fake it with a
  copy-then-delete that can leave you with two copies or none if it fails halfway, renaming an object is
  refused outright with an explanation. A copy-then-delete would also be slow in proportion to the
  file's size rather than instant, and would quietly change the object's storage class and metadata
  along the way.
- **Directories aren't real.** A bucket is a flat list of keys; what looks like a folder is just the
  part of a key before a `/`. So "folders" are a naming convention the app reconstructs as it lists, and
  creating one writes a single zero-byte marker object so the empty folder has something to be. A folder
  that nobody created explicitly exists only for as long as something is inside it.
- **Deleting a folder that still has things in it is refused**, not performed. This is the same decision
  as the rename, for the same reason: S3 can only delete one key per request, so emptying a folder means
  a request per object with nothing holding them together. A run that failed halfway would leave part of
  the folder deleted while reporting success — so the app declines and tells you to delete the contents
  first. An **empty** folder really is just one key (its marker), so that one is deleted normally.
  Related: because S3 answers "no content" to a delete whether or not the object was ever there, a
  successful delete means *"this key is gone now"* rather than *"something was removed"*.
- **Deleting an empty folder asks the store twice.** "Only the marker is here" is the one verdict on
  which the app deletes a key that could have a whole folder underneath it, so before it does, it lists
  the folder a second time — this time asking only for keys *after* the marker. A store that has just
  under-reported the folder has to contradict itself outright to get past that, which is a much harder
  lie to tell than the first one. The cost: this second listing uses an **optional** part of the S3 API
  (`start-after`). A store that doesn't implement it will refuse the request, and the app then refuses
  the delete rather than guessing — so on such a store an empty folder can't be deleted, and the message
  says the second listing is what failed. Deleting a **file** is unaffected; only the empty-folder case
  asks twice.
- **Deleting a file works without list permission; deleting a folder needs it.** Because folders aren't
  real, the usual way to tell "one object" from "a folder with things inside it" is to list the prefix
  first — so a delete asks the store to list before it removes anything. A key that grants
  `s3:DeleteObject` but not `s3:ListBucket` (a common setup on self-hosted MinIO and Ceph) can't do that.
  When the listing is refused the app asks a narrower question that needs only read permission: it checks
  whether an object exists at exactly that key. A folder can never answer yes to that — a folder is just
  the front of other keys, with nothing stored at its own name — so a real file is deleted normally and a
  folder is still refused. If neither question can be answered, the delete is refused and the app says so
  plainly: it names the listing request as the thing that failed, tells you nothing was deleted, and
  points at the missing permission. It will not fall back to deleting without looking — that's the one
  path that could report a whole folder removed while everything in it is still there. Reading, writing
  and getting the details of an object you can name work as normal without list permission.
- **Two consequences of that which are genuinely surprising, so they're written down rather than left to
  be met.** A bucket can hold both an object named `photos` **and** other objects under `photos/` —
  nothing in S3 stops it, because keys are just strings.
    - When you can list, such a bucket shows **two rows both named `photos`** — one file, one folder —
      because that is genuinely what's in the bucket. The two rows get **distinguishable paths**: the
      folder row's path ends in a trailing `/` — S3's own spelling for "this is a prefix, not an
      object" — while the file row's stays bare, so the two are never mistaken for the same target and
      clicking one says exactly which of the two you meant. Delete/move/copy/rename aren't wired up to
      a remote location yet — browsing a saved connection is currently read-only in the app — so this
      mainly matters for what that future remote delete/write can build on: the row already carries the
      bit it needs to tell the object and the folder apart, rather than having to guess from a shared
      path.
    - Deleting something that **isn't there** also differs: with list permission it reports success
      (S3 treats delete as idempotent, and "that key is gone now" is true), while without it the app
      refuses, because a 404 on the key can't be told apart from a folder it isn't allowed to look
      inside.
- **Browsing a folder always needs list permission.** Listing is the one operation no per-object
  permission can substitute for, and it's the first thing you hit when you open a bucket. If it's
  refused, the message names the operation, the folder you were opening, the prefix it asked the server
  for, and `s3:ListBucket` as the permission to grant — plus whatever the server itself said.
- **Uploads are a single request**, so an individual file larger than 5 GB isn't uploadable
  (multi-part upload isn't planned for the first version).
- **Keys are taken literally in the middle, but the app currently tidies the ends.** A key is an opaque
  string to S3, so `report.pdf`, `/report.pdf` and `//report.pdf` are genuinely three different objects,
  and a bucket written by a tool that joined paths carelessly can hold all three. Inside a key the app
  preserves that exactly — `a//b.txt` keeps its doubled slash. **At the start and end of a path it does
  not yet:** typing `//report.pdf` currently addresses `report.pdf`, so writing to it would overwrite
  that object rather than create a separate one. Reaching such a key deliberately isn't possible today;
  you can't get there by clicking, only by typing it, and this is tracked as a known gap rather than
  intended behaviour.
- **One shape of key can't be reached yet: a `.` or `..` between the slashes.** A key like
  `photos/../logo.png` is, to S3, an ordinary object with nothing to do with `logo.png` — but the HTTP
  library the app uses rewrites those segments away while building the request, so the app would end up
  asking for a different object than the one it signed for. Rather than silently fetch the wrong object,
  the app refuses such a key and says why. Keys like this are rare and usually accidental; support for
  them needs a different HTTP library and is tracked separately. One rough edge while that's outstanding:
  *listing* such a folder isn't affected by the rewriting, so if you type a path like `/a/../b` you get a
  folder that opens and browses normally — but every file shown inside it will refuse to open, for the
  reason above. You can't reach one by clicking; only by typing it.
- **Access keys only.** Temporary/STS credentials, instance roles, and SSO logins won't be supported —
  the connection needs a long-lived access key ID and secret. *This one applies to the form today:* it
  is why **Access key** is the only authentication S3 offers.
- **A prefix with an enormous number of objects will be capped.** Listing follows the store's
  continuation tokens to completion before showing you anything, so a prefix holding hundreds of
  thousands of objects has a limit rather than an unbounded wait.

## Downloaded names Windows can't hold

A remote name is not always a name your local filesystem can store. `:` is an ordinary, legal byte in an
S3 key — ISO-8601 timestamps like `2026-08-13T10:00:00Z.json` are everywhere — and it is perfectly legal
on Linux and macOS too. On **Windows** it is not: NTFS reads `a:b` as "file `a`, alternate data stream
`b`", so writing that name straight to disk used to leave you a **0-byte file called `a`** with the real
contents tucked away in a stream nothing shows you. The download reported success. That is the worst
possible outcome — a file you can see but cannot read, with nothing prompting you to go looking.

So when downloading to a **Windows** disk, the app rewrites the parts of a name Windows can't hold, and
tells you nothing has been lost by simply giving you the whole file:

| What the remote calls it | What lands on your Windows disk |
|---|---|
| `colon:name.txt` | `colon%3Aname.txt` |
| `report<draft>.txt` | `report%3Cdraft%3E.txt` |
| `notes.` *(trailing dot)* | `notes%2E` |
| `CON`, `NUL`, `COM1`… *(reserved device names)* | `%43ON`, `%4EUL`, `%43OM1`… |

The rewriting is plain percent-encoding: `%` followed by the character's hex code, so you can always read
the original name straight off the new one. It only touches characters Windows genuinely refuses —
`< > : " | ? *`, control characters, a trailing dot or space, and the reserved device names. Ordinary
names are left exactly as they are. Two different remote names can never be rewritten onto the same local
file — that would silently destroy one of them, which is the same bug wearing a different hat. On Linux
and macOS none of this applies and names are written through untouched.

### What happens to a `%` in a name

Almost nothing. A `%` is only ever rewritten when it would otherwise be **ambiguous** — that is, when it
already looks exactly like one of the app's own escapes and would decode back into a different name:

| Remote name | On your Windows disk | Why |
|---|---|---|
| `50% off.txt` | `50% off.txt` | untouched |
| `100%.txt` | `100%.txt` | untouched |
| `report%2ffinal.txt` | `report%2ffinal.txt` | untouched — `%2f` is not an escape this app produces |
| `city=A%2FB` | `city=A%2FB` | untouched — a normal Hive/Athena partition value |
| `literal%3Aname` | `literal%253Aname` | rewritten — `%3A` *is* an escape this app produces, so it must be distinguished from a real `:` |

### Two limits worth knowing about

**A name that is too long is reported, not skipped.** Encoding makes a name longer — up to three
characters where there was one — and no filename may exceed **255 characters**. If encoding pushes a name
past that, the file cannot be written at all. When that happens the transfer **tells you**: everything it
*could* deliver still arrives, and it then reports how many files it could not write and why. It never
claims success for a download that silently left files behind.

**A very long *path* can still be awkward on Windows.** Windows' classic limit is 260 characters for a
whole path. Files past that are written correctly and this app can read them, but older applications
without long-path support may not be able to open them. The transfer prints a notice when it happens.
Downloading into a shorter folder avoids it.

### Two things this rewriting does *not* do

*Uploading does not undo it.* If you download `colon:name.txt` (arriving as `colon%3Aname.txt`) and later
upload that file back, it goes up under the name you can see — `colon%3Aname.txt` — not the original. The
app deliberately does not guess that a `%3A` in one of your local filenames was "meant" to be a colon,
because plenty of local files legitimately contain `%3A` and silently renaming them on upload would be a
worse surprise than the one it fixed.

*It cannot fix case.* Windows filenames are case-insensitive, so two objects that differ only in case —
`Report.txt` and `report.txt` — are still one file once they land. That is the platform, not the app, and
no renaming scheme can work around it.

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

S3 connections carry their own limits — no rename, a refusal to delete a folder that still has contents,
virtual directories, single-request uploads, keys with `.`/`..` segments being unreachable, leading and
trailing slashes on a typed path being tidied away, and access-key-only credentials — all described in
the S3 section above. If a saved S3 row reports an
unsupported protocol when you click it, your build has the form but not yet the provider.

Reconnecting after an app restart may ask for your password/passphrase again even if you didn't
check **Remember** — the app only holds a not-remembered secret for the current session. There is
currently no way to mount a remote connection as a drive letter/volume, or to forcibly close an
open remote session from the sidebar — both are on the roadmap.
