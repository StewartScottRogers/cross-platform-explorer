---
id: CPE-1141
title: "Archive suite: wire the built-but-unwired compress_archive + encrypted-zip functions as commands"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-705
---

## Summary
Epic CPE-705's archive engine is mostly shipped (`compress_to_zip`, `extract_archive`,
`extract_archive_entry`, `read_archive_entries` are already `#[tauri::command]`s). But **three tested engine
functions in `crates/server/src/archive.rs` are wired to no command** — a bounded, no-user-resource enablement
slice (the same shape as the instant-index enablement):
- `compress_archive` (format-dispatching by extension → `.zip` / `.tar.gz` / `.tgz`, archive.rs ≈ 287-296)
- `compress_to_zip_encrypted` (AES-256 password-protected zip create, ≈ 300-323)
- `extract_zip_encrypted` (password extract, ≈ 327-350)

All three already have headless unit tests (`compress_to_targz_then_extract_round_trips`,
`compress_archive_dispatches_by_extension`, `encrypted_zip_round_trips_and_rejects_a_wrong_password`,
archive.rs ≈ 512+). This ticket exposes them as thin Tauri commands so the frontend can offer tar.gz creation
and password-protected archives. **Backend wiring only — the password-prompt UI is separate attended GUI work
(out of scope here).**

## Design
- Add thin `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs` mirroring the existing archive commands
  (async + `spawn_blocking`, one-line into `cpe_server::archive::*`): `compress_archive`,
  `compress_to_zip_encrypted(paths, dest, password)`, `extract_zip_encrypted(archive, dest, password)`. Match
  the exact signatures/return types the existing `compress_to_zip`/`extract_archive` commands use (OpResult-style
  where applicable). Register all three in `generate_handler!` and the specta `collect_commands!` block.
- **Regenerate `src/lib/bindings.gen.ts`** (`cargo run --bin export_bindings --features "specta-bindings
  sidecar-platform"`) and commit — these are new specta-exported commands, so the CI Typed-bindings drift
  guard will fail otherwise (see [[regen-specta-bindings-on-struct-change]]).
- Do NOT build the password-prompt dialog / context-menu entries here — this is the backend enablement only.
  A follow-up (attended) can add the UI. Note that in the work log.
- No new deps (the archive engine + its deps already exist behind the `archive` domain in `cpe-server`).

## Acceptance Criteria
- [ ] `compress_archive`, `compress_to_zip_encrypted`, `extract_zip_encrypted` are `#[tauri::command]`s
      registered in `generate_handler!` + `collect_commands!`, each a thin async+spawn_blocking dispatcher into
      the existing `cpe_server::archive` functions.
- [ ] `bindings.gen.ts` regenerated + committed; `npm run check` green.
- [ ] `crates/server` tests still pass (the engine tests already cover the round-trips + wrong-password
      rejection); `src-tauri` `cargo check --features sidecar-platform` (or the archive feature gate the
      existing archive commands use) green; `cargo clippy` clean.
- [ ] No password-prompt UI added (explicitly deferred); no new dependencies.

## Notes
- Instant-index-twin found by the 2026-07-29 shift-2 research pass: built + tested engine, unwired to commands.
- Epic CPE-705 was reverted to Proposed in the board hygiene pass; working this child re-activates its residual
  (flip the epic to In Progress while this is in flight, or leave the child to close it out — Foreman's call).
