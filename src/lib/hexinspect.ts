// Pure data-inspector decoders for the hex inspector (CPE-771, epic CPE-719). Given a byte buffer and an
// offset, interpret the bytes as the common scalar types in a chosen endianness — the "data inspector"
// panel a hex editor shows for the selection. No DOM/IO; fully unit-tested so the panel (CPE-773) just
// renders these rows. Rows that would read past the buffer end are omitted rather than throwing.

/** One decoded interpretation of the bytes at the cursor. */
export interface InspectRow {
  /** Type label, e.g. "int32", "float64", "FILETIME". */
  type: string;
  /** Human-readable decoded value. */
  value: string;
}

const FILETIME_EPOCH_DIFF = 116444736000000000n; // 100ns ticks between 1601-01-01 and 1970-01-01

function fmtFloat(n: number): string {
  if (Number.isNaN(n)) return "NaN";
  if (!Number.isFinite(n)) return n > 0 ? "Infinity" : "-Infinity";
  // Trim to a readable precision without trailing-zero noise.
  return Number(n.toPrecision(9)).toString();
}

function isoOrRange(ms: number): string {
  if (!Number.isFinite(ms)) return "(out of range)";
  try {
    return new Date(ms).toISOString();
  } catch {
    return "(out of range)";
  }
}

/** Printable-ASCII run starting at `offset` (stops at the first non-printable byte or `max` chars). */
function asciiPreview(bytes: Uint8Array, offset: number, max: number): string {
  let s = "";
  for (let i = offset; i < bytes.length && s.length < max; i++) {
    const b = bytes[i];
    if (b < 0x20 || b > 0x7e) break;
    s += String.fromCharCode(b);
  }
  return s;
}

/**
 * Decode the bytes at `offset` across the common scalar types in the given endianness. Only rows with
 * enough bytes remaining are returned; an offset on the last byte still yields int8/uint8. Pure.
 */
export function inspect(bytes: Uint8Array, offset: number, littleEndian: boolean): InspectRow[] {
  const rows: InspectRow[] = [];
  if (!Number.isInteger(offset) || offset < 0 || offset >= bytes.length) return rows;
  const remaining = bytes.length - offset;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const le = littleEndian;
  const add = (type: string, value: string | number | bigint) => rows.push({ type, value: String(value) });

  if (remaining >= 1) {
    add("int8", dv.getInt8(offset));
    add("uint8", dv.getUint8(offset));
  }
  if (remaining >= 2) {
    add("int16", dv.getInt16(offset, le));
    add("uint16", dv.getUint16(offset, le));
  }
  if (remaining >= 4) {
    add("int32", dv.getInt32(offset, le));
    add("uint32", dv.getUint32(offset, le));
    add("float32", fmtFloat(dv.getFloat32(offset, le)));
  }
  if (remaining >= 8) {
    add("int64", dv.getBigInt64(offset, le));
    add("uint64", dv.getBigUint64(offset, le));
    add("float64", fmtFloat(dv.getFloat64(offset, le)));
  }

  add("string", asciiPreview(bytes, offset, 16));

  if (remaining >= 4) {
    const secs = dv.getUint32(offset, le);
    add("unix32", isoOrRange(secs * 1000));
  }
  if (remaining >= 8) {
    const ft = dv.getBigUint64(offset, le);
    // FILETIME → ms since Unix epoch. Divide as BigInt to avoid precision loss, then to Number.
    const ms = Number((ft - FILETIME_EPOCH_DIFF) / 10000n);
    add("FILETIME", isoOrRange(ms));
  }

  return rows;
}
