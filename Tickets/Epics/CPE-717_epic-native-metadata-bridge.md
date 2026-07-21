---
id: CPE-717
title: "EPIC: Native metadata bridge — Finder tags, NTFS streams, xattrs"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

> **Activated 2026-07-21** (dayshift, autonomous — best-guess decisions logged, user delegated PM).
> Chosen for the dayshift because it is backend-first and **fully headless-verifiable**: NTFS ADS on the
> local Windows machine + POSIX xattr, and a pure reconciliation layer, all exercised by CI's 3-OS
> `Server crates` job. Preferred over the higher-priority CPE-703 (instant index), which is `big-design`
> with attended/privilege-gated open questions that can't be verified unattended without rework risk.
> Builds directly on the existing `cpe-server::tags` internal store.

## Goal
Read and write OS-native file metadata — macOS Finder tags & comments, NTFS alternate data streams, Linux
extended attributes — and reconcile them with CPE's internal tag store so labels survive outside the app.

## Why
CPE's tags live in its own store today; a file tagged in CPE looks untagged in Finder/Explorer and vice
versa. Bridging to native metadata makes labels portable and interoperable with the rest of the OS.

## Rough scope (areas, not child tickets)
- Per-OS read/write of native metadata (Finder tags/comments, NTFS ADS, Linux xattrs).
- A reconciliation layer mapping native tags <-> CPE's internal tags (two-way sync, conflict policy).
- Surfacing native comments/attributes in Properties + as columns.
- Opt-in so users who don't want native writes keep the internal-only store.

## Open questions (resolve at activation)
- Sync direction/authority and conflict resolution between native and internal tags.
- Filesystem support gaps (FAT/exFAT lack xattrs/ADS) and graceful degradation.
- Performance of reading native metadata across large listings.

## Definition of Done
- Tags set in CPE appear in the OS's native metadata and vice-versa, per the chosen sync policy.
- Native comments/attributes are visible in Properties and available as columns.
- With the bridge off, tagging behaves exactly as today (internal store only).

## Decisions (activated 2026-07-21 — autonomous best-guess, PM delegated)
- **Sync authority / conflict policy:** the internal store is authoritative on **push** (CPE tags → native
  metadata); native → internal is an explicit, **non-destructive pull** that unions tags (mirroring the
  existing `tag_store_merge` / `import` precedent) and takes a non-empty native label only when internal is
  empty. No silent background two-way sync in v1 — push on tag edit (when the bridge is on), pull on demand.
- **Filesystem gaps (FAT/exFAT, no ADS/xattr):** every native op returns a typed `Unsupported` outcome
  rather than an error that could fail a listing; reads degrade to "no native metadata". Graceful, never
  fatal — same spirit as `list_dir` skip-on-error.
- **Performance / large listings:** native reads are **opt-in and lazy** — only when the bridge is enabled
  and only for the item(s) in view (Properties / a visible column), never on the hot `list_dir` path. The
  fast-when-off rule holds: bridge off ⇒ zero native I/O, tagging is byte-for-byte today's behaviour.
- **Dependencies:** Windows NTFS ADS needs **no new dep** (named streams are plain `path:stream` file I/O);
  POSIX xattr uses the pure-Rust `xattr` crate (libc syscalls, no system libs). The whole bridge lives in
  `cpe-server` (Tauri-free, headless-testable), keeping the app adapter thin.

## Child tickets
1. **CPE-826** — Native metadata I/O core (`cpe-server::native_meta`): read/write/remove a named metadata
   blob per OS — NTFS ADS (Windows) + POSIX xattr (Unix) — with a graceful `Unsupported` outcome on
   filesystems that lack them. Pure + cargo-tested on the native OS (all three via CI). *Headless —
   buildable now.*
2. **CPE-827** — Reconciliation + portable codec: pure two-way mapping native ⇄ internal `TagStore` under
   the push/pull conflict policy, plus CPE's own portable metadata blob (JSON `{tags,label}`) for the
   Windows-ADS / Linux-xattr case. Pure, no new dep, fully cargo-tested. *Headless. (prereq: 826)*
3. **CPE-828** — Wire into commands + surface in Properties / as a column + the opt-in bridge toggle; push
   on tag edit, pull on demand. Frontend + adapter. **GUI-verified — attended.** *(prereq: 826, 827)*
4. **CPE-829** — macOS Finder-tag bplist codec (`_kMDItemUserTags`): `Vec<FinderTag> ⇄ binary plist`,
   round-trip cargo-tested. Split from CPE-827 because byte-compat with real Finder can only be verified on
   a Mac (attended). *(prereq: 826)*

## Work Log
- **2026-07-21** — Activated (dayshift, autonomous). Resolved the three open questions (above) with logged
  best-guesses. Decomposed into CPE-826 (I/O core, headless), CPE-827 (reconcile + Finder plist, headless),
  CPE-828 (UI/commands, attended). Starting CPE-826.
