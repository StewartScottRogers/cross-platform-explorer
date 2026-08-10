---
id: CPE-1572
title: "Binary Inspector slice 1: structured binary-info DTO + ELF/Mach-O parity (extend binary_preview.rs)"
type: Task
status: Done
priority: Medium
component: Backend
epic: CPE-1562
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1562 (Binary Inspector) slice 1 — the smallest high-value, fully-headless first step of the Binary Studio
program (CPE-1561). Today `crates/server/src/binary_preview.rs::pe_info` returns a text blob for PE only. Promote it
to a **structured DTO** and extend to **ELF + Mach-O parity** via goblin (already a dep). Pure-Rust, in-process, no
sidecar, no new dep.

## Scope (crates/server ONLY — no command/frontend wiring this slice)
- Add a `BinaryInfo` DTO family to `crates/server/src/model.rs` (plain structs + `Serialize` + `specta::Type`, same
  style as `TrashEntry`/`Place`/`EntryInfo`): format (PE/ELF/Mach-O), arch (reuse `bin_arch.rs`), plus
  **Sections / Imports / Exports / Symbols** lists. Keep entries bounded/streamable (large tables) and skip-on-error.
- Extend `binary_preview.rs` to populate that DTO for **PE, ELF, and Mach-O** via goblin (parity across all three),
  replacing/augmenting the current PE-only text summary with the structured form. Preserve the existing text summary
  path if other code depends on it (grep first).
- Unit tests against fixture binaries (or small crafted headers) asserting sections/imports/symbols for each format;
  fuzz-safety: bound all iteration, never panic on malformed input (parser-fuzz discipline — see
  `crates/server/tests/parser_panic_safety.rs`).

## Acceptance criteria
- `BinaryInfo` DTO populated correctly for PE/ELF/Mach-O sample inputs; malformed input degrades gracefully (skip-on-error, no panic).
- `cargo build`, `cargo test -p cpe-server`, `cargo clippy --all-targets -D warnings` green.
- NO new Cargo dependency (goblin already present). NO `#[tauri::command]` and NO frontend in this slice.

## Notes
Command dispatcher + specta bindings + the tabbed frontend provider (Overview/Sections/Imports/Symbols/Disasm/.NET)
are LATER slices of CPE-1562. Disasm (iced-x86) and the hand-rolled ECMA-335 .NET metadata reader are separate
follow-on slices. EXE/DLL decompile is CPE-1563+, out of scope. See Library
`binary-studio-engines-delivery-2026-08-10`. Model: sonnet.
