// Binary Inspector preview provider (CPE-1597, epic CPE-1562 slice 4): pure, framework-free helpers
// behind `BinaryPreview.svelte` — kept separate (mirrors `csv.ts`/`jsonTree.ts`/`outline.ts`) so the
// capping/classification/managed-.NET-detection logic is unit-testable without mounting a component.

import type { BinaryExport, BinaryImport, BinaryInfo } from "../bindings.gen";

/** Render cap for a Sections/Imports/Exports/Symbols table (CPE-1597, "big binaries must not stall the
 *  pane"). The backend already bounds each list at `MAX_BINARY_LIST_ENTRIES` (4096, guards against a
 *  hostile/malformed count field driving an unbounded allocation) — this is a SEPARATE, tighter cap on
 *  what actually gets rendered as DOM rows, because an ordinary (non-hostile) system DLL can still carry
 *  1,000+ real entries (kernel32.dll: ~1,274 imports, ~1,693 exports) and an unvirtualized table that
 *  large can visibly stall the pane on a slow machine. Simpler than full virtualization (mirrors the
 *  existing `CSV_ROW_CAP` precedent in `PreviewPane.svelte`) and, per CPE-1597, must be labelled
 *  honestly rather than silently truncating. */
export const BINARY_TABLE_ROW_CAP = 1000;

/** A list capped for display, with enough information to render an honest "showing N of M" note. */
export interface CappedRows<T> {
  /** The rows to actually render — `all` unchanged when not capped. */
  rows: T[];
  /** The real total count, even when capped. */
  total: number;
  /** True when `rows.length < total` — i.e. the cap actually cut something. */
  capped: boolean;
}

/** Cap `all` to at most `cap` rows for rendering. Pure and total: an empty/short list is never "capped". */
export function capRows<T>(all: T[], cap: number = BINARY_TABLE_ROW_CAP): CappedRows<T> {
  return { rows: all.length > cap ? all.slice(0, cap) : all, total: all.length, capped: all.length > cap };
}

/** The three calm, human buckets a `binary_info`/`binary_disasm` command failure sorts into (CPE-1597).
 *  The backend already returns human-readable `Err` text (see `ensure_previewable_size` and
 *  `fs::read`'s io::Error Display) — this classifies it so the component can show a short, specific
 *  sentence instead of dumping that raw string as the primary message. */
export type BinaryErrorKind = "too-large" | "permission" | "unrecognized";

/** Classify a `binaryInfo`/`binaryDisasm` error message. Pure string matching — deliberately loose
 *  (case-insensitive substring) since the exact wording is Rust's, not ours, and can drift. Falls back to
 *  "unrecognized" (covers a goblin parse failure on a non-binary/truncated/zero-byte file, and any other
 *  message this hasn't seen before) — the safest bucket, since it never claims a permission or size
 *  problem that isn't actually there. */
export function classifyBinaryError(message: string): BinaryErrorKind {
  const m = message.toLowerCase();
  if (m.includes("too large")) return "too-large";
  // "Access is denied" (Windows io::Error Display) / "Permission denied" (Unix) / raw os-error codes
  // (5 = ERROR_ACCESS_DENIED on Windows, 13 = EACCES on Linux/macOS).
  if (
    m.includes("access is denied") ||
    m.includes("permission denied") ||
    m.includes("os error 5)") ||
    m.includes("os error 13)")
  ) {
    return "permission";
  }
  return "unrecognized";
}

/**
 * Confidence level for the frontend-side managed-.NET heuristic below — deliberately not a plain boolean,
 * so the component can word its caveat honestly instead of asserting a guess as fact. "confirmed" only
 * for the strong signal (see below); "possible" for a weaker signal that's real but ambiguous; "none"
 * otherwise.
 */
export type ManagedConfidence = "confirmed" | "possible" | "none";

/**
 * Extensions where an EMPTY import/export table is the **normal, by-design** shape for a legitimate
 * native binary — not a hint of anything unusual, let alone managed .NET (CPE-1597, fixing a code-review
 * finding on the first version of this heuristic):
 *
 * - `.efi` — UEFI drivers/boot applications reach firmware services through the `EFI_SYSTEM_TABLE`
 *   pointer handed to them at entry, never through a PE import table. A zero-import, zero-export `.efi`
 *   is the norm, not an anomaly — flagging one as "possibly managed" was actively wrong, hid a perfectly
 *   real, valid x86/x64 disassembly behind an unnecessary extra click, and told the user an everyday
 *   shape was "unusual" when it's standard.
 * - `.sys` — Windows kernel drivers. They can likewise carry a near-empty (occasionally fully empty)
 *   import table depending on how they reach the kernel/HAL, but the stronger reason is categorical: the
 *   CLR does not run in kernel mode at all, so a kernel driver can never legitimately be a managed .NET
 *   assembly in the first place. "Possibly managed" is inapplicable to this format regardless of how
 *   common empty import tables happen to be among drivers.
 *
 * The zero-imports-AND-zero-exports signal below is SKIPPED entirely for these extensions (never
 * downgraded to a lesser warning — it simply isn't evidence for this format), so `.efi`/`.sys` never gate
 * their Disassembly tab behind the "possible" caveat's opt-in click. The "confirmed" signal (an actual
 * `mscoree.dll` import / CLR export) is unaffected and still applies to any extension — it's real evidence
 * regardless of format.
 */
const EMPTY_TABLES_NORMAL_EXTS = new Set(["efi", "sys"]);

/**
 * Best-effort, frontend-side detection of a managed .NET assembly (CPE-1597) — so the Disassembly tab
 * never presents CIL bytecode decoded as x86/x64 as if it were real machine code (confirmed nonsense on
 * `mscorlib.dll` during CPE-1585's UAT: 2,048 meaningless "instructions").
 *
 * TODO(CPE-1596): that ticket is adding a real `is_managed` flag to `BinaryInfo`, computed backend-side
 * from the actual CLR header (the IMAGE_COR20_HEADER pointed at by the PE optional header's 15th data
 * directory) — once it lands, prefer `info.is_managed` directly and retire this whole heuristic (the
 * `EMPTY_TABLES_NORMAL_EXTS` carve-out below goes away with it — a real CLR-header read never needs a
 * per-format exception list). Landing this ticket first (rather than blocking on that parallel branch)
 * means the caveat ships now, on data the backend already returns.
 *
 * Two signals, checked in order:
 *
 * 1. **"confirmed"** — the PE imports `mscoree.dll` (checked case-insensitively by owning library) or
 *    exports the CLR loader entry points `_CorExeMain`/`_CorDllMain`. This is the classic, well-known
 *    signal: an OLDER or 32-bit-targeted CLR-hosted image carries exactly this one native import so the
 *    plain PE/COFF loader can bootstrap the runtime before any managed code runs. Applies to ANY
 *    extension — an actual CLR import/export is real evidence no matter what the file is named.
 *
 * 2. **"possible"** — the PE has **zero imports and zero exports**, AND its extension isn't in
 *    {@link EMPTY_TABLES_NORMAL_EXTS} (where that shape is the norm, not a signal — see its doc comment).
 *    Verified against a real 64-bit `mscorlib.dll`
 *    (`C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll`) during this ticket's own manual
 *    testing: it reports `imports: 0, exports: 0` — signal 1 alone MISSES it, because a modern x64/AnyCPU
 *    pure-IL assembly is loaded straight off its CLR header, with no legacy import-table stub at all. An
 *    ordinary EXE/DLL, by contrast, almost always imports *something* (at minimum a handful of kernel32
 *    functions), so an import-free, export-free one is worth a hedged caveat there — but this signal is
 *    weak either way (a real IL assembly usually carries exactly one `mscoree.dll` import, which the
 *    "confirmed" tier already catches), so the UI must never claim this shape is itself unusual — only
 *    that it's consistent with a guess, not a finding.
 *
 * Neither signal reads the CLR header itself, so a packed/obfuscated .NET binary that strips its imports
 * AND happens to carry unrelated exports could still slip past both — an acceptable gap for a caveat, not
 * a security boundary, and exactly what the real `is_managed` flag (CPE-1596) will close for good.
 */
export function managedDotNetConfidence(
  info: Pick<BinaryInfo, "format" | "imports" | "exports">,
  extension: string,
): ManagedConfidence {
  if (info.format !== "Pe") return "none";
  const confirmed =
    info.imports.some((i: BinaryImport) => (i.library ?? "").toLowerCase() === "mscoree.dll") ||
    info.exports.some((e: BinaryExport) => e.name === "_CorExeMain" || e.name === "_CorDllMain");
  if (confirmed) return "confirmed";
  if (EMPTY_TABLES_NORMAL_EXTS.has(extension.toLowerCase())) return "none";
  if (info.imports.length === 0 && info.exports.length === 0) return "possible";
  return "none";
}

/**
 * Whether `extension` is a format where an empty import/export table is normal-by-design (CPE-1597) —
 * exported so the component can show a short, non-gating, purely informational note explaining WHY the
 * Imports/Exports tabs are empty for one of these files, without implying anything about managed .NET
 * (that heuristic already skips these extensions entirely — see {@link EMPTY_TABLES_NORMAL_EXTS}). Kept
 * separate from {@link managedDotNetConfidence} so the note can render even though that function
 * correctly returns "none" here (nothing to caveat, but a user staring at two empty tables still
 * deserves an explanation).
 */
export function emptyImportExportIsNormalFor(extension: string): boolean {
  return EMPTY_TABLES_NORMAL_EXTS.has(extension.toLowerCase());
}

/** Human label for a `BinaryFormat` value. */
export function formatLabel(format: BinaryInfo["format"]): string {
  switch (format) {
    case "Pe":
      return "PE (Windows executable/library)";
    case "Elf":
      return "ELF (Linux/Unix executable/library)";
    case "MachO":
      return "Mach-O (macOS executable/library)";
    default:
      return format;
  }
}

/** Zero-pad a virtual address to an even-width hex string with a `0x` prefix (e.g. `0x00401000`). `null`
 *  (an export/symbol with no known address) renders as an em dash rather than "0x0", which would read as
 *  a real address at the image base. */
export function hexAddress(address: number | null): string {
  if (address === null) return "—";
  const hex = address.toString(16);
  const width = hex.length <= 4 ? 4 : hex.length <= 8 ? 8 : hex.length <= 16 ? 16 : hex.length;
  return "0x" + hex.padStart(width, "0");
}
