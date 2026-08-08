---
id: CPE-1485
title: "Binary architecture detection (ELF/PE/Mach-O CPU arch) — extend the file-type detector"
type: Feature
status: Done
priority: Low
component: Backend
tags: [ready]
epic: CPE-1000
created: 2026-08-08
---
## What (a cheap, headless-buildable win from the superfile reference pass — [[superfile-pm-reference]])
superfile added "binary architecture detection for ELF, PE, and Mach-O" (its changelog). CPE's magic-byte
type detector (epic CPE-1000 / `crates/server` file-type module + CPE-1002 inspection) identifies executable
*formats* but not the target **CPU architecture**. Add that: for a detected ELF / PE / Mach-O, parse the small
fixed header fields that carry the machine type and report a normalized arch string (e.g. `x86-64`, `x86`,
`ARM64`, `ARM`, `RISC-V`, plus 32/64-bit and endianness where relevant; for Mach-O universal/fat binaries,
list the contained slices).

## Where / how
- Extend the existing file-type detector in `crates/server` (the CPE-1000/1002 module — grep for the
  magic-byte/type-detection code). Read only the **header** (bounded read — a few hundred bytes; never the
  whole file), following the existing bounded-read + resource-exhaustion conventions.
  - **ELF**: `e_ident[EI_CLASS]` (32/64), `[EI_DATA]` (endian), `e_machine` (arch).
  - **PE**: the COFF header `Machine` field (via the `PE\0\0` offset from the DOS stub).
  - **Mach-O**: `cputype`/`cpusubtype` in the mach header; handle fat/universal (`0xCAFEBABE`) by listing arch slices.
- **No new Cargo deps** (lean-core): these are a handful of byte-offset reads + a match table; hand-roll like
  the other detectors, don't pull a binary-parsing crate unless it's already in the tree.
- Surface it via the existing inspection/metadata path — a field the Properties/inspection panel or a metadata
  column can show (align with CPE-707 columns / CPE-1002 inspection; a later GUI ticket wires the column).

## Verify (headless)
`cargo test` with small hand-built fixtures: a known x86-64 ELF, an ARM64 ELF, a PE (Machine=0x8664 and 0x14c),
a Mach-O (thin + a fat/universal), an endianness case, and non-binary/garbage → `None`/graceful (no panic,
bounded read). `cargo clippy --all-targets -D warnings` both feature modes.

## Effort
Small — pure logic + a match table + fixtures, same pattern as the shipped CPE-1000/1002 detectors. Backend-only,
fully headless-verifiable. Good workshift-batch fodder.

## Work Log

### 2026-08-08 — Worker
Implemented as specified, new pure module + a two-line wire-in to the existing inspection composer:

- **`crates/server/src/bin_arch.rs`** (new): `detect_arch(bytes: &[u8]) -> Option<BinaryArch>`, a pure
  header sniffer mirroring `file_type.rs`'s bounded, never-panics style — no new Cargo deps, hand-rolled
  byte-offset reads + match tables only.
  - `Arch` enum: `X86`, `X86_64`, `Arm`, `Arm64`, `RiscV`, `RiscV64`, `Mips`, `PowerPc`, `PowerPc64`, `Sparc`,
    `Unknown` (a recognised header whose machine/cputype code isn't one of the above — reported rather than
    collapsed to `None`, so bitness/endianness still surface for an architecture this sniffer doesn't
    special-case).
  - `Bitness` (`Bit32`/`Bit64`) and `Endian` (`Little`/`Big`), each `Option` since PE encodes neither
    explicitly (always little-endian by spec; bitness is implied by the machine code, not a separate field).
  - `ArchInfo { arch, bitness, endian }` and `BinaryArch::{Single(ArchInfo), Fat(Vec<ArchInfo>)}` — `Fat` is
    the Mach-O universal-binary case, one entry per contained architecture slice.
  - **ELF**: reads `e_ident[EI_CLASS]` (offset 4), `[EI_DATA]` (offset 5), then `e_machine` (offset 18, 2
    bytes) with byte order selected by `EI_DATA` — 0x03→x86, 0x3E→x86-64, 0x28→ARM, 0xB7→ARM64, 0xF3→RISC-V
    (RiscV64 when `EI_CLASS` says 64-bit, since RISC-V has no distinct 64-bit `e_machine` code), plus
    MIPS/PowerPC/PowerPC64/SPARC. An invalid `EI_DATA` byte (neither 1 nor 2) returns `None` rather than
    guessing an endianness for the `e_machine` read.
  - **PE**: reads the `e_lfanew` pointer at offset 0x3C (always little-endian), verifies the `PE\0\0`
    signature at that offset, then the COFF `Machine` u16 right after it — 0x8664→x86-64, 0x14C→x86,
    0xAA64→ARM64, 0x1C0/0x1C4→ARM.
  - **Mach-O**: thin magics `FE ED FA CE`/`CE FA ED FE` (32-bit) and `FE ED FA CF`/`CF FA ED FE` (64-bit,
    endianness per which magic matched) read `cputype` at offset 4; fat/universal magics `CA FE BA BE`
    (big-endian, the common `lipo`-built form) and `BE BA FE CA` (little-endian) read `nfat_arch` then that
    many 20-byte `fat_arch` entries, each contributing one `ArchInfo` (its own byte order isn't reported —
    that would require walking into each slice's own thin `mach_header`, out of scope for a header-only
    sniff). `nfat_arch` is capped at 64 slices (`MAX_FAT_SLICES`) against a hostile/malformed count before
    any allocation, so a 4-byte field claiming billions of entries can't be used to over-allocate — real
    universal binaries never approach that count.
  - All three paths are pure bounds-checked slice reads (`bytes.get(...)`, no indexing panics) — verified
    with dedicated "shorter than every offset the parser touches" tests for each format, plus empty/1-byte/
    all-0xFF short-input fuzzing that only asserts no panic.
  - 28 unit tests: x86-64/ARM64/x86-32/big-endian ELF, RISC-V 32 vs. 64 via `EI_CLASS`, an unrecognised ELF
    machine code (asserts `Unknown` not `None`), 4 truncation-safety tests; PE x86-64/x86/ARM64/ARM, a
    non-PE MZ stub (DOS-only / corrupted), 2 truncation-safety tests including `e_lfanew` pointing past the
    buffer; Mach-O thin 64-bit little-endian (x86-64, ARM64), 32-bit big-endian, 32-bit little-endian, a
    truncation-safety test; fat Mach-O with 2 slices (the ticket's required case), the little-endian fat
    variant, the hostile-`nfat_arch` cap test, a truncation test, and a genuine 0-slice fat header (asserts
    `Fat(vec![])`, not `None` — a real if degenerate case, distinct from "not Mach-O at all"); plus
    non-binary/empty/1-byte/short-fuzz tests and an `Arch::label()` sanity check.
- **`crates/server/src/inspect.rs`**: added `pub architecture: Option<String>` to the `FileInspection`
  `specta::Type` struct (this is the "surface it via the existing inspection/metadata path" step the ticket
  calls for), computed via `bin_arch::detect_arch(bytes)` and formatted by two small local helpers —
  `architecture_label` (`"Universal: x86-64 + ARM64"` for a fat Mach-O, or the single-arch label otherwise)
  and `arch_info_label` (`"x86-64 (64-bit, little-endian)"` when bitness/endian are known, or bare
  `"x86-64"` for PE where they aren't). 4 new tests: an x86-64 ELF, a PE with no bitness/endian suffix, a
  2-slice fat Mach-O, and non-binary content → `None`.
- **`crates/server/src/lib.rs`**: registered `pub mod bin_arch;` alongside `file_type` with a doc comment
  cross-referencing both.
- Deliberately **not** wired into `PropertiesDialog.svelte` — the ticket scopes this ticket as backend-only
  ("a later GUI ticket wires the column"); the new `architecture` field is present in `FileInspection` /
  `bindings.gen.ts` and ready for that follow-up to consume without any further backend change.
- Decide-and-log: chose `Arch::Unknown` (not `None`) for a well-formed ELF/PE/Mach-O header whose
  machine/cputype code isn't in the curated table, so bitness/endianness are never silently dropped for an
  architecture this sniffer hasn't special-cased yet — consistent with `file_type.rs`'s own philosophy of
  "detect what you can, don't regress a real file to unknown" (see its `resolve_ftyp_brand` fallback
  reasoning).

**Verification (Z: drive worktree `Z:\repos\cpe-1485-wt`, not Temp, per the LNK1104 lock issue noted in
prior tickets):**
- `cargo test --lib` (crates/server) — **1768 passed, 0 failed** (28 new `bin_arch` tests + 4 new `inspect`
  tests included).
- `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean after fixing 2
  `clippy::op_ref` findings (`&bytes[0..4] == [...]` → `bytes[0..4] == [...]`, needless reference on the
  magic-byte comparisons in `detect_arch`).
- `cargo clippy --all-targets --features specta -- -D warnings` (crates/server) — clean.
- `cargo build` (src-tauri, default profile) — OK, `cross-platform-explorer` + `cpe-server` compiled clean.
- `cargo clippy --all-targets -- -D warnings` (src-tauri, default profile) — clean.
- `cargo run --bin export_bindings --features specta-bindings,sidecar-platform` (src-tauri) — regenerated
  `src/lib/bindings.gen.ts`; diff is scoped to the new `architecture: string | null` field + its doc
  comment on `FileInspection`, nothing else changed.
- No new Cargo dependencies; no `Cargo.lock` changes anywhere in the workspace (verified via `git status`
  after the full build/test/clippy pass).
