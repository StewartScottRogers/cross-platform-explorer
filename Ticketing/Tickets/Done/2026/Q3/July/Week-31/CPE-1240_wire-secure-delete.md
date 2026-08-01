---
id: CPE-1240
title: "Wire secure delete: shred command + Securely-delete action + honest confirm dialog"
type: Task
priority: Medium
component: Multiple
tags: [ready]
estimate: 2h
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
`secure_shred::shred_file(path, scheme) -> ShredReport` (schemes Zero/Random/DoD3/Gutmann, CPE-1012) +
`secure_delete::plan_shred`/`passes` (honest SSD/CoW caveats, CPE-941) are built + cargo-tested but
ORPHANED — no command, no UI (`grep shred src-tauri/src/lib.rs` → only a comment). Wire them so a user
can securely delete (shred) a file from the explorer, with HONEST messaging.

## GREP FIRST
- `crates/server/src/secure_shred.rs` (`shred_file`, `ShredReport`) + `secure_delete.rs`
  (`ShredScheme`, `plan_shred`, the caveat text) — reuse, don't reimplement.
- `src-tauri/src/lib.rs` — the existing `delete`/`delete_permanent` commands + how the context menu's
  delete/trash actions dispatch (mirror the shape; async + spawn_blocking per CPE-760/761 since shred
  does heavy disk I/O).
- The context menu component + how existing destructive actions (Delete) render + confirm
  (`ConfirmDialog`), so the new action matches conventions.

## Build
- **Backend**: a thin `shred_paths(paths, scheme)` command (async, `spawn_blocking`), dispatching to
  `shred_file` per path, returning per-path `ShredReport`/error (OpResult-style, skip-and-report — one
  failure doesn't abort the batch). Register in `generate_handler!` + `collect_commands!`. Regen
  `bindings.gen.ts` (ShredReport/ShredScheme cross the boundary → specta).
- **Frontend**: a "Securely delete…" (Shred) context-menu action with a leading icon; menu text uses
  `var(--text)` (NEVER red — MENUS.md), colour from theme vars.
- **Confirm dialog** (visible border `var(--dialog-border)`): must clearly state (a) this is PERMANENT
  and NON-RECOVERABLE — it does NOT go to the recoverable Bin/Trash (that's the whole point), and
  (b) the HONEST platform caveat from `plan_shred` (on SSDs / copy-on-write / journaling filesystems,
  overwriting can't guarantee the old data is gone — this is best-effort). No false guarantees. An
  explicit confirm (type-to-confirm or a clearly-labelled destructive button) is the safeguard since
  there's no trash fallback.

## Acceptance criteria
- A user can pick "Securely delete…" on a file, read an honest confirm, and shred it; the file is
  overwritten + removed; a per-path report/summary is surfaced; failures are reported per-path.
- Reuses the built engine + caveat text — no reimplementation, no false guarantees.
- `cargo test -p cpe-server` + `cd src-tauri && cargo test` + clippy both modes; `npm run check` +
  `npm test`. REAL tests: the command shreds a temp file (cargo, reuse the engine's test style) + a
  vitest for the dialog's honest-caveat + permanent-warning copy + the action wiring. Non-hollow.
- gui-smoke render pin optional (Foreman may capture the confirm dialog for the Visual Critic).

## Notes
Scope = secure-delete only. Encrypted vaults are DEFERRED (user-gated: crypto-dep exception + security
review). Do NOT route shred through the trash. "os error 225" on cargo test = Defender quarantine.
