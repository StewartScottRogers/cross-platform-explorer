/**
 * File-checksum helpers (CPE-413).
 *
 * Verifying a download against a published hash is the point of computing one (CPE-412). Comparison
 * must be forgiving of how hashes are copied around: different case, and stray whitespace (leading,
 * trailing, or embedded — some sites print SHA-256 in space-separated groups).
 */

/** Normalize a hex digest for comparison: drop ALL whitespace and lowercase it. */
export function normalizeDigest(s: string): string {
  return s.replace(/\s+/g, "").toLowerCase();
}

/**
 * Whether `computed` matches the user-supplied `expected` digest. Returns `null` (neutral — show no
 * verdict) when either side is empty, so an untouched input never reads as a mismatch.
 */
export function checksumMatches(computed: string, expected: string): boolean | null {
  const c = normalizeDigest(computed);
  const e = normalizeDigest(expected);
  if (!c || !e) return null;
  return c === e;
}
