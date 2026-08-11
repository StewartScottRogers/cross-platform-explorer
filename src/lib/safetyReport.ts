/**
 * File Health view-model (CPE-1293, epic CPE-1002): unifies the five safety-scan result shapes
 * (`ArchiveSafetyReport`, `EmptyDirsReport`, `OrphanSidecarResult`, `DanglingReport`, `MismatchReport` —
 * CPE-1287) into one sorted, grouped `FileHealthReport` — the headless contract a future cleanup/review
 * panel renders. Pure TS, zero Svelte, zero Rust, no I/O: `buildFileHealth` is a synchronous transform
 * over already-fetched scan results.
 */
import type {
  ArchiveSafetyReport,
  DanglingReason,
  DanglingReport,
  EmptyDirsReport,
  FlaggedEntry,
  MismatchReport,
  OrphanSidecarResult,
} from "./bindings.gen";

/** One category of file-health finding. `"archive-unreadable"` (CPE-1603) is deliberately NOT the same
 *  bucket as `"zip-bomb"`: it means the archive path couldn't be assessed at all (or only partially),
 *  never that a bomb was actually found — see `archiveFindings`. */
export type Category = "type-mismatch" | "zip-bomb" | "dangling-link" | "orphaned-sidecar" | "empty-folder" | "archive-unreadable";

/**
 * Severity ranking, highest risk first: a disguised file (extension/content mismatch) or a zip-bomb
 * entry is a security concern (`"high"`); an archive that couldn't be (fully) assessed is `"medium"` —
 * genuinely unknown, not confirmed dangerous, but not nothing either (CPE-1603); a dangling/cyclic link
 * is also `"medium"`; an orphaned sidecar is `"low"` (tidiness, not risk); an empty folder is purely
 * informational (`"info"`).
 */
export type Severity = "high" | "medium" | "low" | "info";

/** Lower sorts first — high severity, then path, ascending. */
const SEVERITY_RANK: Record<Severity, number> = { high: 0, medium: 1, low: 2, info: 3 };

/** Plain ordinal path comparator (not `localeCompare`, which can reorder around punctuation like a
 *  leading path separator depending on ICU collation) — deterministic across locales/platforms. */
const ordinalCmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

const CATEGORY_SEVERITY: Record<Category, Severity> = {
  "type-mismatch": "high",
  "zip-bomb": "high",
  "archive-unreadable": "medium",
  "dangling-link": "medium",
  "orphaned-sidecar": "low",
  "empty-folder": "info",
};

/** One unified finding, ready for display. */
export interface Finding {
  category: Category;
  severity: Severity;
  path: string;
  summary: string;
}

/** The unified "File Health" view-model produced by `buildFileHealth`. */
export interface FileHealthReport {
  findings: Finding[];
  byCategory: Record<Category, number>;
  status: "healthy" | "issues";
  truncated: boolean;
}

/** The five scan results `buildFileHealth` accepts, all optional — an absent scan contributes nothing. */
export interface FileHealthInputs {
  archive?: ArchiveSafetyReport;
  emptyDirs?: EmptyDirsReport;
  orphans?: OrphanSidecarResult;
  dangling?: DanglingReport;
  mismatches?: MismatchReport;
}

const finding = (category: Category, path: string, summary: string): Finding => ({
  category,
  severity: CATEGORY_SEVERITY[category],
  path,
  summary,
});

/** Basename of a `/`- or `\`-separated path (last segment); the whole string if there's no separator. */
function basename(path: string): string {
  const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return idx === -1 ? path : path.slice(idx + 1);
}

/** Human one-liner for a dangling/cyclic link, keyed off its flagged reason. */
function danglingSummary(reason: DanglingReason): string {
  return reason === "Cyclic" ? "link → cyclic reference" : "link → missing target";
}

/**
 * CPE-1603: mirrors `ArchiveSafetyDialog`'s tri-state rather than reading only `report.flagged`. Two
 * "couldn't actually scan this" signals must never collapse into zero findings (which `buildFileHealth`
 * reads as a clean bill of health): `report.unreadable` (CPE-1320) means the archive itself couldn't be
 * opened at all; `report.unreadable_entries > 0` (CPE-1591, widened by CPE-1602) means it opened but one
 * or more entries inside it couldn't be read (encrypted, or a bounded verification that ran out of
 * budget) — in both cases `dangerous: false` on the readable portion means "we don't know", not "safe".
 * Emitted under the dedicated `"archive-unreadable"` category (never `"zip-bomb"`) so a genuinely unknown
 * result stays structurally distinct from a confirmed one. The report type carries no archive path of its
 * own (CPE-1603 is landing ahead of the archive-tab slice that will supply one), so these two findings use
 * an empty `path` — the caller that eventually wires an archive tab in should thread the real path through.
 */
function archiveFindings(report: ArchiveSafetyReport | undefined): Finding[] {
  if (!report) return [];
  const findings: Finding[] = report.report.flagged.map((entry: FlaggedEntry) =>
    finding("zip-bomb", entry.name, `archive expands ${Math.round(entry.ratio)}x`),
  );
  if (report.unreadable) {
    findings.push(finding("archive-unreadable", "", "archive could not be opened — safety not checked"));
  } else if (report.unreadable_entries > 0) {
    const n = report.unreadable_entries;
    findings.push(
      finding(
        "archive-unreadable",
        "",
        `${n} ${n === 1 ? "entry" : "entries"} could not be read — archive safety not fully checked`,
      ),
    );
  }
  return findings;
}

function emptyDirFindings(report: EmptyDirsReport | undefined): Finding[] {
  if (!report) return [];
  return report.dirs.map((dir) => finding("empty-folder", dir, "empty folder"));
}

function orphanFindings(report: OrphanSidecarResult | undefined): Finding[] {
  if (!report) return [];
  return report.orphans.map((path) => finding("orphaned-sidecar", path, "sidecar with no primary"));
}

function danglingFindings(report: DanglingReport | undefined): Finding[] {
  if (!report) return [];
  return report.links.map((link) => finding("dangling-link", link.path, danglingSummary(link.reason)));
}

function mismatchFindings(report: MismatchReport | undefined): Finding[] {
  if (!report) return [];
  return report.hits.map((hit) =>
    finding("type-mismatch", hit.path, `${basename(hit.path)} is actually a ${hit.detected_label}`),
  );
}

/**
 * Unify the five scan results into one sorted, grouped `FileHealthReport`. Findings are sorted by
 * severity (high → info) then by `path` (ascending). Every input is optional — an absent scan is
 * treated as "not run", contributing neither findings nor a `truncated` flag; when every scan is absent
 * (or every present scan came back clean), the result is `status: "healthy"` with an empty `findings`
 * list, never an error. `truncated` is true when ANY supplied report was itself truncated (an
 * incomplete sweep), so the UI can flag the health view as partial.
 */
export function buildFileHealth(inputs: FileHealthInputs): FileHealthReport {
  const findings = [
    ...mismatchFindings(inputs.mismatches),
    ...archiveFindings(inputs.archive),
    ...danglingFindings(inputs.dangling),
    ...orphanFindings(inputs.orphans),
    ...emptyDirFindings(inputs.emptyDirs),
  ].sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity] || ordinalCmp(a.path, b.path));

  const byCategory: Record<Category, number> = {
    "type-mismatch": 0,
    "zip-bomb": 0,
    "archive-unreadable": 0,
    "dangling-link": 0,
    "orphaned-sidecar": 0,
    "empty-folder": 0,
  };
  for (const f of findings) byCategory[f.category]++;

  const truncated = Boolean(
    inputs.archive?.truncated ||
      inputs.emptyDirs?.truncated ||
      inputs.orphans?.truncated ||
      inputs.dangling?.truncated ||
      inputs.mismatches?.truncated,
  );

  return { findings, byCategory, status: findings.length === 0 ? "healthy" : "issues", truncated };
}
