// Pure binary/byte diff (CPE-778, epic CPE-722). Given two byte buffers, report whether they are equal,
// the first differing offset, and the differing byte ranges — so the hex-compare view (CPE-779) can
// highlight differences over `read_file_range` (CPE-772) without re-scanning. No DOM/IO; unit-tested.

/** A maximal run of differing byte positions. */
export interface ByteRange {
  start: number;
  len: number;
}

export interface ByteDiff {
  /** True when the buffers are byte-for-byte identical (same length, no differences). */
  equal: boolean;
  /** Offset of the first differing byte, or null when equal. */
  firstDiff: number | null;
  /** Coalesced runs of differing positions (over the common length + any trailing extra bytes). */
  ranges: ByteRange[];
  /** True when the two buffers have different lengths. */
  lengthDiffers: boolean;
}

/**
 * Compare two byte buffers. Differences in the common prefix are coalesced into maximal ranges; if the
 * lengths differ, the extra tail of the longer buffer is a trailing differing range (merged with a run
 * that reaches the common end). Pure and total.
 */
export function byteDiff(a: Uint8Array, b: Uint8Array): ByteDiff {
  const min = Math.min(a.length, b.length);
  const ranges: ByteRange[] = [];

  let runStart = -1;
  for (let i = 0; i < min; i++) {
    if (a[i] !== b[i]) {
      if (runStart < 0) runStart = i;
    } else if (runStart >= 0) {
      ranges.push({ start: runStart, len: i - runStart });
      runStart = -1;
    }
  }
  if (runStart >= 0) ranges.push({ start: runStart, len: min - runStart });

  const lengthDiffers = a.length !== b.length;
  if (lengthDiffers) {
    const tailLen = Math.max(a.length, b.length) - min;
    const last = ranges[ranges.length - 1];
    if (last && last.start + last.len === min) {
      last.len += tailLen; // the trailing extra bytes are contiguous with a run that reached the common end
    } else {
      ranges.push({ start: min, len: tailLen });
    }
  }

  const equal = ranges.length === 0;
  return { equal, firstDiff: equal ? null : ranges[0].start, ranges, lengthDiffers };
}
