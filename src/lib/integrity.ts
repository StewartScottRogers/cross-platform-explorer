// Pure integrity model (CPE-790, epic CPE-737). Compare a checksum baseline against a fresh scan and
// classify each path. The key value is the bitrot heuristic: when a file's hash changed but its mtime did
// NOT, the content changed silently (corruption); when both changed, it's a legitimate edit. No DOM/IO —
// unit-tested — so the verify/report layers (CPE-791/792) are thin. Hashes come from the sha256 backend.

/** A baseline / scan entry for one file. */
export interface ChecksumEntry {
  path: string;
  sha256: string;
  size: number;
  modified: number | null;
}

export type IntegrityStatus = "intact" | "edited" | "corrupted" | "missing" | "new";

/** Paths grouped by how they changed relative to the baseline. */
export interface IntegrityReport {
  intact: string[];
  /** Hash changed AND mtime changed → an intended edit. */
  edited: string[];
  /** Hash changed but mtime UNCHANGED → silent corruption (bitrot). */
  corrupted: string[];
  /** In the baseline, absent from the current scan. */
  missing: string[];
  /** In the current scan, absent from the baseline. */
  new: string[];
}

/** Compare `current` against `baseline`, matched by path. Pure. */
export function verifyManifest(baseline: ChecksumEntry[], current: ChecksumEntry[]): IntegrityReport {
  const B = new Map(baseline.map((e) => [e.path, e]));
  const C = new Map(current.map((e) => [e.path, e]));
  const report: IntegrityReport = { intact: [], edited: [], corrupted: [], missing: [], new: [] };

  for (const [path, b] of B) {
    const c = C.get(path);
    if (!c) {
      report.missing.push(path);
    } else if (c.sha256 === b.sha256) {
      report.intact.push(path);
    } else if (c.modified !== b.modified) {
      report.edited.push(path); // content + mtime both moved → deliberate change
    } else {
      report.corrupted.push(path); // content moved but mtime didn't → silent bitrot
    }
  }
  for (const path of C.keys()) {
    if (!B.has(path)) report.new.push(path);
  }
  return report;
}

/** True when the report contains anything alarming (silent corruption or a missing file). */
export function hasIssues(report: IntegrityReport): boolean {
  return report.corrupted.length > 0 || report.missing.length > 0;
}

const isEntry = (x: unknown): x is ChecksumEntry => {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.path === "string" &&
    typeof o.sha256 === "string" &&
    typeof o.size === "number" &&
    (o.modified === null || typeof o.modified === "number")
  );
};

/** Parse a persisted baseline. Tolerant: bad JSON / wrong shape → `[]`, invalid entries dropped. */
export function parseManifest(json: string | null): ChecksumEntry[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isEntry) : [];
  } catch {
    return [];
  }
}

export function serializeManifest(entries: ChecksumEntry[]): string {
  return JSON.stringify(entries);
}
