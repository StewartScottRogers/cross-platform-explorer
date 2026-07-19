---
id: CPE-760
title: Async the filesystem commands so a slow drive can't freeze the whole app
type: bug
component: Backend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 2-3h
---

## Summary
Root cause of the "6 s to show 20 files" (found via the CPE-758/759 Diagnostics readout the user pasted
back): **~25 unrelated backend commands all take ~5.2 s at the same instant** — `home_dir`, `list_drives`,
`special_folders`, `disk_space`, `forge_repo_status`, `list_dir_stream`, `write_settings`, … — and the same
commands are 2–10 ms otherwise. Trivial calls can't be 5 s on their own.

**Diagnosis:** the Tauri commands are **synchronous**, so they run on the main thread and block each other.
One genuinely-slow op — git status / listing / disk on the user's **slow/network `Z:` drive** — freezes
the main thread for ~5 s and everything queues behind it. The listing itself is 6 ms; it was just stuck.

## Fix
Make the filesystem/OS commands **`async` + `tauri::async_runtime::spawn_blocking`** so a slow drive op runs
on a blocking thread and never freezes the main thread or the other commands. Pattern: keep the body as a
sync `_impl` (internal/test callers use it), add a thin async command wrapper.

Converted (per-navigation path, this pass):
- `list_dir_stream` (the folder listing), `list_dir`, `disk_space`, `forge_repo_status` (git status).

## Acceptance
- [ ] Navigating folders no longer freezes: the file list paints fast even while git status / disk run.
- [ ] `cargo` builds + tests pass in both feature modes; clippy clean.
- [ ] Diagnostics no longer shows a batch of unrelated commands all at ~5 s.

## Notes / follow-up
Startup commands (`list_drives`/`special_folders`/`home_dir`/`load_tags`/`sidecar_registry_ids`) and the
treemap (`dir_size`/`dir_children_sizes`) are the same class and should get the same treatment if the
startup freeze remains — follow-up after confirming the per-nav fix. First real async commands in the
codebase (`spawn_blocking` pattern).
