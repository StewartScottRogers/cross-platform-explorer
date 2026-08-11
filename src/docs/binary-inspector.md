---
title: Binary Inspector
order: 45
category: Previews & Media
categoryOrder: 6
---

# Binary Inspector

Select an executable or library — a Windows `.exe`/`.dll`/`.sys`/`.efi`/`.ocx`/`.scr`/`.cpl`, a Linux/Unix
`.so`, or a macOS `.dylib` — and the preview pane shows its structure in a tabbed, read-only inspector
instead of a raw hex dump: format and architecture, its sections, what it imports and exports, its symbol
table, its .NET/CLR metadata when it's a managed assembly, and (for x86/x64 code) a disassembly listing.

The inspector never executes, decompiles, edits, or writes to the file. It only reads and displays the
structural information already stored in the file's own headers.

## Overview tab

The first thing you see for any recognized binary:

- **Format** — PE (Windows), ELF (Linux/Unix), or Mach-O (macOS).
- **Architecture** — the CPU architecture(s) read from the file's header (for example "x86-64" or
  "ARM64"); a macOS universal/fat binary lists every architecture it bundles.
- **Bitness** — 32-bit or 64-bit.
- **File size**.
- **Sections / Imports / Exports / Symbols** — a quick count for each, so you know what to expect before
  clicking into a tab.

If the file is a managed .NET assembly, a short badge here points you to the **.NET metadata** tab (for
its assembly identity, referenced assemblies, and types) and to the Disassembly tab's explanation of why
its code section isn't decoded as x86/x64 (see *Managed .NET assemblies* below).

## Sections tab

Every section or segment the file declares (for example `.text`, `.data`, `.rdata` on a PE image), each
with its virtual address and in-memory size.

## Imports tab

Every symbol the binary imports from another library, with the owning library shown alongside it where
the format records one (PE and Mach-O tie an import to a specific DLL/dylib; ELF's dynamic-symbol table
doesn't, so that column reads "—" there).

## Exports tab

Every symbol the binary exports for other code to call, with its virtual address where known.

## Symbols tab

Entries from the format's own symbol table (ELF's `.symtab`, Mach-O's `LC_SYMTAB`). A typical Windows
EXE/DLL carries no such table at all — only object files and PDBs do — so this tab is empty for almost
every PE file you'll open, with a note explaining why rather than an unexplained blank grid.

## .NET metadata tab

Only shown for a **managed .NET/CLR assembly** — see *Managed .NET assemblies* below for how that's
detected. Opening this tab reads the file's CLR metadata tables (the ECMA-335 `#~` stream) and shows:

- **Assembly identity** — the assembly's name, version, culture (or "neutral" for the culture-neutral
  default), public key (hex, when strong-named), and any recognized `AssemblyFlags` bits (for example
  `PublicKey`, `Retargetable`) as a row of pills. A file with no assembly manifest at all (a module rather
  than a standalone assembly) shows a short note here instead of these fields.
- **Referenced assemblies** — every assembly this one references, with name, version, culture, and public
  key token.
- **Types** — every type this assembly defines, by namespace and name.
- **Methods** — every method this assembly defines, by name.

This is names and identity only — no method bodies, no IL disassembly, no decompilation (that's a
separate, larger piece of work). A large assembly can carry thousands of referenced assemblies, types, or
methods; each table is capped the same way the Sections/Imports/Exports/Symbols tables are (see *Big
binaries* below).

If the file's CLR header is present but its metadata root can't be located or parsed, the tab says so
plainly instead of showing an empty-looking table — an unreadable result is never presented as if it were
a clean "no assembly manifest" or "no types" result.

## Disassembly tab

An x86/x64 disassembly of the binary's code section: address, raw bytes, and the decoded
mnemonic/operands (for example `mov rbp, rsp`), read straight off the actual instruction stream. Only
fetched once you actually open this tab — decoding it costs roughly twice what the other tabs' data costs
to read, so it's never done for a file you never look at — and it's capped at 2,048 decoded instructions
so an enormous code section can't stall the pane.

Disassembly is only ever attempted for **x86/x64** binaries. A non-x86/x64 architecture, or a file with no
locatable code section, shows a plain "no disassembly available" note instead of an empty table.

### Managed .NET assemblies

A **managed .NET assembly** (an EXE or DLL built to run on the CLR) doesn't hold native machine code in
its "code section" — it holds Common Intermediate Language (CIL) bytecode, which only means something to
the .NET runtime's JIT compiler. Decoding those bytes as if they were x86/x64 produces output that looks
like a real disassembly listing but isn't: real bytes, meaningless instructions.

The inspector detects this by reading the file's actual CLR header (the PE optional header's
`IMAGE_COR20_HEADER` data directory) — a real structural read, not a guess from imports/exports or file
extension. A PE either has this header or it doesn't; ELF and Mach-O files are never managed .NET
assemblies. When it's present, the Overview tab's badge and the Disassembly tab both state plainly that
this is a managed assembly — there's no hedged "possibly" wording, because the flag is authoritative.

The Disassembly tab shows that explanation instead of a decode. A **"Show it anyway"** button is still
there if you want to see the raw (meaningless) decode of the CIL bytes — it stays clearly labelled the
whole time it's shown, so nothing is ever presented as fact that isn't.

## Big binaries

A system library can easily carry over a thousand imports or exports, and a large managed assembly can
likewise carry thousands of referenced assemblies, types, or methods. Every table in the inspector —
Sections/Imports/Exports/Symbols and the .NET metadata tab's referenced-assemblies/types/methods tables —
shows at most 1,000 rows each, comfortably above what most files need but capped so an unusually large
table can't make the pane unresponsive. When a table is capped, a note under it says so plainly and gives
the real total (for example "Showing the first 1,000 of 1,693 exports").

## When a file can't be inspected

- **Not a recognized binary** — the file isn't a PE, ELF, or Mach-O image (including a non-binary file
  renamed with a binary-looking extension, or a corrupt/zero-byte file). A short, calm note says so.
- **Too large** — files above the preview size cap are declined with a clear explanation rather than being
  read into memory.
- **No permission** — a file the app can't read shows a short permission note instead of a raw OS error.

None of these ever show a raw error string or stack trace as the primary message.

## Limits / notes

- **Read-only.** There is no editing, patching, or saving anything back to the file.
- **No decompilation.** The inspector shows structure and (for native code) raw disassembly — it does not
  reconstruct source code, function boundaries beyond what the symbol table already states, or control
  flow graphs.
- **x86/x64 disassembly only.** ARM/ARM64 and other architectures show their header information (format,
  architecture, sections, imports, exports, symbols) normally, but never a disassembly listing.
- **.NET metadata is names and identity only.** The .NET metadata tab shows assembly identity, referenced
  assemblies, and type/method names — it does not disassemble IL, resolve method bodies, or list
  resources/attributes. Full decompilation is tracked as separate, larger work.
- **Extensionless binaries.** An ELF binary with no `.so` extension isn't recognized by this provider —
  detection here is by file extension, not by sniffing the file's magic bytes.
