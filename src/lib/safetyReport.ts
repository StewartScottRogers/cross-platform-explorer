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

/** One category of file-health finding. */
export type Category = "type-mismatch" | "zip-bomb" | "dangling-link" | "orphaned-sidecar" | "empty-folder";

/**
 * Severity ranking, highest risk first: a disguised file (extension/content mismatch) or a zip-bomb
 * entry is a security concern (`"high"`); a dangling/cyclic link is `"medium"`; an orphaned sidecar is
 * `"low"` (tidiness, not risk); an empty folder is purely informational (`"info"`).
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

function archiveFindings(report: ArchiveSafetyReport | undefined): Finding[] {
  if (!report) return [];
  return report.report.flagged.map((entry: FlaggedEntry) =>
    finding("zip-bomb", entry.name, `archive expands ${Math.round(entry.ratio)}x`),
  );
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
