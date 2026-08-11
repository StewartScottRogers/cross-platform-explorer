// Binary Inspector preview provider (CPE-1597, epic CPE-1562 slice 4; CPE-1615 slice 5): pure,
// framework-free helpers behind `BinaryPreview.svelte` — kept separate (mirrors
// `csv.ts`/`jsonTree.ts`/`outline.ts`) so the capping/classification/.NET-metadata-formatting logic is
// unit-testable without mounting a component.

import type { BinaryInfo } from "../bindings.gen";

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
 * Known bits of ECMA-335 II.23.1.2 `AssemblyFlags`, decoded from `DotnetAssemblyIdentity.flags` for
 * display (CPE-1615) — the backend exposes the raw column unparsed (see its own doc comment in
 * `bindings.gen.ts`), so the frontend names the bits a user is likely to care about rather than showing a
 * bare hex/decimal number. Deliberately not exhaustive (the processor-architecture sub-field and a couple
 * of rarely-set compatibility bits are omitted) — an unrecognized bit simply contributes no pill, it's
 * never dropped silently from anything that matters (the raw value is still available via `flags` itself
 * if ever needed).
 */
const ASSEMBLY_FLAG_BITS: ReadonlyArray<readonly [number, string]> = [
  [0x0001, "PublicKey"],
  [0x0100, "Retargetable"],
  [0x0200, "DisableJITcompileOptimizer"],
  [0x4000, "EnableJITcompileTracking"],
];

/** Decode the recognized bits of an `AssemblyFlags` value into short display labels (CPE-1615), for
 *  rendering as a reflowing pill row. Pure and total: an unrecognized/zero value yields `[]`, never throws. */
export function decodeAssemblyFlags(flags: number): string[] {
  return ASSEMBLY_FLAG_BITS.filter(([bit]) => (flags & bit) !== 0).map(([, label]) => label);
}

/** Human label for a nullable CLR culture string (CPE-1615) — `null`/empty means the neutral culture, per
 *  `DotnetAssemblyIdentity.culture`'s and `DotnetAssemblyRef.culture`'s own doc comments. */
export function cultureLabel(culture: string | null): string {
  return culture && culture.length > 0 ? culture : "neutral";
}

/** Render a nullable hex-blob field (`public_key`/`public_key_token`) for display (CPE-1615) — an em dash
 *  for `None` (no key/token present), matching {@link hexAddress}'s convention for "nothing here" rather
 *  than an empty cell that could be mistaken for a loading gap. */
export function hexOrDash(hex: string | null): string {
  return hex && hex.length > 0 ? hex : "—";
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
