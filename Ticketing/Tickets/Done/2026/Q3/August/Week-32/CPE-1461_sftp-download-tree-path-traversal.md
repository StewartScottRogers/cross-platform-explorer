---
id: CPE-1461
title: "Path traversal / arbitrary local file write in transfer::download_tree from a hostile remote server (SFTP entry names)"
type: Bug
status: Done
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

## Work Log (2026-08-08, Done — PR with CPE-1462 on branch `cpe-1461-1462-remote-traversal-dos`)
Fixed at BOTH the shared sink and each source, defense-in-depth.

**Sink — `crates/server/src/transfer.rs`.** Added `pub fn guarded_join(base, rel) -> Option<PathBuf>`: it
rebuilds the path segment-by-segment, splitting on BOTH `/` and `\` (so a Windows-style separator is
neutralized on every OS, incl. Linux CI where `\` is otherwise a legal filename byte), keeping ONLY
`Component::Normal` segments; any `..` → `None`, any segment that parses as a root/drive/UNC prefix →
`None`. Because only `Normal` segments are ever pushed onto `base`, the result is always lexically
contained — proven by the `guarded_join_never_escapes_the_base` test over the whole malicious-input list.
`download_tree` now uses it for every write AND every mkdir; an entry that would escape is SKIPPED with a
surfaced `eprintln!` notice (skip-on-error — one hostile entry never fails the whole transfer) and its
parent dirs are NOT created. Belt-and-suspenders: `local_dir` is canonicalized once up front and each
materialized dir is verified to canonicalize back under it (guards a pre-existing symlink inside the root).

**How each malicious input is neutralized (sink):** `../../../../../home/x/.bashrc` → `..` segment →
skipped. `C:\Users\…\Startup\evil.bat` → on Windows the `C:` segment is a `Prefix` (non-`Normal`) →
skipped; on Unix it's contained as a literal `C:` dir INSIDE the root (still no escape). `\\host\share\x`
(UNC) and `\x` (rooted) → split on `\` → contained under the root as normal segments (no escape). `x\..\..\y`
→ split on `\` yields `..` → skipped on every OS. `a/../../b` → `..` → skipped. `%2e%2e` reaches the sink
only as a literal leaf name → written safely inside the root (the decode-to-`..` happens at the WebDAV
source, below).

**Source (defense-in-depth).** Added `pub fn is_safe_name(name)` in `transfer.rs` (rejects empty/`.`/`..`,
any `/`/`\`/NUL, and any non-single-`Normal`-component like a bare drive). Wired it into
`crates/sftp/src/lib.rs` `list` (filters the READDIR filename) and `crates/webdav/src/lib.rs`
`parse_multistatus` (skips the whole entry when the derived name is unsafe — this is where `%2e%2e` →
`..` and `…/C:\…` are dropped, verified by `parse_multistatus_skips_path_traversal_hrefs`).

**Redirect policy (watch-item).** Small change, so done: `WebdavProvider::connect` now builds the agent
with `ureq::AgentBuilder::new().redirects(0).build()` instead of the default `ureq::agent()` (which
follows up to 5 redirects). A `3xx` toward `file://`/an attacker host is no longer auto-followed — it
surfaces as an error. Closes the future-SSRF foothold flagged in the audit.

**Tests (all green, `cargo test`):** server transfer module +6 (`guarded_join_never_escapes_the_base`,
`is_safe_name_accepts_leaves_and_rejects_paths`, `download_tree_neutralizes_every_traversal_input` via a
`HostileNames` provider asserting a sentinel file is never created outside the root,
`download_tree_still_downloads_a_legit_nested_tree` = no over-rejection, plus the CPE-1462 cap tests);
webdav +1 traversal-href test (12 total pass, incl. the existing download round-trip = no over-rejection);
sftp 14 pass (incl. download round-trip). Build + `clippy --all-targets -D warnings` clean for all three
crates. Public API unchanged (`walk`/`download_tree`/`WalkEntry` signatures identical).

## Work Log — attempt 2 (2026-08-08, post-review; PR #717 got SEC PASS, this addresses 4 flagged defects)
The adversarial auditor confirmed the HIGH remote-traversal→arbitrary-write vector is genuinely closed
(SEC PASS). The reviewer + auditor flagged 4 issues in the belt-and-suspenders symlink guard; all fixed on
the same branch, primary fix untouched:

1. **Guard validated AFTER mutating (must-fix).** The old code did `create_dir_all(dir_to_make)` and THEN
   canonicalize-checked, so a pre-existing symlinked dir inside `local_dir` (planted out-of-band) would be
   *followed* by the mkdir before the check ran — an inert empty dir could escape (no attacker content,
   but the docstring lied). Fixed by validating BEFORE any mutation: new `fn existing_ancestor(path)`
   (walks up via `symlink_metadata`, so a symlink is found without being followed) finds the longest
   already-existing ancestor of the target; it is canonicalized and asserted `starts_with(canonical_root)`
   *before* `create_dir_all` runs. The portion then created is brand-new (no symlinks) under a
   verified-contained real dir. Fail-closed (skip + notice) on a dangling/unresolvable ancestor. Uses the
   reviewer's insight that `walk` emits+creates a dir entry before any of its children, so the parent is
   always already present.
2. **Leaf symlink followed on write (fold-in).** The parent was checked but `std::fs::write(local, …)`
   would still follow a pre-existing *symlinked leaf file* out of the root. Fixed: before writing, if
   `symlink_metadata(local).file_type().is_symlink()`, skip the entry (fail-closed, surfaced notice).
3. **`is_safe_name` too permissive (belt-and-suspenders).** Now also rejects any leaf containing `:`
   (Windows NTFS ADS / drive selector — `x:y`, `..::$DATA`, `file:$DATA`) or beginning with `..`
   (`..stream`, `..:$DATA`). `:` is meaningless on Unix and dangerous on Windows, so fail closed. (Reserved
   device names CON/NUL/… left as lower-priority — the verbatim-`\\?\` canonicalize at the sink already
   contains them; noted here as a deliberate skip.)
4. **No dedicated SFTP filter test (reviewer minor).** Added `list_filters_out_a_path_traversal_readdir_name`
   (`#[cfg(unix)]`): seeds a backslash-bearing hostile filename on the server root and asserts `sftp::list`
   drops it while the legit `readme.txt`/`sub` survive.

New tests: `download_tree_does_not_create_a_child_through_a_preexisting_symlinked_dir` (defect 1) and
`download_tree_does_not_follow_a_preexisting_symlinked_leaf_on_write` (defect 2), both `#[cfg(unix)]`
(creating a symlink on Windows needs admin/dev-mode; the ordering fix itself is cross-platform — the
Windows dev box can't run these, the 3-OS CI matrix's Linux/macOS legs do); `is_safe_name` bad-list
extended with `x:y`/`..:stream`/`..::$DATA`/`..evil`/`C:` (defect 3); the sftp filter test (defect 4).

Re-verified SYNCHRONOUSLY on Windows: `cargo build` + `cargo clippy --all-targets -D warnings` (cpe-server
default AND `--features index`; webdav + sftp — neither has an `index` feature) all clean; `cargo test` —
server transfer 10 pass (2 unix-only symlink tests compiled out here, run on CI), webdav 12, sftp 14. All
prior tests (sentinel-never-escapes battery, legit-tree download, walk caps) still green. Primary fix
(`guarded_join` / source `is_safe_name` / walk caps / `redirects(0)`) unchanged.
