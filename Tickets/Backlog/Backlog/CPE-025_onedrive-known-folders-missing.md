---
id: CPE-025
title: Known folders redirected to OneDrive (Pictures) are missing from the sidebar
type: Defect
status: Open
priority: Medium
component: Backend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

The reference Explorer shows Desktop, Documents, **Pictures**, Music, Videos, Downloads. Our app shows
everything except **Pictures**. `special_folders` only probes `%USERPROFILE%\<Folder>`, but on this
machine Pictures (and often Documents/Desktop) are redirected into OneDrive, so
`C:\Users\Stewart Rogers\Pictures` does not exist and the folder is silently dropped.

The "only return folders that exist" rule is right; the probe is too narrow.

## Acceptance Criteria

- [ ] `special_folders` also probes the OneDrive location when the profile path is absent
- [ ] Pictures appears in the sidebar and Quick access on this machine
- [ ] Folders that genuinely do not exist are still omitted (no dead links)
- [ ] No new Rust crates (a Windows-only dep would not be compile-checked by the Linux CI job)
- [ ] Rust unit tests cover the fallback

## Resolution
## Work Log
## Notes

Deliberately avoiding the registry (`winreg`): a Windows-only crate would not be compiled by the
ubuntu CI job, so it would be an unverifiable dependency — exactly the risk CPE-011 avoided.
The `%OneDrive%` environment variable is available on Windows and needs no crate.
