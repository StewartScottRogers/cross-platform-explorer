---
id: CPE-1049
title: "Self-asserting native-metadata OS-interop test (retire manual Get-Item -Stream / getfattr check)"
type: test
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-717
estimate: 2-3h
burndown: "MANUAL-TEST-BURNDOWN row #8 (native OS metadata interop)"
---

## Summary
Retire manual-test burndown **#8**: today, native-metadata interop (does the OS *itself* read the bytes CPE
writes as an NTFS ADS on Windows / a POSIX xattr on Linux/macOS?) is verified by a human running the
`native_tags_demo` example and then eyeballing `Get-Item -Stream` / `getfattr`. Make it a **self-asserting
automated test** that runs in the existing 3-OS CI matrix (`Backend (ubuntu/windows/macos)` job in
`ci.yml`, which runs `cargo test`), so a machine confirms it on every push — no human, no user resource.

## Design (buildable)
A `#[cfg]`-gated integration test in `crates/server/tests/` that, on a temp file:
1. Writes a value via **`cpe_server::native_meta::write(path, name, value)`**.
2. **Re-reads it through the OS's OWN mechanism — NOT `native_meta::read`** (that would be circular; the
   point is to prove a *standard, any-tool-readable* attribute was written):
   - **Windows**: read the alternate data stream via the OS path syntax — `std::fs::read(format!("{path}:{stream}"))`
     where `{stream}` is `native_meta::cpe_name(name)` (Windows `CreateFile` accepts `file:stream`); this is
     a plain filesystem read, independent of `native_meta`'s DeviceIoControl/backup-API path.
   - **Linux/macOS**: read the xattr via a mechanism independent of `native_meta::read` — prefer the OS tool
     (`getfattr -n <name> --only-values <file>` on Linux) if available, else a direct `libc::getxattr` /
     `getxattr` call (no new deps — `libc` is already a transitive dep, or use the tool via `Command`).
     The attribute name is the standard `user.`-namespaced `cpe_name(name)`.
3. Assert the OS-read bytes equal what was written. Also assert graceful behavior on a filesystem that
   doesn't support it (skip/pass, mirroring `MetaError::Unsupported` — don't fail CI on a tmpfs that lacks
   xattr).
4. If a platform genuinely can't be self-asserted independently in CI, say so in the test comments + keep
   the manual note for that platform — don't fake it.

## Acceptance Criteria
- [x] A cargo test writes via `native_meta` and reads back via the OS-native path (Windows ADS `path:stream`
      / Linux+macOS getfattr/getxattr), asserting equality — passing on the 3-OS CI matrix.
- [x] Unsupported-filesystem case degrades (skips/passes), never a false CI failure.
- [x] No new deps; `cargo test -p cpe-server` + clippy clean both modes on all three OSes.
- [x] Burndown #8 flips to ✅ once the test is green in the 3-OS matrix, naming that job as the pin.

## Work Log
2026-07-25 (sprint) — Filed by the QA Architect as the next clean headless MVD win (GUI surfaces #3/#4
are blocked on a self-hosted runner; this needs no user resource). Builds on the CPE-717 native_meta
modules + the existing `native_tags_demo` example.

2026-07-25 (sprint, Foreman) — Prior session had built the test (`crates/server/tests/native_meta_os_interop.rs`,
189 lines) into PR #367, CI-green on the whole 3-OS backend/server matrix but never reviewed. Ran the
gauntlet: **Reviewer APPROVE** (confirmed Windows `path:stream` read genuinely bypasses `native_meta::read`,
namespacing correct, degrade branches tight, not hollow) + **UAT PASS** (ran the test locally on Windows:
`test result: ok. 1 passed`, assertion genuine). Reviewer flagged one non-blocking gap — the `Server crates`
Linux leg didn't install `attr`, so `getfattr` could be absent and the Linux branch would silently skip.
Folded in the fix (added `apt-get install -y attr` to the ubuntu leg of the Server-crates CI job) so the
Linux OS-interop assertion always runs. Re-ran CI: all Backend + Server-crates legs green on ubuntu/windows/
macos. **Merged (squash) to main.** Burndown #8 → ✅ automated (pinned by the `Backend` + `Server crates`
3-OS `cargo test` jobs). MVD 8 → 7.
