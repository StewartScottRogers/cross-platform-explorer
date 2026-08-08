---
id: CPE-1461
title: "Path traversal / arbitrary local file write in transfer::download_tree from a hostile remote server (SFTP entry names)"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready, security]
epic: CPE-616
created: 2026-08-08
---
## Vector (found in the crates/net+sftp/vfs deep audit, 2026-08-08)
`crates/server/src/transfer.rs:~84-93` `download_tree` maps a REMOTE-server-supplied entry name to a LOCAL
write path without sanitization:
```rust
let rel = entry.path.strip_prefix(&base).unwrap_or(&entry.path).trim_start_matches('/');
let local = local_dir.join(rel);   // untrusted rel joined onto the local download root
std::fs::write(&local, data)        // arbitrary local write
```
The untrusted source is PROVIDER-AGNOSTIC — the shared `transfer.rs` sink is fed by BOTH remote providers, so
fix the sink AND validate at each source:
- **SFTP:** `crates/sftp/src/lib.rs:~288` `name: entry.file_name()` — the SFTP READDIR filename; russh-sftp does
  NOT sanitize it (strips only exact `.`/`..`).
- **WebDAV:** `crates/webdav/src/lib.rs:~226` `let name = norm.rsplit('/').next()...` — the name derived from the
  server's `<d:href>` (after percent_decode + normalize_href); only checked for "not empty", never validated
  against `..`, separators, or absolute/drive prefixes. A single hostile PROPFIND response with
  `<d:href>/C:\Windows\...\evil</d:href>` or `<d:href>/%2e%2e</d:href>` reaches the sink on Windows with NO
  recursion — arbitrary drive-wide write.
`trim_start_matches('/')` strips only leading slashes — neither neutralizes embedded `..`/`\` nor a Windows drive.
(Consolidates the independently-filed CPE-1451 — the webdav view of this same bug.)

## Concrete malicious inputs (server returns these as READDIR entries, marked as regular files)
- **Unix relative escape:** `../../../../../../home/<victim>/.bashrc` → `local_dir.join("../../.../.bashrc")` is
  relative, so `..` climbs OUT of `local_dir` when the OS resolves the write → overwrite `.bashrc` → RCE.
- **Windows drive-absolute (worst — `Path::join` REPLACES the base):** `C:\Users\<victim>\AppData\Roaming\
  Microsoft\Windows\Start Menu\Programs\Startup\evil.bat` → `local_dir.join(r"C:\Users\...")` replaces
  `local_dir` entirely → plants a Startup item → RCE at next login.

## Reachability
LATENT — `cpe-sftp`/`cpe-vfs` are not yet dependencies of `src-tauri` (verified); the remote-provider stack is
built+tested but not wired to the app IPC. Becomes LIVE the moment a "connect to SFTP + download remote folder"
command wires `SftpProvider::download_tree` (its stated purpose, epic CPE-616). Fix NOW, before the connect UI
makes it exploitable.

## Fix direction
Before writing, sanitize each `rel`: build the local path component-by-component from `Path::components()`
keeping ONLY `Component::Normal(_)` — reject/skip any `ParentDir`, `RootDir`, or `Prefix` (drive/UNC) component;
then verify the final `local` still lives under a canonicalized `local_dir` (`canonicalize` the parent and assert
`starts_with(local_dir)`), erroring (skip-on-error, don't fail the whole transfer) on any entry that escapes.
Put this in ONE reusable guarded-join helper and use it in every provider path that writes a server-named entry
locally. Add tests for `..`, absolute `/etc/...`, Windows `C:\...`, UNC `\\host\share`, and a mixed `a/../../b`.

## Effort / blast radius
S–M / one guard helper in transfer.rs + tests. Serialize with CPE-1462 (same file/function). Epic CPE-616.
