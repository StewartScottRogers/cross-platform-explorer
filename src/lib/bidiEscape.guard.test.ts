// CPE-1757: makes the bidi/format-character escape (CPE-1712's `displaySafeName` / `displaySafePath`,
// src/lib/filename.ts) an ENFORCED invariant instead of a remembered one. CPE-1712's round-2 review
// verdict: nothing prevented a new call site from rendering a filesystem name/path raw — a second helper
// name (`displaySafePath`) is a discoverability signpost, not a barrier. This test is the barrier.
//
// Follows the two patterns this repo already owns for exactly this shape of problem:
//   - src/lib/sectionDocs.test.ts — an exhaustive registry check (every `Section` maps to a real doc).
//   - src/lib/invoke.guard.test.ts — a grep-based boundary guard with an explicit, justified allowlist.
//
// Scope: the domain scanned here is exactly the set of components CPE-1712 and CPE-1757 have already
// reviewed for filesystem-derived name/path rendering — COVERED_FILES (must come back fully escaped)
// and ALLOWLIST (deliberately left raw, one-for-one with src/docs/03-explorer.md's "Not yet covered"
// list). A component outside this domain that starts rendering a filesystem name/path is not
// auto-discovered by this test — the discipline it enforces is the same one sectionDocs.test.ts uses for
// a new `Section`: a component that starts drawing a directory-listing/search-result/audit-log name or
// path must ADD ITSELF to COVERED_FILES here (or, with a recorded reason, to ALLOWLIST + the doc), the
// same way a new Section must register its doc slug. Omitting that step is exactly what round 1 of
// CPE-1712 did, and exactly what this test now catches.
//
// Demonstrated per CPE-1757's acceptance criteria: a temporary component with a raw `{entry.path}` text
// render was added to COVERED_FILES, this test was run and failed with that file+line reported as an
// uncovered offender, then both the component and its registration were reverted. See the CPE-1757 PR
// description for the transcript.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const COMPONENTS = join(process.cwd(), "src", "lib", "components");
const APP = join(process.cwd(), "src", "App.svelte");
const DOC = join(process.cwd(), "src", "docs", "03-explorer.md");

/** Components fixed by CPE-1757 (on top of CPE-1712's original ~20) where EVERY raw filesystem
 *  name/path render the scanner below finds must be wrapped in displaySafeName/displaySafePath. */
const COVERED_FILES: string[] = [
  "ConflictDialog.svelte", // the overwrite-or-skip decision — undisclosed entirely before CPE-1757
  "FileNameSearchDialog.svelte", // :97's root span, inconsistent with its already-fixed ContentSearchDialog/DuplicatesDialog siblings
  "RepoBrowser.svelte",
  "AgentTimeline.svelte",
  "ConsultedFiles.svelte",
  "SessionHistoryDialog.svelte",
  "IntegrityDialog.svelte",
  "CheckpointDialog.svelte",
  "DiffSideBySide.svelte",
  "InspectCryptoDialog.svelte",
  "BoardView.svelte",
  "CopilotDialog.svelte",
];

/** file -> line numbers deliberately left raw (CPE-1757 review): lower-consequence diagnostic read-outs
 *  rather than decision surfaces, though the reviewer confirmed they CAN show attacker-supplied names.
 *  One-for-one with src/docs/03-explorer.md's "Not yet covered" list — a change to one without the other
 *  is exactly the drift this guard exists to stop; see the "matches the doc" test below. */
const ALLOWLIST: Record<string, number[]> = {
  "ContentIndexSearchDialog.svelte": [153, 210, 213],
  "FileHealthDialog.svelte": [427, 522, 524, 576, 585, 641, 643, 681, 683],
  "NearDuplicatesDialog.svelte": [155, 206],
  "SimilarImagesDialog.svelte": [155, 206],
  "DeclutterDialog.svelte": [181, 227, 228],
  "BatchMediaDialog.svelte": [470, 534, 536, 573, 597, 618],
  "SplitFileDialog.svelte": [101, 114],
  "JoinPartsDialog.svelte": [42, 131, 140],
  "ExplorerPane.svelte": [551],
  "TerminalPanel.svelte": [185],
  "Sidebar.svelte": [428, 443],
};

/** App.svelte is far too large to add wholesale to COVERED_FILES, so it gets its own narrow check: two
 *  allowlisted lines (the SplitFileDialog/JoinPartsDialog "done" notice strings), everything else raw
 *  the scanner finds is a failure. */
const APP_ALLOWLIST: number[] = [2744, 2759];

/** True if the text immediately before `idx` (ignoring whitespace) is the opening of a
 *  `displaySafeName(`/`displaySafePath(` call — i.e. the match at `idx` is already escaped. */
function isWrapped(src: string, idx: number): boolean {
  const before = src.slice(Math.max(0, idx - 40), idx);
  return /displaySafe(?:Name|Path)\(\s*$/.test(before);
}

/** True if `idx` (the position of a `{`) sits somewhere the DOM actually draws it: plain text content
 *  (`>{…}<`), a `title=`/`aria-label=`/`alt=` attribute given directly as `attr={…}`, or one embedded in
 *  a quoted `attr="…{…}…"` value (e.g. `aria-label="Diff for {baseOf(path)}"`). Everything else —
 *  `bind:value={path}` (an editable input's live value), a component prop pass-through like
 *  `<DiffSideBySide path={sbs.path}>` or `<JwtPreview {path} />` (the raw bytes travel to a leaf that
 *  escapes its OWN render, same boundary QuickLook's `<img src>` relies on) — is deliberately NOT a
 *  render site and must not be flagged. */
function isRenderContext(src: string, idx: number): boolean {
  const before = src.slice(Math.max(0, idx - 120), idx);
  if (/>\s*$/.test(before)) return true;
  if (/\b(?:title|aria-label|alt)=\{?\s*$/.test(before)) return true;
  if (/\b(?:title|aria-label|alt)="[^"]*$/.test(before)) return true;
  return false;
}

function lineOf(src: string, idx: number): number {
  return src.slice(0, idx).split("\n").length;
}

/** Raw filesystem name/path render patterns (CPE-1757's own description of the guard): a bare
 *  `X.name`/`X.path` property read, a bare `{name}`/`{path}`/`{root}` identifier (the FileNameSearchDialog
 *  `{root}`-fallback shape), or a `baseName(…)`/`basename(…)` call — none of them wrapped in the escape.
 *  The first two are gated to actual render contexts (see `isRenderContext`); a `baseName(…)`/`basename(…)`
 *  call is flagged wherever it's unwrapped — including a JS-built notice string like App.svelte's
 *  `showNotice($t(..., { name: baseName(path) }))` — since extracting a basename is itself always in
 *  service of showing it to the user, template or not. */
function findRawOffenders(src: string): number[] {
  const lines = new Set<number>();
  const BARE_PROP = /\{\s*[a-zA-Z_$][\w$]*\.(?:name|path)\s*\}/g;
  const BARE_IDENT = /\{\s*(?:root|path|name)\s*\}/g;
  const BASENAME_CALL = /\bbase[Nn]ame\(/g;

  for (const re of [BARE_PROP, BARE_IDENT]) {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) {
      if (isRenderContext(src, m.index) && !isWrapped(src, m.index)) lines.add(lineOf(src, m.index));
    }
  }
  BASENAME_CALL.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = BASENAME_CALL.exec(src)) !== null) {
    if (!isWrapped(src, m.index)) lines.add(lineOf(src, m.index));
  }
  return [...lines].sort((a, b) => a - b);
}

describe("bidi/format-character escape guard (CPE-1757)", () => {
  it("every covered component's filesystem name/path renders are fully escaped", () => {
    const offenders: string[] = [];
    for (const file of COVERED_FILES) {
      const src = readFileSync(join(COMPONENTS, file), "utf8");
      const allowed = new Set(ALLOWLIST[file] ?? []);
      for (const line of findRawOffenders(src)) {
        if (!allowed.has(line)) offenders.push(`${file}:${line}`);
      }
    }
    expect(
      offenders,
      `these render a filesystem name/path without displaySafeName/displaySafePath (src/lib/filename.ts) — ` +
        `wrap them, or add a justified entry to ALLOWLIST here AND src/docs/03-explorer.md's "Not yet ` +
        `covered" list: ${offenders.join(", ")}`,
    ).toEqual([]);
  });

  it("App.svelte's escape is complete outside its two allowlisted notice strings", () => {
    const src = readFileSync(APP, "utf8");
    const allowed = new Set(APP_ALLOWLIST);
    const offenders = findRawOffenders(src)
      .filter((l) => !allowed.has(l))
      .map((l) => `App.svelte:${l}`);
    expect(offenders, `raw filesystem name/path render(s) in App.svelte: ${offenders.join(", ")}`).toEqual([]);
  });

  it("the allowlist here names the same components as the doc's disclosed 'Not yet covered' list", () => {
    const doc = readFileSync(DOC, "utf8");
    for (const file of Object.keys(ALLOWLIST)) {
      const name = file.replace(/\.svelte$/, "");
      expect(doc.includes(name), `${name} must be named in src/docs/03-explorer.md's "Not yet covered" list`).toBe(true);
    }
  });
});
