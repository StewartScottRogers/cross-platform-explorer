# Remote & cloud filesystems — design

**Epic CPE-616.** How the explorer browses a **remote** location (SFTP, WebDAV, …) through the *same*
interface it uses for the local disk, with credentials that never touch disk in plaintext.

This is the headless backend architecture. The sidebar UI, the OS-keychain secret storage, the
transfer-manager UI, and wiring these into the app's `list_dir`/preview commands are the attended layer
on top — see "Status" at the end.

## The one seam: `FileSystemProvider`

Every location backend implements one trait (`cpe_server::provider`, CPE-681):

```rust
trait FileSystemProvider {
    fn list(&self, path)  -> Vec<ProviderEntry>;   // one directory level
    fn stat(&self, path)  -> ProviderEntry;
    fn read(&self, path)  -> Vec<u8>;
    fn write(&mut self, path, data);               // create-or-overwrite
    fn mkdir(&mut self, path);
    fn delete(&mut self, path);                    // file or subtree
    fn rename(&mut self, from, to);                // move a file or subtree (CPE-907)
}
```

`ProviderEntry { name, is_dir, size }` is deliberately OS-agnostic (a remote may not have mtime/perms).
Paths are provider-relative, `/`-separated. Errors are human-readable strings, matching the command layer.

Implementations: **`LocalProvider`** (`std::fs`) and an in-memory **`FakeProvider`** (the reference impl +
test double) in `cpe-server`; **`SftpProvider`** in `cpe-sftp`; **`WebdavProvider`** in `cpe-webdav`.

## Locating a backend: the URI model + the router

`cpe_server::location::parse` (CPE-680) classifies a string as **local** (a POSIX path, `C:\…`, or
`\\host\share`) or a **remote scheme** (`sftp`/`ssh`/`smb`/`webdav`/`davs`/`s3`), split into
`{scheme, user, host, port, path}`. Pure, no network.

`cpe-vfs` (CPE-906) is the **scheme router** — the single entry point that turns a saved connection + its
secret into a live provider:

```rust
cpe_vfs::open(&Connection, secret, known_hosts, policy) -> Box<dyn FileSystemProvider>
```

It dispatches `sftp`/`ssh` → `cpe-sftp`, `webdav`/`davs`/`dav` → `cpe-webdav`, else a clear error. The app
does: *load connections → fetch the secret from the OS keychain → `vfs::open(...)` → browse/transfer.*

## The two remote providers

Both follow one pattern: **a provider crate + an in-process fs-backed test server** (no Docker, so the
tests run identically on all three CI OSes — Docker only runs on the Linux runner).

| | `cpe-sftp` (CPE-682/899…) | `cpe-webdav` (CPE-904) |
|---|---|---|
| Protocol | SSH/SFTP | HTTP/WebDAV (PROPFIND/GET/PUT/MKCOL/DELETE/MOVE) |
| Client | `russh` + `russh-sftp` (**async**) | `ureq` (**sync**) |
| Sync-ness | owns a small internal tokio runtime, presents a **sync** provider | naturally sync — no runtime |
| TLS/crypto | `ring` (not aws-lc-rs → no NASM on Windows) | `rustls` + `ring` (same) |
| Auth | password **or** OpenSSH private key (passphrase-decrypted) | HTTP Basic |
| Test server | fs-backed `russh-sftp` server on a loopback port | fs-backed `tiny_http` server |

**Two Windows gotchas baked into the code, learned the hard way:**
- **`ring`, never `aws-lc-rs`.** aws-lc-sys needs NASM to build on the Windows CI leg; `ring` is
  self-contained. Force it via crate features.
- **`tokio::net::TcpListener::from_std` needs `set_nonblocking(true)` first** (SFTP fixture). On Unix it
  *panics* otherwise; on Windows it silently stalls the async I/O pump (the "SFTP init timeout" bug).

## Host-key security (SFTP)

`cpe_server::known_hosts` (CPE-897/898) is the pure security core, decoupled from any ssh crate:
`parse_known_hosts` + `verify_host_key(host, port, key_type, key_b64) -> HostKeyVerdict`:

- **Trusted** — a stored entry for this host+key-type matches → proceed.
- **Changed** — host+type known but the key differs → possible MITM, **refused loudly**.
- **Revoked** — matches a `@revoked` entry → refused.
- **Unknown** — first contact → prompt (TOFU); accepted only under `HostKeyPolicy::Tofu`.

The SFTP provider plugs this into russh's `check_server_key` hook, so a changed/revoked key is refused
*before* any filesystem op — the whole point of SFTP over a bare TCP transport. `default_known_hosts_path`
+ `load_known_hosts` read the user's `~/.ssh/known_hosts`.

## Connections (secret-free profiles)

`cpe_server::connections` (CPE-683) is the persisted list behind the "Connections" sidebar. A profile
records only metadata — `{name, scheme, host, port, user, auth: Password | Key{key_path}, path}` — and
**never a secret**: the password value / key passphrase live in the OS keychain, keyed by the connection,
fetched at connect time. `Connection::location()` round-trips through the URI parser, so a saved
connection navigates like any location. `load`/`save`/`upsert`/`remove` + a per-OS default path.

## Transfer + enumeration (all backends)

`cpe_server::transfer` (CPE-684/905) is provider-agnostic — it uses only trait methods, so **every**
backend gets it:

- `walk(provider, root, cancel, on_entry)` — depth-first recursive enumeration; the `cancel` flag is
  checked before each directory listing and each entry, so a slow/large remote walk stops promptly;
  unreadable dirs are skipped.
- `download_tree` / `upload_tree` — cancellable remote⇄local tree copy, recreating the structure.

## Crate layout

```
cpe-server   (std-only, sync)   FileSystemProvider trait + Local/Fake, location, known_hosts,
                                connections, transfer  — the lean domain core
cpe-sftp     (russh, async)     SftpProvider + in-process test server
cpe-webdav   (ureq,  sync)      WebdavProvider + in-process test server
cpe-vfs                         scheme router: open(connection) -> Box<dyn FileSystemProvider>
```

The async/network/crypto dependency surface is isolated in `cpe-sftp`/`cpe-webdav`; `cpe-server` stays
std-only. All four are standalone (out of the app's cargo workspace) and covered by the 3-OS `Server
crates` CI job.

## Status

**Headless-complete** (all CI-green): the provider trait + op-set (incl. `rename`), the URI model, the
scheme router, both remote providers with password + key/Basic auth, host-key verification, the
connections model, and generic walk + bidirectional transfer.

**Attended / app-facing remainder** (needs the GUI, a real server, or a real keychain):
- the Connections sidebar UI + reading/writing the actual secret in the OS keychain;
- the transfer-manager UI + progress;
- wiring `vfs::open` into the app's scheme-routed `list_dir`/preview/transfer commands;
- an **SMB** or **S3** provider (S3 is impedance-mismatched with both the filesystem op-set — no real
  directories — and the password/key `Connection` model — access-key auth — so it needs those abstractions
  extended first).
