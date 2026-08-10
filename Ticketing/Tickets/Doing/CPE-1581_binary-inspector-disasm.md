---
id: CPE-1581
title: "Binary Inspector slice 2: x86/x64 disassembly via iced-x86 (headless)"
type: Task
status: Doing
priority: Medium
component: Backend
epic: CPE-1562
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1562 (Binary Inspector) slice 2, building on CPE-1572 (`BinaryInfo` DTO in `crates/server`). Add a real
disassembly of an executable's code section. The Binary Studio spike (Library `binary-studio-engines-delivery-2026-08-10`)
pre-vetted **`iced-x86`** (MIT, pure-Rust, ~250KB, in-process) for x86/x64 — the sanctioned choice.

## Scope (crates/server only — no command/frontend/bindings this slice)
- Add `iced-x86` to `crates/server/Cargo.toml` (pin a current version; regenerate + commit BOTH
  `crates/server`'s and `src-tauri`'s `Cargo.lock` per [[multiple-independent-cargo-locks]]).
- Add a disassembly DTO/function to `binary_preview.rs`: for a PE/ELF/Mach-O with an x86/x64 code section, decode
  instructions into a bounded, streamable list (address, bytes, mnemonic+operands text). Cap the instruction count
  (documented const, like `MAX_BINARY_LIST_ENTRIES`) so a huge/hostile binary can't blow up memory/time.
- Only x86/x64 this slice (use `bin_arch` to detect; other arches → empty/omitted, not an error). Skip-on-error;
  never panic on malformed/truncated code (add to the parser panic-safety battery).

## Acceptance criteria
- Disassembles a known x86/x64 fixture's code section into correct mnemonics (substantive assertion, not hollow);
  non-x86 input yields no disasm without erroring; malformed input never panics (battery case).
- `cargo build`, `cargo test -p cpe-server`, `cargo clippy --all-targets -D warnings` green. Both Cargo.locks committed.
- No `#[tauri::command]`, no frontend this slice.

## Notes
Dep is pre-approved by the spike (iced-x86 MIT). Command dispatcher + frontend Disasm tab are later slices. ARM
(yaxpeax-arm) is a future slice. Model: sonnet.
