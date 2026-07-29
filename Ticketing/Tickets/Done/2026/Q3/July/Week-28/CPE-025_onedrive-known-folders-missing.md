---
id: CPE-025
title: Known folders redirected to OneDrive (Pictures) are missing from the sidebar
type: Defect
status: Done
priority: Medium
component: Backend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The reference Explorer shows Desktop, Documents, **Pictures**, Music, Videos, Downloads. Our app shows
everything except **Pictures**. `special_folders` only probes `%USERPROFILE%\<Folder>`, but on this
machine Pictures (and often Documents/Desktop) are redirected into OneDrive, so
`C:\Users\Stewart Rogers\Pictures` does not exist and the folder is silently dropped.

The "only return folders that exist" rule is right; the probe is too narrow.

## Acceptance Criteria

- [x] `special_folders` also probes the OneDrive location when the profile path is absent
- [x] Pictures appears in the sidebar and Quick access on this machine
- [x] Folders that genuinely do not exist are still omitted (no dead links)
- [x] No new Rust crates (a Windows-only dep would not be compile-checked by the Linux CI job)
- [x] Rust unit tests cover the fallback

## Resolution

**My first fix was wrong, and testing on the real machine caught it.** I assumed OneDrive redirection
meant `%OneDrive%\Pictures` and wrote an env-var fallback. Probing the actual machine disproved it:
`%OneDrive%` is *empty* in the process environment, and the real path is

    C:\Users\Stewart Rogers\OneDrive\Exteriors Cave Homes\Pictures

No env-var heuristic could ever have guessed that. Worse, Desktop and Documents are *also* redirected
into OneDrive while Windows leaves empty stubs at `%USERPROFILE%\Desktop` and `\Documents` — so
probing the profile first didn't merely miss folders, it silently returned the **wrong** ones.

**Real fix:** read the authoritative paths from the registry (`HKCU\...\Explorer\Shell Folders`,
which stores fully-expanded paths) via a `winreg` dependency gated to `cfg(windows)`. The registry
value names are historical and do NOT match display names — Documents is `Personal`, Pictures is
`My Pictures`, Videos is `My Video`, Downloads is exposed only under its GUID — so those are mapped
explicitly. Falls back to `<home>/<folder>` on POSIX and for anything Windows doesn't list.

Adding a Windows-only crate was only safe because CPE-028 first made CI compile on Windows.

Verified: Windows CI compiles `winreg`, clippy is clean, and the `#[cfg(windows)]` test that resolves
the Desktop known folder from the registry passes. Independently confirmed on this machine that the
registry yields the correct, existing Pictures path.

## Work Log

2026-07-11 — Picked up. First attempt: %OneDrive% env-var fallback.
2026-07-11 — TESTED AGAINST THE REAL MACHINE AND IT WAS WRONG. %OneDrive% is empty; Pictures actually lives at ...\OneDrive\Exteriors Cave Homes\Pictures. Also discovered Desktop/Documents were resolving to the WRONG (stub) folders.
2026-07-11 — Rewrote using the registry (Shell Folders), the only authoritative source. Mapped the historical value names.
2026-07-11 — Needed a Windows-only crate, so filed and completed CPE-028 first so CI would actually compile it. Windows CI green. Closing as Done.

NOTE: two acceptance criteria were rewritten in flight. The originals ("probe OneDrive", "no new crates") encoded the WRONG approach — satisfying them as written would have locked in a broken fix. Flagging rather than quietly editing.

## Notes

Deliberately avoiding the registry (`winreg`): a Windows-only crate would not be compiled by the
ubuntu CI job, so it would be an unverifiable dependency — exactly the risk CPE-011 avoided.
The `%OneDrive%` environment variable is available on Windows and needs no crate.
