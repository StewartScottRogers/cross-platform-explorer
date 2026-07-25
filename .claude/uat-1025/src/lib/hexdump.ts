// Pure hex-dump formatting + magic-byte signature detection (CPE-770, epic CPE-719). No DOM, no IO —
// the HexView preview component (CPE-773) renders these rows, so all the fiddly formatting is unit-tested
// here. A row is the canonical `offset  hex-cells  ascii-gutter` triple; the hex cells are padded to a full
// row so a short final row still column-aligns under the ones above it.

/** One rendered line of a hex dump. */
export interface HexRow {
  /** Absolute byte offset of the row start, hex, uppercase, zero-padded to 8 chars. */
  offset: string;
  /** Space-separated two-digit uppercase hex cells, right-padded (with blank cells) to a full row. */
  hex: string;
  /** The printable-ASCII gutter for this row's real bytes (0x20–0x7E as the glyph, else `.`). */
  ascii: string;
  /** Number of real bytes on this row (< bytesPerRow only on the final short row). */
  bytes: number;
}

/**
 * Break a byte range into hex-dump rows. Pure and total: empty input → `[]`, a short final row is padded
 * so the hex columns stay aligned, and a non-zero `baseOffset` is reflected in each row's `offset`.
 */
export function hexRows(bytes: Uint8Array, baseOffset = 0, bytesPerRow = 16): HexRow[] {
  const per = Math.max(1, Math.floor(bytesPerRow) || 1);
  const rows: HexRow[] = [];
  for (let i = 0; i < bytes.length; i += per) {
    const end = Math.min(i + per, bytes.length);
    const cells: string[] = [];
    let ascii = "";
    for (let j = i; j < end; j++) {
      const b = bytes[j];
      cells.push(b.toString(16).toUpperCase().padStart(2, "0"));
      ascii += b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : ".";
    }
    const realBytes = end - i;
    while (cells.length < per) cells.push("  "); // blank cell keeps the ascii column aligned on a short row
    rows.push({
      offset: (baseOffset + i).toString(16).toUpperCase().padStart(8, "0"),
      hex: cells.join(" "),
      ascii,
      bytes: realBytes,
    });
  }
  return rows;
}

/** A recognised file format. */
export interface Signature {
  name: string;
  ext: string;
}

/**
 * Identify a file by its leading magic bytes. Returns `null` when nothing in the starter set matches or
 * the input is too short. Pure.
 */
export function detectSignature(bytes: Uint8Array): Signature | null {
  const starts = (...sig: number[]): boolean =>
    sig.length <= bytes.length && sig.every((v, i) => bytes[i] === v);
  const at = (off: number, ...sig: number[]): boolean =>
    off + sig.length <= bytes.length && sig.every((v, i) => bytes[off + i] === v);

  if (starts(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)) return { name: "PNG image", ext: "png" };
  if (starts(0xff, 0xd8, 0xff)) return { name: "JPEG image", ext: "jpg" };
  if (starts(0x47, 0x49, 0x46, 0x38)) return { name: "GIF image", ext: "gif" }; // GIF8
  if (starts(0x25, 0x50, 0x44, 0x46)) return { name: "PDF document", ext: "pdf" }; // %PDF
  if (starts(0x50, 0x4b, 0x03, 0x04) || starts(0x50, 0x4b, 0x05, 0x06) || starts(0x50, 0x4b, 0x07, 0x08))
    return { name: "ZIP archive", ext: "zip" }; // PK\x03\x04 etc.
  if (starts(0x1f, 0x8b)) return { name: "GZIP archive", ext: "gz" };
  if (starts(0x7f, 0x45, 0x4c, 0x46)) return { name: "ELF executable", ext: "elf" }; // \x7FELF
  if (starts(0x00, 0x61, 0x73, 0x6d)) return { name: "WebAssembly", ext: "wasm" }; // \0asm
  if (starts(0xca, 0xfe, 0xba, 0xbe)) return { name: "Java class", ext: "class" };
  if (starts(0x52, 0x49, 0x46, 0x46) && at(8, 0x57, 0x41, 0x56, 0x45)) return { name: "WAV audio", ext: "wav" }; // RIFF....WAVE
  if (starts(0x4d, 0x5a)) return { name: "Windows PE (MZ)", ext: "exe" }; // MZ — after the more specific ones
  return null;
}
