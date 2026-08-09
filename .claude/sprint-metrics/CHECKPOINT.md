# Sprint Checkpoint

## RUN 2026-08-08→09 (CLI) — batch 20/40, agent budget ~118/200 → HAND OFF for a fresh session
**State:** `main` @ origin `af5c2c7e`, working tree clean, main CI green (backend/server/sidecar/frontend all
pass; gui-smoke is the known-flaky leg at 37/39). `Z:\repos\` cleaned of all `cpe-*` worktree strays. Lock held
(`.claude/sprint-metrics/SPRINT-LOCK`). **This is a clean budget hand-off, NOT a stop** — resume with a fresh
session for a full agent budget.

> **⚠ Concurrent process:** the shared checkout is currently on branch `chore/sprint-lights-out-factory`
> (created by ANOTHER process — CLI or desktop Cowork — which hardened `sprint.md`/`continue-sprints-without-the-user`
> memory with "lights-out factory" language). It points at the same commit as origin/main. Do NOT delete/clobber
> that branch. Commit to main via `git push origin HEAD:main` or switch to `main` only if the tree is clean.
> Coordinate ([[concurrent-nightshift-coordination]]).

## Shipped this run (20 batches, 0 escaped defects; every merge gauntlet-verified: independent Reviewer + UAT)
1. **`workshift` → `Sprint` rename** (commit `1f31f045`, 266 files) — skills `/sprint` + `/sprint-batched`,
   `.claude/sprint-metrics/`, `SPRINT.md`, memories. The pre-existing SPR-NN "Sprints" (`/ticketing-sprint`) kept
   + disambiguated per user ("keep both"). No ticket (user directive).
2. **Network "mount anything" program — SFTP + WebDAV + FTP now LIVE + secured** (user activated it):
   - CPE-1510 connection-secret **keychain** store (`crates/server/src/secret_store.rs`, reuses
     `vault_manager::SecretAccess`/`KeyringBackend`, no new dep; real Windows Credential Manager round-trip UAT).
   - CPE-1511 **vfs-route crux** (`crates/vfs/src/connect.rs` `connected_provider` + `ProviderPool`; `fs_route`
     routes remote URIs; **LOCAL path proven byte-for-byte unchanged** — opus adversarial review + UAT). SFTP +
     WebDAV browse via `list_dir`.
   - CPE-1513 **Network sidebar UI** (`src/lib/network.ts` +27 tests, Sidebar "network" section + Explore
     "Network…" row, add-connection form, per-connection menu, secret prompt, `connections_*` commands). **CODE on
     main + reviewed (nit fixed: `--success` token). ⚠ VISUAL SIGN-OFF OWED by the user** (5 surfaces — see
     `Ticketing/Tickets/Deferred/CPE-1513_*`). Landed via a worktree-leak+`git add -A` onto main (PR #731 closed
     superseded; verified byte-identical) — hence the "never `git add -A` while worktrees live" rule.
   - CPE-1514 **cpe-ftp provider** (FTP/FTPS via suppaftp, `ftp`/`ftps` scheme; hostile-server UAT: traversal
     dropped, 32MiB RETR + 100k LIST bounded). First net-new protocol.
   - CPE-1512 **SFTP host-key TOFU persistence** (`known_hosts.rs` app-managed store at `%APPDATA%`/XDG; never
     mutates `~/.ssh`; Changed-key can't auto-trust; user pins win on merge). Completes TOFU.
3. **Earlier backend run (batches 1–15):** CSP CPE-1477; audio-waveform CPE-1478; **gui-smoke restoration**
   CPE-1479/1481/1507 (0→37 passing, mouse harness CDP→W3C-Actions fallback); thumb_video SSRF CPE-1480;
   binary-arch detection CPE-1485; image-diff engine CPE-1490; file split/join CPE-1491; CPE-1414 closed superseded.

## Queued / next (all Proposed unless noted)
- **Network program (continue):** CPE-1500 (OS-mount "Mount as drive"), CPE-1503 (S3 → unlocks B2/GCS free),
  CPE-1506 (cloud OAuth Drive/OneDrive/Dropbox), CPE-1501 (provider capability/auth-model ext).
- **GUIs on shipped backends (need visual/attended verify):** CPE-1508 image-compare pane, CPE-1509 split/join dialog.
- **gui-smoke tail:** CPE-1483 (Linux drive-tile), CPE-1507 remaining (samples + saved-search).
- **superfile/competitive/theme epics:** CPE-1484–1496 (hotkeys, vim-nav, dense-view, Drop Stack, theme engine 5-epic).
- Follow-up filed: CPE-1512 done; no open follow-ups from it.

## OWED TO THE USER (the only non-headless blocker)
**Attended VISUAL verification of the Network sidebar (CPE-1513)** — a build→install→run so the user can add an
sftp/webdav/ftp connection and browse it, and eyeball the sidebar UI (5 surfaces). Offer it on their return.

## Tuned defaults / lessons (seed the next session)
- **Worktrees live INSIDE the project** at `.claude/worktrees/cpe-<id>-wt` (gitignored) — NEVER `Z:\repos\`
  (parent) or Temp ([[keep-all-fs-work-inside-project]], user 2026-08-09). Dispatch prompts must say so.
- **NEVER `git add -A`** on the shared main checkout while worktree workers are live — explicit paths only (a
  leak swept a worker's files onto main once). Verify `HEAD == origin/main` + no stranded commits after merges.
- Workers must run `cargo`/`git`/`gh` **synchronously inline** — several appeared "stalled" but were just doing
  long real builds ([[subagents-run-work-synchronously]]).
- **Opus adversarial Reviewer** gates high-blast-radius / traversal / untrusted-parser diffs (it caught a real
  SSRF, a manifest overflow-panic, and confirmed the local-path safety) — sonnet reviewers missed the SSRF.
- New protocol providers **mirror `cpe-sftp`/`cpe-webdav`** exactly (sync crate + FileSystemProvider + is_safe_name
  + bounded reads + scheme arm in `cpe_vfs::open`). Regenerate + commit **`src-tauri/Cargo.lock`** on any dep
  (use `cargo check`, NOT `cargo generate-lockfile` which churns ~990 packages).
- **Lights-out:** never AskUserQuestion mid-sprint; decide-and-log; user resource/authority needs → skip-and-queue,
  never asked-and-awaited ([[continue-sprints-without-the-user]]).

## To resume
Fresh session → "start the sprint" (or "start a batched sprint"). Read this file + `history.md` tail. Continue
the Network program (CPE-1500 OS-mount or CPE-1503 S3 are the natural next headless builds) OR whatever the user
directs. The Network sidebar visual verify is the one thing to surface to the user for their hands-on check.
