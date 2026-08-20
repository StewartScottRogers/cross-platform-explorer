---
id: CPE-1820
title: "Security: bump russh off 0.54 to clear two high-severity RUSTSEC advisories on the SFTP path"
type: Bug
priority: High
status: Backlog
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-08-20
closed:
---

## Advisory (found by the dependency-steward audit, 2026-08-20)

`cargo audit` against `src-tauri/Cargo.lock` (996 crates scanned) flags `russh 0.54.5` and
`russh-cryptovec 0.52.0` — pulled in by `crates/sftp/Cargo.toml`'s `russh = { version = "0.54", ... }`
pin (epic CPE-616) — for two **high-severity** advisories, both with a shipped fix:

1. **RUSTSEC-2026-0154** — "Unbounded 32-bit allocation" in `russh`, severity 7.5 (high). Fix:
   upgrade to `>=0.60.3`.
2. **RUSTSEC-2026-0153** — "Unchecked `CryptoVec` allocation and growth handling" in
   `russh-cryptovec`, severity 7.5 (high). Fix: upgrade to `>=0.60.3`.

Both are on the **shipping path**: `crates/sftp` is the SFTP `FileSystemProvider` (CPE-616/CPE-899),
routed live via `crates/vfs` (CPE-1511), and both advisories describe allocation size taken from
attacker-controlled protocol data — i.e. a malicious or compromised SSH/SFTP server the app connects
to can trigger an oversized allocation on the client. Unlike the already-tracked `rsa` Marvin Attack
(CPE-1442, blocked on no upstream fix, and not reachable in our usage), **a fixed release already
exists** (`russh`/`russh-cryptovec` `0.60.3`), so this one is actionable now rather than a
tracking-only ticket.

`crates/server`'s own `Cargo.lock` does **not** carry `russh` at all — `crates/server` has no
dependency on `crates/sftp`, so the crate only enters the tree through `src-tauri` (which depends on
`crates/sftp`). That's expected repo shape (independent lockfiles per CLAUDE.md's "Multiple
independent Cargo.lock files" trap), not a version disagreement: where both lockfiles share a
dependency (`rsa 0.9.10`, `sevenz-rust 0.6.1`) they agree exactly.

## What to do

- [ ] Bump the `russh` pin in `crates/sftp/Cargo.toml` from `"0.54"` to `">=0.60.3"` (or a suitable
      `"0.60"` pin), and bump `russh-sftp` alongside it if the major bump requires a compatible
      `russh-sftp` release — check crates.io for the pairing before landing.
- [ ] Regenerate `src-tauri/Cargo.lock` (the only lockfile that currently carries `russh`) after the
      bump — this is the shipped-app lockfile per CLAUDE.md; a `crates/sftp`-only build won't catch a
      stale `src-tauri/Cargo.lock`.
- [ ] Re-run `cargo audit` from both `crates/server` and `src-tauri` and confirm RUSTSEC-2026-0154 and
      RUSTSEC-2026-0153 both clear.
- [ ] Check `russh`'s 0.54 → 0.60 changelog for any API break in the small internal sync-over-async
      driver `crates/sftp` uses (per its own doc comment: "presents a SYNC provider by driving russh on
      a small internal [runtime]") — this is a two-minor-version jump, not a patch bump.
- [ ] `cargo test` + `cargo clippy --all-targets -D warnings` (both feature modes) on `crates/sftp` and
      `src-tauri` after the bump.
- [ ] Exercise against the real QNAP NAS SFTP target (see project SFTP/E2E notes) if practical, since
      this touches the wire-protocol decode path directly.

## Not in scope here

- `rsa` 0.9.10's Marvin Attack (RUSTSEC-2023-0071) — already tracked separately at **CPE-1442**
  (Blocked, no upstream fix, not reachable in our usage). Do not duplicate.
- `sevenz-rust` 0.6.1's path-traversal advisory (RUSTSEC-2026-0245, severity 8.3) and its
  now-unmaintained warning (RUSTSEC-2026-0246) — no fixed upstream release exists, and the app already
  gates every 7z entry through `entry_name_is_safe` before it reaches `sevenz-rust`'s writer (CPE-628,
  CPE-1746), so the advisory is not reachable through our extraction path today. Worth a future
  tracking ticket in the CPE-1442 shape if it starts mattering (e.g. crate replacement needed), but not
  actionable right now — left out of this ticket to keep it to the one item that has a real fix.
- `ureq` 2 → 3 — already tracked at **CPE-1800** (tagged `big-design`).
- The ~24-29 "unmaintained"/"unsound" warnings (`atk`/`gdk`/`gtk` GTK3 bindings, `encoding`, `paste`,
  `rustybuzz`, `ttf-parser`, `unic-*`, `event-listener`, `glib`) — all transitive, no known
  vulnerability, and several (the GTK3 bindings) come from Tauri's own dependency tree, not something
  this repo can bump directly. Not actioned here.

## Notes

Dependency Steward finding, sprint audit 2026-08-20. `cargo audit` counts at the time of this ticket:
`crates/server` — 2 vulnerabilities (`rsa`, `sevenz-rust`), 5 unmaintained warnings. `src-tauri` — 4
vulnerabilities (`rsa`, `russh`, `russh-cryptovec`, `sevenz-rust`), 24 unmaintained/unsound warnings.
`npm audit --omit=dev` at repo root — 0 vulnerabilities. Related: **CPE-616** (SFTP epic), **CPE-899**
(sftp-provider-russh), **CPE-1442** (rsa tracking, same shape as the "not in scope" items above).
