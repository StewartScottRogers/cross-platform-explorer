---
id: CPE-1820
title: "Security: bump russh off 0.54 to clear two high-severity RUSTSEC advisories on the SFTP path"
type: Bug
priority: High
status: Blocked
component: Backend
tags: resource-blocked-upstream
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

## Work Log

### 2026-08-20 — Sprint worker: attempted the bump, hit a hard cross-crate conflict, STOPPED (no PR)

Branched `cpe-1820-russh-bump` off `main` and bumped `crates/sftp/Cargo.toml`'s `russh` pin from `"0.54"`
first to `"0.60.3"` (the advisory's exact fix floor), then to `"0.62.7"` (latest stable, in case a newer
minor sidestepped the problem below). `russh-sftp` did not need a companion bump — it talks to `russh`
only through a generic `AsyncRead`/`AsyncWrite` channel stream, no direct `russh` dependency, confirmed via
`cargo info russh-sftp`.

**Both attempts fail to compile** (`cargo build --all-targets` in `crates/sftp`) with 18 trait errors in
`ml-kem` (`E0437`/`E0407`/`E0107`/`E0220`/`E0053`/`E0277`/`E0046` — "type `Error` is not a member of trait
`kem::Decapsulate`", "trait takes 0 generic arguments but 2 were supplied", etc.). Root cause, traced via
`cargo update -p ...` and reading the resolved crates' manifests directly out of the registry cache:

- `russh >=0.60.3` (any version tested: `0.60.3`, `0.62.7`) added mandatory post-quantum hybrid key
  exchange support, pulling in `ml-kem` as a **non-optional, non-feature-gated** dependency — it is not
  behind `default-features = false` or any of russh's four features (`aws-lc-rs`, `des`, `dsa`, `rsa`),
  so there is no way to opt out of it from our `Cargo.toml`.
- `crates/server`'s `age = "=0.12.1"` (CPE-1247, the encrypted-vaults passphrase crypto core, pinned exact
  on purpose) *also* mandatorily depends on `ml-kem = "0.2"` — again not feature-gated in `age` (its
  `default = []` feature set has nothing to do with it) — which resolves to `ml-kem 0.2.1`.
- Both `ml-kem` lines bottom out in the `kem` crate, and **`kem 0.3.0` (the current stable release) ships
  a breaking trait change relative to the `0.3.0-pre.0`/`-rc.x` line `ml-kem 0.2.1`'s published source was
  actually written against.** `ml-kem 0.2.1`'s manifest declares a *loose* `kem = "0.3.0-pre.0"` bound
  (no `=`), so Cargo happily unifies it onto the newer `kem 0.3.0` that `russh`'s `ml-kem 0.3.x` also
  needs — and then the real compiled source doesn't match that API. (`ml-kem 0.2.3`, the only other
  0.2.x release, pins `kem` with an exact `=0.3.0-pre.0` instead, which turns the same conflict into a
  **resolution failure** — "failed to select a version for `kem`" — rather than a compile error, so it's
  not an escape route either.) `crates.io` currently has no `ml-kem` release between 0.2.3 and 0.3.0 that
  fixes this, and `age` has no release newer than `0.12.1` (checked via the crates.io API) whose `ml-kem`
  bound moved to the 0.3.x line — so there is no dependency-bump-only fix available today for either side.

**This is not an API-porting problem** — no amount of faithfully porting `crates/sftp`'s russh call sites
(client handler, `connect`, auth, channel/subsystem open, the `russh::server` test rig) matters, because
the crate never gets far enough to compile at all. It's a genuine, currently-unresolved incompatibility
between two *other* crates (`ml-kem`/`kem`) that both `russh >=0.60.3` and `age 0.12.1` pull in for
unrelated reasons (PQ key exchange vs. PQ recipients), and fixing it would require bumping `age` too
(itself already at its latest release, so even that isn't purely a version bump — it'd mean waiting on
`age` or `ml-kem` upstream, or a `[patch.crates-io]` override), which is out of this ticket's scope ("bump
`russh`/`russh-cryptovec`... No other dependency bumps").

**Stopping per rule 10 — no PR opened, no gates run** (the crate doesn't compile, so `cargo test`/`clippy`/
`cargo audit` before/after counts would be meaningless). All local `crates/sftp/Cargo.toml` and
`Cargo.lock` changes were reverted; the working tree is clean. Branch `cpe-1820-russh-bump` exists locally
in the worker's worktree but was not pushed (nothing to review — it's a no-op besides this Work Log entry).

**Recommendation:** re-queue this behind an `age` upstream fix (or file a tracking ticket in the CPE-1442
shape — "blocked, no upstream fix" — for the `ml-kem`/`kem` ecosystem incompatibility) rather than
retrying as-is; the underlying crates.io state, not our code, is what's blocking it. Worth periodically
re-checking `age`'s and `ml-kem`'s crates.io version lists for a release that moves `age` onto `ml-kem
0.3.x` (or `ml-kem 0.2.x` onto a `kem` release compatible with `0.3.0` stable).

## Blocked on (Foreman disposition, 2026-08-20)

**Blocked-on:** an upstream release, not our code. `russh >= 0.60.3` mandatorily depends on
`ml-kem 0.3.x`; `crates/server`'s deliberately exact-pinned `age = "=0.12.1"` (CPE-1247) mandatorily
depends on `ml-kem 0.2.x`. Both bottom out in `kem`, whose 0.3.0 stable broke the trait shape
`ml-kem 0.2.1` was written against — so the resolver's unification produces 18 trait errors in a
transitive crate neither of our crates touches directly. `ml-kem 0.2.3` pins `kem` exactly instead,
which converts the compile error into a resolution failure, so it is not an escape route. `age` has
no release newer than 0.12.1, and there is no `ml-kem` patch between 0.2.3 and 0.3.0 that reconciles
them.

**Unblocks-when:** `age` ships a release that moves onto `ml-kem 0.3.x`, or `ml-kem` ships a 0.2.x
patch compatible with `kem 0.3.0`. Re-check both crates.io version lists periodically.

**Not an API-porting problem.** The `crates/sftp` call sites were never reached — the build fails
before them. `russh-sftp` needs no companion bump (it couples only through a generic
`AsyncRead`/`AsyncWrite` stream).

**Exposure while blocked:** RUSTSEC-2026-0154 and RUSTSEC-2026-0153 remain live on the SFTP
wire-protocol decode path, reachable from an untrusted remote server. Same disposition shape as
CPE-1442 (`rsa` Marvin Attack, no upstream fix).
