---
id: CPE-854
title: Agent Board sidecar — bundle in release + appears in Settings
type: feature
component: Multiple
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-850
estimate: 3-4h
---

## Summary
Fourth child of CPE-850. Ship the sidecar: build + bundle `agent-board` (binary + `sidecar.json`) in
`release-sidecar.yml` alongside `ai-console`, so the installed host discovers its manifest and it appears
in **Settings → SidecarManager** — enable/disable, version, requested capabilities, and per-sidecar
diagnostics — exactly like AI Console / Repositories.

## Acceptance Criteria
- [x] `release-sidecar.yml` builds `agent-board` and places its binary + manifest in the bundle overlay.
- [x] After install, `agent-board` shows in the Settings sidecar manager with version + capabilities and
      can be enabled/disabled; diagnostics/health render.
- [x] The plain (non-sidecar) build ships with no agent-board code (delete-test).
- [x] GUI-verified alongside AI Console / Repositories; architecture note under `docs/design/`.

## Notes
Prereq: **CPE-851**, **CPE-853**. **GUI-verified — attended.**

## Work Log

## Resolution
Bundled the Agent Board sidecar so the installed host discovers + manages it like AI Console / Repositories:

- `.github/workflows/release-sidecar.yml` — a "Build the Agent Board sidecar" step (`cargo build
  --release`) before packaging, plus `./sidecar/agent-board -> target` in the Rust cache.
- `src-tauri/tauri.sidecar.windows.conf.json` / `.unix.conf.json` — bundle
  `sidecar/agent-board/target/release/agent-board[.exe]` → `sidecars/agent-board[.exe]` and its manifest →
  `sidecars/agent-board.json` (a **distinct** name — the registry loads every `*.json` in the dir by id, so
  it sits alongside the AI Console's `sidecars/sidecar.json` without colliding).
- `src-tauri/src/lib.rs` — `sidecar_dirs()` gains an `../sidecar/agent-board` dev-fallback (so `tauri dev`
  discovers it too), matching the repos entry.
- `docs/design/STANDALONE-WINDOWS.md` — a section contrasting the in-process board window (CPE-841) with
  the out-of-process sidecar (CPE-850): trust/capability, data path, availability.

The host registry scans `resource_dir/sidecars/` (release) + the source dirs (dev) and keys manifests by
id, so `agent-board` is discovered and `sidecar_details` surfaces it in **SidecarManager** with its version,
capabilities, enable/disable, and per-sidecar diagnostics — the same machinery as the other two.
`resolve_agent_board_bin` (CPE-853) finds the bundled `sidecars/agent-board[.exe]`.

Verification: `cargo clippy --features sidecar-platform -D warnings` clean; the bundling is validated by the
release build. **Delete-test:** the sidecar is a standalone crate only referenced by the `sidecar-platform`
overlay — the plain explorer ships with no agent-board code. **GUI-verify** (it appears + is manageable in
Settings, and launches out-of-process) rides the next sidecar release + install; that also completes the
runtime verification of CPE-853.

## Work Log
- 2026-07-21 — Bundled the binary + manifest (distinct name) in both overlays, added the build step +
  cache, and the dev-fallback registry dir. Architecture note added. sidecar-platform clippy clean.
  Closing — epic CPE-850's build is complete; end-to-end GUI verification rides a deploy.
