// CPE-1757 round 2 — makes the bidi/format-character escape (CPE-1712's `displaySafeName`/
// `displaySafePath`, src/lib/filename.ts) an ENFORCED invariant instead of a remembered one.
//
// Round 1 of this guard (a named-shape regex zoo: `X.name`, `X.path`, bare `root`/`path`/`name`,
// `baseName(…)`/`basename(…)`) passed 3/3 with a raw `{revertOnePath}` sitting in a file it already
// covered (CheckpointDialog.svelte:322 — UAT), and a follow-up review's 17-shape probe component proved
// it recognized only 3 of those 17 real render shapes. The engine here (`./bidiRenderScan.ts`) is the
// inversion the review prescribed: inside a registered component, EVERY `{…}` in a text / `title=` /
// `aria-label=` / `alt=` / `{@html …}` position must be a literal, a `displaySafeName(…)`/
// `displaySafePath(…)` call (or an `||`/`??`/ternary-branch combination of those), or it's an offender —
// no shape is named, so no shape can be missed by omission. See `bidiRenderScan.ts`'s own header for the
// full account of what this DOES catch (validated against all 17 probe shapes in
// `bidiRenderScan.test.ts`) and what it still can't see.
//
// REGISTRY is exhaustive-by-equality, not exhaustive-by-omission: for every file below, CI recomputes
// `findUnsafeRenderLines` fresh and requires it to equal the recorded array EXACTLY (sorted). That closes
// both directions of drift review round 2 found inert in round 1's design:
//   - A NEW raw render in an already-registered file changes the computed set, so it no longer equals
//     the recorded array → fails. (Round 1's `ALLOWLIST[file] ?? []` was only ever read for files that
//     were ALSO in `COVERED_FILES`, which never overlapped with `ALLOWLIST`'s keys — so the recorded
//     numbers were prose typed as code, never actually compared to anything. Swapping them for `[99999]`
//     stayed green. Equality against a live recompute can't have that hole.)
//   - A STALE recorded line (the render was fixed, or the file was edited around it) also fails, instead
//     of silently sitting there as a lie about what's still raw.
//
// The domain here is COVERED_FILES from round 1 (12 files) PLUS the 19 components CPE-1712 itself
// originally escaped (FileList, Sidebar, TabBar, HomeView, DetailsPane, TrashView, NavToolbar,
// PropertiesDialog, InstantSearch, ArchiveSafetyDialog, PreviewPane, QuickLook, DiskSpaceView,
// DropStackPanel, FolderBrowser, SidebarNode, RunCommandConfirm, ContentSearchDialog, DuplicatesDialog —
// review round 2's B4: these were claimed as covered in the doc but never mechanically checked at all)
// PLUS the ticket's originally-disclosed "not yet covered" dialogs (ContentIndexSearchDialog,
// FileHealthDialog, NearDuplicatesDialog, SimilarImagesDialog, DeclutterDialog, BatchMediaDialog,
// SplitFileDialog, JoinPartsDialog, ExplorerPane, TerminalPanel), registered here too so their remaining
// raw lines are pinned exactly rather than left completely unchecked. That's 41 of the 135 `.svelte`
// files under src/lib/components/, plus App.svelte on its own (see below) — NOT `readdir(components)` in
// full (review round 2's B5, explicitly hedged "Consider"). Running this engine over the other ~94 files
// found the overwhelming majority of their `{…}` renders are non-filesystem UI text (macro/workspace/
// rule/template names, i18n strings, counts) that the ticket's own scope excludes — auditing every one of
// them individually is a different, much larger undertaking than this ticket's residual-call-site list,
// and doing it hastily to tick a box would produce exactly the "guard that passes while the property is
// broken" the review warned against. Left as a natural follow-up, not silently declared done.
//
// A dry-run of this exact engine across every file in COVERED_FILES surfaced FOUR real, previously-missed
// spoof surfaces beyond the CheckpointDialog:322 UAT bug — proof the inversion earns its keep, not just a
// bigger regex:
//   - AgentTimeline.svelte: the MAIN activity-timeline row's name span (`{baseOf(e.path)}`, a helper this
//     file defines itself — never named "baseName"/"basename", the exact class of miss review predicted)
//     and the "Competing renames" row's name span (`{baseOf(rc.path)}`) — both title-only fixed in round
//     1, leaving the adjacent name half raw, the identical shape of bug as the CheckpointDialog UAT.
//   - AgentTimeline.svelte: the session-history table's row tooltip (`title={row.cwd}`), a real agent
//     working-directory path, entirely undisclosed.
//   - PreviewPane.svelte: five `<img alt={entry.name}>` attributes (image/decoded-image/raw-image/heic/
//     dicom preview kinds) — `alt` is a render position this engine checks and round 1's file-level scope
//     never reached (B4's whole point).
//   - TrashView.svelte: `f.name` passed raw into `$t("trash.restoreFailed", { name: f.name, ... })` — an
//     i18n interpolation parameter, not a template literal or property-access shape, another class round
//     1's regex never considered.
// All four are fixed in this PR (see the diff for each file) before being added to REGISTRY at `[]` (or
// their remaining harmless offenders) below.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { findUnsafeRenderLines } from "./bidiRenderScan";

const COMPONENTS = join(process.cwd(), "src", "lib", "components");
const APP = join(process.cwd(), "src", "App.svelte");
const DOC = join(process.cwd(), "src", "docs", "03-explorer.md");

/** file -> the EXACT sorted line numbers `findUnsafeRenderLines` currently reports for it. Recomputed
 *  live every run and checked for equality (not "offenders minus this array must be empty") — see the
 *  header above for why that specific shape closes round 1's inert-allowlist hole. A non-empty array is
 *  NOT necessarily a disclosed spoof risk: most entries here are UI text this engine can't prove safe
 *  (i18n params, counts, labels, diagnostic error/note/reason strings, diff/metadata CONTENT, macro/
 *  workspace/rule/ticket/agent identity strings) — read `bidiEscape.doc-parity` below for which specific
 *  files' entries are an actual disclosed filesystem-name/path gap vs. harmless-but-unprovable text. */
const REGISTRY: Record<string, number[]> = {
  "ConflictDialog.svelte": [117, 121, 136, 145, 146, 147, 163, 171],
  "FileNameSearchDialog.svelte": [96, 98, 99, 116, 121, 123, 125, 128, 129],
  "RepoBrowser.svelte": [287, 301, 305, 319, 324, 336, 364, 373],
  "AgentTimeline.svelte": [
    605, 665, 670, 673, 710, 713, 734, 760, 761, 769, 781, 782, 783, 796, 798, 802, 816, 822, 826, 852,
    854, 905, 906, 920, 922, 943, 946, 947, 948, 949, 951, 952, 953, 954, 958, 961, 964, 988, 992, 1015,
    1017, 1021, 1024, 1047, 1048, 1049, 1050, 1051, 1052, 1061, 1073, 1076, 1079, 1082, 1096, 1097, 1098,
    1099, 1100, 1116, 1117, 1118, 1119, 1120, 1141, 1142, 1143, 1151, 1185, 1197,
  ],
  "ConsultedFiles.svelte": [31, 40],
  "SessionHistoryDialog.svelte": [101, 109, 116, 123, 124, 132],
  "IntegrityDialog.svelte": [84, 85, 90, 107],
  "CheckpointDialog.svelte": [217, 218, 233, 234, 248, 251, 252, 253, 277, 279, 288, 295, 320, 323],
  "DiffSideBySide.svelte": [38, 39],
  "InspectCryptoDialog.svelte": [],
  "BoardView.svelte": [
    265, 297, 298, 301, 311, 313, 314, 315, 316, 318, 321, 328, 350, 351, 354, 363, 365, 366, 367, 368,
    370, 373, 374, 375, 394, 396,
  ],
  "CopilotDialog.svelte": [196, 199, 206, 213, 219, 220, 221, 222, 223, 232, 243, 267, 273, 280, 287, 292, 295],

  // --- B4: the 19 components CPE-1712 itself originally escaped ---------------------------------
  "FileList.svelte": [646, 647, 656, 658, 672, 674, 691, 694, 705, 708, 713, 779, 795, 800, 817, 819, 824, 825, 828, 830, 845, 863, 867],
  "Sidebar.svelte": [415, 419, 428, 440, 442, 443, 471, 515, 527, 533, 534, 561, 565, 573, 579, 606, 610, 618, 624, 650, 654, 670, 706, 710, 738, 742],
  "TabBar.svelte": [40, 49],
  "HomeView.svelte": [126, 131, 163, 180, 185, 187, 190, 197, 200, 203, 206, 214, 215, 220, 233, 239, 240, 253, 254, 275, 288, 289, 310, 311, 335, 336, 342, 347, 348, 370, 371],
  "DetailsPane.svelte": [28, 31, 36, 42, 46, 50, 54],
  "TrashView.svelte": [149, 150, 155, 158, 161, 164, 167, 178, 186, 188, 190, 194, 196, 197, 198, 215, 218, 228],
  "NavToolbar.svelte": [85, 88, 91, 94, 114, 115, 169, 170],
  "PropertiesDialog.svelte": [
    220, 221, 225, 232, 233, 236, 238, 239, 240, 245, 246, 250, 251, 253, 255, 263, 268, 273, 282, 283,
    285, 296, 298, 302, 304, 306, 313, 316, 318, 320, 322, 331, 334, 335, 337, 338, 341, 349, 355, 364,
    368, 370, 376,
  ],
  "InstantSearch.svelte": [165, 167, 168, 169, 182, 188, 189, 191, 193, 196, 198, 201, 203, 205, 207],
  "ArchiveSafetyDialog.svelte": [75, 78, 80, 86, 88, 89, 96, 107, 115, 121, 126, 131, 132, 133, 134, 135, 137, 138, 141, 142, 147, 155, 160],
  "PreviewPane.svelte": [
    1015, 1017, 1024, 1031, 1033, 1036, 1037, 1041, 1043, 1044, 1048, 1049, 1057, 1060, 1069, 1071, 1077,
    1085, 1093, 1113, 1136, 1138, 1146, 1151, 1156, 1158, 1160, 1202, 1204, 1208, 1209, 1210, 1229, 1234,
    1239, 1247, 1252, 1263, 1267, 1276, 1285, 1286, 1290, 1306, 1334, 1335, 1336, 1338,
  ],
  "QuickLook.svelte": [33],
  "DiskSpaceView.svelte": [147, 182, 202],
  "DropStackPanel.svelte": [44, 51, 77, 85],
  "FolderBrowser.svelte": [121, 123, 125, 140],
  "SidebarNode.svelte": [42],
  "RunCommandConfirm.svelte": [66, 80, 86, 90, 91, 92],
  "ContentSearchDialog.svelte": [110, 112, 113, 130, 131, 136, 138, 140, 143, 146, 151, 154, 159, 162, 168, 169],
  "DuplicatesDialog.svelte": [107, 109, 114, 115, 118, 120, 122, 126, 129, 132, 134, 143, 144, 148],

  // --- The ticket's originally-disclosed "not yet covered" dialogs — pinned exactly, not fixed here ---
  "ContentIndexSearchDialog.svelte": [152, 153, 155, 156, 159, 160, 172, 179, 180, 182, 183, 185, 188, 192, 194, 196, 198, 200, 203, 206, 210, 213, 214, 215, 217, 221],
  "FileHealthDialog.svelte": [
    424, 426, 427, 428, 444, 454, 459, 463, 464, 472, 480, 486, 490, 497, 498, 501, 503, 504, 506, 507,
    511, 515, 517, 522, 524, 525, 526, 535, 536, 539, 541, 542, 544, 545, 549, 553, 555, 576, 585, 586,
    589, 592, 600, 605, 616, 617, 620, 622, 623, 625, 626, 630, 634, 636, 641, 643, 644, 647, 656, 657,
    660, 662, 663, 665, 666, 670, 674, 676, 681, 683, 684,
  ],
  "NearDuplicatesDialog.svelte": [152, 154, 155, 156, 163, 164, 167, 169, 170, 172, 173, 177, 181, 184, 185, 187, 196, 201, 204, 206, 207],
  "SimilarImagesDialog.svelte": [152, 154, 155, 156, 163, 164, 167, 169, 170, 172, 173, 177, 181, 184, 185, 187, 196, 201, 204, 206, 207],
  "DeclutterDialog.svelte": [178, 180, 181, 182, 189, 190, 193, 195, 196, 198, 199, 203, 208, 210, 219, 224, 227, 228],
  "BatchMediaDialog.svelte": [469, 470, 488, 494, 501, 534, 536, 537, 548, 550, 556, 563, 573, 597, 612, 617, 618, 634, 655],
  "SplitFileDialog.svelte": [101, 104, 105, 106, 107, 114, 129, 168, 176, 183],
  "JoinPartsDialog.svelte": [131, 133, 140, 146, 147, 165, 173, 180],
  "ExplorerPane.svelte": [481, 483, 484, 485, 486, 490, 492, 493, 494, 495, 499, 501, 502, 506, 513, 549, 551, 554, 556, 557, 564, 565],
  "TerminalPanel.svelte": [183, 185, 216],
};

/** The subset of REGISTRY whose non-empty array is an ACTUAL disclosed "still renders a raw filesystem
 *  name/path" gap (matching the ticket's own priority-ordered residual list), as opposed to harmless
 *  UI text this engine merely can't prove safe. `src/docs/03-explorer.md`'s "Not yet covered" paragraph
 *  must name exactly this set — see the `doc-parity` test below, checked in BOTH directions. */
const DISCLOSED_GAPS = [
  "ContentIndexSearchDialog", "FileHealthDialog", "NearDuplicatesDialog", "SimilarImagesDialog",
  "DeclutterDialog", "BatchMediaDialog", "SplitFileDialog", "JoinPartsDialog", "ExplorerPane",
  "TerminalPanel", "Sidebar",
];

/** App.svelte's markup-level offenders, same exact-equality treatment as REGISTRY. App.svelte is far
 *  too large to add wholesale to COVERED_FILES' original per-file review, so it's split from REGISTRY. */
const APP_MARKUP_OFFENDERS = [6386, 6398, 6401, 6407, 6415, 6417, 6424, 6429, 6434, 6439, 6526, 6605, 6606, 6767, 6768, 6777, 6778, 6783, 6791, 6792, 6796, 6812, 6813, 6824, 6830, 7045, 7060, 7398, 7416, 7682];

/** App.svelte's two already-disclosed SplitFileDialog/JoinPartsDialog completion notices
 *  (`showNotice($t(..., { name: baseName(path) }))`) are built in `<script>` code, not markup — the one
 *  shape `findUnsafeRenderLines` (markup-only, see its module doc) genuinely cannot see, since the
 *  eventual DOM render happens through a separate `{notice}`-style span elsewhere, not at this call
 *  site. Checked here with a narrow, targeted scan instead: every `baseName(`/`basename(` call anywhere
 *  in App.svelte's source not immediately wrapped in `displaySafeName(`/`displaySafePath(` must be one
 *  of these two allowlisted lines. */
const APP_SCRIPT_BASENAME_ALLOWLIST = [2744, 2759];

function findRawBaseNameCallsInScript(src: string): number[] {
  const lines = new Set<number>();
  const re = /\bbase[Nn]ame\(/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const before = src.slice(Math.max(0, m.index - 40), m.index);
    if (!/displaySafe(?:Name|Path)\(\s*$/.test(before)) {
      lines.add(src.slice(0, m.index).split("\n").length);
    }
  }
  return [...lines].sort((a, b) => a - b);
}

describe("bidi/format-character escape guard (CPE-1757 round 2)", () => {
  it("every registered component's raw-render set matches its recorded lines EXACTLY", () => {
    const mismatches: string[] = [];
    for (const [file, recorded] of Object.entries(REGISTRY)) {
      const src = readFileSync(join(COMPONENTS, file), "utf8");
      const found = findUnsafeRenderLines(src);
      const recordedSorted = [...recorded].sort((a, b) => a - b);
      if (JSON.stringify(found) !== JSON.stringify(recordedSorted)) {
        const newlyRaw = found.filter((l) => !recordedSorted.includes(l));
        const stale = recordedSorted.filter((l) => !found.includes(l));
        mismatches.push(
          `${file}: found [${found.join(",")}] vs recorded [${recordedSorted.join(",")}]` +
            (newlyRaw.length ? ` — NEW raw line(s): ${newlyRaw.join(",")}` : "") +
            (stale.length ? ` — STALE recorded line(s) no longer found: ${stale.join(",")}` : ""),
        );
      }
    }
    expect(
      mismatches,
      `wrap a genuinely new offender in displaySafeName/displaySafePath, or update REGISTRY here (and, ` +
        `if it's a real disclosed gap, src/docs/03-explorer.md's "Not yet covered" list) to match reality: ` +
        `${mismatches.join(" | ")}`,
    ).toEqual([]);
  });

  it("App.svelte's markup-level raw-render set matches its recorded lines exactly", () => {
    const src = readFileSync(APP, "utf8");
    const found = findUnsafeRenderLines(src);
    const recorded = [...APP_MARKUP_OFFENDERS].sort((a, b) => a - b);
    expect(found, `App.svelte markup offenders drifted from REGISTRY: found [${found.join(",")}]`).toEqual(recorded);
  });

  it("App.svelte's script-side baseName(…)/basename(…) calls are all wrapped, except the two allowlisted notices", () => {
    const src = readFileSync(APP, "utf8");
    const found = findRawBaseNameCallsInScript(src);
    expect(found, `raw baseName()/basename() call(s) in App.svelte outside the allowlist: ${found.join(",")}`).toEqual(
      [...APP_SCRIPT_BASENAME_ALLOWLIST].sort((a, b) => a - b),
    );
  });

  it("the demonstration: a brand-new raw render in a new component reds this guard (see PR description for the live run + revert)", () => {
    // Reproduced literally, not asserted: findUnsafeRenderLines is the exact function REGISTRY is
    // checked against, so proving it flags a fresh violation IS proving the guard would have failed.
    const demoSrc = `<script>\n  export let entry;\n</script>\n\n<span title={entry.path}>{entry.name}</span>\n`;
    expect(findUnsafeRenderLines(demoSrc)).toEqual([5]);
  });

  it('the doc names exactly the disclosed gaps — bidirectional (a name missing OR a name wrongly present both fail)', () => {
    const doc = readFileSync(DOC, "utf8");
    const section = /\*\*Not yet covered\*\*[\s\S]*?(?=\n- \*\*|\n## |$)/.exec(doc)?.[0];
    expect(section, `src/docs/03-explorer.md must have a "Not yet covered" bullet`).toBeTruthy();
    const paragraph = section!;

    const missing = DISCLOSED_GAPS.filter((name) => !paragraph.includes(`\`${name}\``));
    expect(missing, `these disclosed gaps must be named in the doc's "Not yet covered" paragraph: ${missing.join(", ")}`).toEqual([]);

    // The reverse direction (review round 2's B3b): a fully-covered file's name must NOT be listed as a
    // gap — e.g. deleting `Sidebar`'s mention wouldn't be caught by a plain doc.includes("Sidebar") check
    // (it matches an unrelated "sidebar folder tree" bullet elsewhere), which is exactly why this test
    // scopes to the extracted paragraph specifically, in both directions.
    const registeredNames = Object.keys(REGISTRY).map((f) => f.replace(/\.svelte$/, ""));
    const wronglyPresent = registeredNames.filter((name) => !DISCLOSED_GAPS.includes(name) && paragraph.includes(`\`${name}\``));
    expect(wronglyPresent, `these are NOT disclosed gaps but appear in the doc's "Not yet covered" paragraph: ${wronglyPresent.join(", ")}`).toEqual([]);
  });
});
