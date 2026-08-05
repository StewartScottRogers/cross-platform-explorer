import { describe, it, expect } from "vitest";
import { buildFileHealth } from "./safetyReport";
import type {
  ArchiveSafetyReport,
  DanglingReport,
  EmptyDirsReport,
  MismatchReport,
  OrphanSidecarResult,
} from "./bindings.gen";

const archive = (over: Partial<ArchiveSafetyReport> = {}): ArchiveSafetyReport => ({
  report: {
    total_compressed: 100,
    total_uncompressed: 50000,
    overall_ratio: 500,
    flagged: [{ name: "payload.bin", ratio: 500 }],
    dangerous: true,
  },
  entries_scanned: 10,
  truncated: false,
  unreadable: false,
  ...over,
});

const emptyDirs = (over: Partial<EmptyDirsReport> = {}): EmptyDirsReport => ({
  dirs: ["/proj/old/empty"],
  scanned: 20,
  truncated: false,
  ...over,
});

const orphans = (over: Partial<OrphanSidecarResult> = {}): OrphanSidecarResult => ({
  orphans: ["/proj/foo.srt"],
  scanned: 30,
  truncated: false,
  ...over,
});

const dangling = (over: Partial<DanglingReport> = {}): DanglingReport => ({
  links: [{ path: "/proj/link-b", reason: "Missing" }],
  scanned: 40,
  truncated: false,
  ...over,
});

const mismatches = (over: Partial<MismatchReport> = {}): MismatchReport => ({
  hits: [{ path: "/proj/foo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" }],
  scanned: 50,
  truncated: false,
  ...over,
});

describe("buildFileHealth severity ranking (CPE-1293)", () => {
  it("sorts a mix of all five categories high -> info, with a path tiebreak within a severity", () => {
    const report = buildFileHealth({
      archive: archive(),
      emptyDirs: emptyDirs(),
      orphans: orphans(),
      dangling: dangling(),
      mismatches: mismatches(),
    });

    expect(report.findings.map((f) => f.category)).toEqual([
      "type-mismatch", // high (before zip-bomb: "/proj/foo.jpg" < "payload.bin")
      "zip-bomb", // high
      "dangling-link", // medium
      "orphaned-sidecar", // low
      "empty-folder", // info
    ]);
    expect(report.findings.map((f) => f.severity)).toEqual(["high", "high", "medium", "low", "info"]);
  });

  it("breaks ties within the same severity by path, ascending", () => {
    const report = buildFileHealth({
      mismatches: mismatches({
        hits: [
          { path: "/proj/zzz.jpg", claimed_ext: "jpg", detected_label: "PE", detected_ext: "exe" },
          { path: "/proj/aaa.jpg", claimed_ext: "jpg", detected_label: "PE", detected_ext: "exe" },
        ],
      }),
    });
    expect(report.findings.map((f) => f.path)).toEqual(["/proj/aaa.jpg", "/proj/zzz.jpg"]);
  });

  it("produces sensible human summaries per category", () => {
    const report = buildFileHealth({
      archive: archive(),
      emptyDirs: emptyDirs(),
      orphans: orphans(),
      dangling: dangling(),
      mismatches: mismatches(),
    });
    const byCat = Object.fromEntries(report.findings.map((f) => [f.category, f.summary]));
    expect(byCat["type-mismatch"]).toBe("foo.jpg is actually a Windows executable/library");
    expect(byCat["zip-bomb"]).toBe("archive expands 500x");
    expect(byCat["dangling-link"]).toBe("link → missing target");
    expect(byCat["orphaned-sidecar"]).toBe("sidecar with no primary");
    expect(byCat["empty-folder"]).toBe("empty folder");
  });

  it("labels a cyclic dangling link distinctly from a missing one", () => {
    const report = buildFileHealth({
      dangling: dangling({ links: [{ path: "/proj/loop", reason: "Cyclic" }] }),
    });
    expect(report.findings[0].summary).toBe("link → cyclic reference");
  });
});

describe("buildFileHealth per-category counts (CPE-1293)", () => {
  it("counts findings per category, including zero for categories with no hits", () => {
    const report = buildFileHealth({
      mismatches: mismatches({
        hits: [
          { path: "/a.jpg", claimed_ext: "jpg", detected_label: "PE", detected_ext: "exe" },
          { path: "/b.jpg", claimed_ext: "jpg", detected_label: "PE", detected_ext: "exe" },
        ],
      }),
      dangling: dangling(),
    });
    expect(report.byCategory).toEqual({
      "type-mismatch": 2,
      "zip-bomb": 0,
      "dangling-link": 1,
      "orphaned-sidecar": 0,
      "empty-folder": 0,
    });
  });
});

describe("buildFileHealth empty-state (CPE-1293)", () => {
  it("all-empty (well-formed but zero-hit) inputs yield status healthy, no findings, no throw", () => {
    const report = buildFileHealth({
      archive: archive({ report: { total_compressed: 1, total_uncompressed: 1, overall_ratio: 1, flagged: [], dangerous: false } }),
      emptyDirs: emptyDirs({ dirs: [] }),
      orphans: orphans({ orphans: [] }),
      dangling: dangling({ links: [] }),
      mismatches: mismatches({ hits: [] }),
    });
    expect(report.findings).toEqual([]);
    expect(report.status).toBe("healthy");
    expect(report.truncated).toBe(false);
    expect(report.byCategory).toEqual({
      "type-mismatch": 0,
      "zip-bomb": 0,
      "dangling-link": 0,
      "orphaned-sidecar": 0,
      "empty-folder": 0,
    });
  });

  it("no inputs at all is also healthy and never throws", () => {
    expect(() => buildFileHealth({})).not.toThrow();
    const report = buildFileHealth({});
    expect(report.status).toBe("healthy");
    expect(report.findings).toEqual([]);
    expect(report.truncated).toBe(false);
  });
});

describe("buildFileHealth truncation propagation (CPE-1293)", () => {
  it("is false when no input scan was truncated", () => {
    const report = buildFileHealth({ dangling: dangling({ truncated: false }), emptyDirs: emptyDirs({ truncated: false }) });
    expect(report.truncated).toBe(false);
  });

  it("is true when exactly one input scan was truncated, even if the rest were clean", () => {
    const report = buildFileHealth({
      archive: archive({ truncated: false }),
      emptyDirs: emptyDirs({ truncated: false }),
      orphans: orphans({ truncated: true }),
      dangling: dangling({ truncated: false }),
    });
    expect(report.truncated).toBe(true);
  });

  it("is true when the mismatch scan alone was truncated", () => {
    const report = buildFileHealth({ mismatches: mismatches({ truncated: true }) });
    expect(report.truncated).toBe(true);
  });
});
