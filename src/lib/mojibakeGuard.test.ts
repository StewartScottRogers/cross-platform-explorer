// CPE-1771: the whole-repo mojibake guard.
//
// Attempt 1 of this ticket matched a hand-picked "lead character + artifact character" pair table, which
// an independent review found caught only 13 of 34 real mojibake shapes -- it missed ordinary double-
// encoded accented Latin, the CP1252 symbol block, and every non-Latin script (Greek, Cyrillic, Hebrew,
// Arabic, CJK, kana, Hangul) plus emoji. Attempt 2 (this file, paired with `./mojibakeGuard.ts`) replaces
// the pair table with an actual UTF-8 round-trip validator: text is mojibake when its characters decode
// from individual CP1252 bytes AND those same bytes, read back together, form strictly valid UTF-8 for
// exactly one Unicode code point. See `mojibakeGuard.ts`'s header for the full account.
//
// Attempt 1 also scoped the tree walk by directory exclusion + file-extension inclusion, which an
// independent review found silently skipped 1899 of 3190 tracked files (60%) -- including lockfiles
// (`src-tauri/Cargo.lock`, the shipped-app lockfile: exactly the "release manifest" category this ticket
// is named after), `.svg`/`.xml` assets, and every extensionless tracked file. This attempt drives the
// walk from `git ls-files` instead: every TRACKED file is scanned unless it is binary (a NUL byte in its
// first 4 KB) or under one of the two directory exclusions below, each with a stated, ticketed reason --
// not a hand-maintained extension list that silently drops a category nobody thought to add.
//
// Three kinds of test live here:
//   - Unit tests of `findMojibake`/`hasLeadingBom` against synthetic strings built with
//     `simulateCp1252Corruption` (real UTF-8 text run through the actual corruption this guard exists to
//     catch), not hand-typed corrupted literals -- typing a corrupted literal by hand is exactly the
//     mistake that briefly made an earlier draft of `mojibakeGuard.ts` trip its own guard.
//   - The shape matrix: one test per script/character class (accented Latin, CP1252 symbols, Greek,
//     Cyrillic, Hebrew, Arabic, CJK, kana, Hangul, emoji, a mojibake'd BOM, ...), each generated the same
//     way, proving the detector's coverage is not a re-hidden version of round 1's shape list.
//   - The tree-wide scan, which is the actual CI guard: it fails if any tracked, non-binary file contains
//     the mojibake signature or opens with a UTF-8 BOM, unless the exact (file, line, kind) is in
//     `ALLOWLIST` with a recorded reason.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { findMojibake, hasLeadingBom, simulateCp1252Corruption } from "./mojibakeGuard";

const ROOT = process.cwd();

describe("findMojibake (CPE-1771 attempt 2)", () => {
  it("catches the em-dash mojibake this repo shipped in Cargo.toml/tauri.conf.json", () => {
    const corrupted = simulateCp1252Corruption("a normal dependency — it pulls only serde.");
    expect(findMojibake(corrupted)).toHaveLength(1);
  });

  it("catches the ellipsis mojibake found in Cargo.toml", () => {
    const corrupted = simulateCp1252Corruption("sftp://…, webdav://…");
    expect(findMojibake(corrupted)).toHaveLength(2);
  });

  it("catches the arrow mojibake found in CLAUDE.md and crates/server/src/dispatch.rs", () => {
    const corrupted = simulateCp1252Corruption("window (`start → end`)");
    expect(findMojibake(corrupted)).toHaveLength(1);
  });

  it("reports 1-based line numbers across multiple lines", () => {
    const corrupted = `clean line one\nclean line two\nbad ${simulateCp1252Corruption("—")} line three`;
    expect(findMojibake(corrupted).map((o) => o.line)).toEqual([3]);
  });

  it("does NOT flag a bare accented letter used as an ordinary letter", () => {
    // Romanian, Portuguese, French all use U+00E2 ("a" with circumflex) as a normal letter. None of
    // these are followed by a CP1252 continuation-range character, so none should match.
    const legit = ["Română", "Câmera", "Distância focal", "Tâches"];
    for (const s of legit) {
      expect(findMojibake(s), s).toEqual([]);
    }
  });

  it('does NOT flag the real CPE-1771 false positive: i18n.ts\'s Portuguese "NÃO"', () => {
    // Ã (A-tilde) followed by a plain ASCII letter is real Portuguese, not a double-encoded artifact.
    expect(findMojibake('"prop.noMatchTip": "O arquivo NÃO corresponde"')).toEqual([]);
  });

  it("does NOT flag an arrow preceded by a multiplication sign with a space between (mediaTransport.ts:105)", () => {
    // U+00D7 (multiplication sign) is a valid CP1252 lead byte, but U+2192 (arrow) is not itself a
    // CP1252-producible character at all, so it can never complete a continuation run -- and here the two
    // are separated by an ordinary space besides. Real text from src/lib/mediaTransport.ts:105.
    expect(findMojibake("wrapping 2× → 0.5×.")).toEqual([]);
  });
});

describe("shape matrix (CPE-1771 attempt 2) -- catches corruption across scripts, not a re-hidden shape list", () => {
  const SHAPES: Record<string, string> = {
    "accented Latin (e-acute)": "café",
    "accented Latin (n-tilde)": "mañana",
    "accented Latin (u-diaeresis)": "über",
    "accented Latin (c-cedilla)": "façade",
    "accented Latin (o-diaeresis)": "können",
    "CP1252 symbol block (copyright/degree/registered/plus-minus)": "© ° ® ±",
    "CP1252 symbol block (guillemets)": "«hello»",
    "em dash": "—",
    "en dash": "–",
    "ellipsis": "…",
    "curly quotes": "“hello” ‘hi’",
    "arrow": "→",
    "bullet": "•",
    "non-breaking space": "a b",
    "box drawing": "┌─┐",
    "check mark": "✓",
    "warning sign": "⚠",
    "Greek": "Ελληνικά",
    "Cyrillic": "Привет",
    "Hebrew": "שלום",
    "Arabic": "مرحبا",
    "CJK (Chinese)": "中文",
    "kana (Japanese hiragana)": "こんにちは",
    "Hangul": "안녕하세요",
    "heart": "♥",
    "star": "★",
    "emoji (astral, 4-byte UTF-8)": "\u{1f389}", // party popper
    "BOM (U+FEFF)": "﻿",
  };

  for (const [label, original] of Object.entries(SHAPES)) {
    it(`catches ${label}`, () => {
      const corrupted = simulateCp1252Corruption(original);
      const offenders = findMojibake(corrupted);
      expect(offenders.length, `expected ${label} (source ${JSON.stringify(original)}) to be caught after corruption`).toBeGreaterThan(0);
    });
  }

  it("catches the real CPE-1771 attempt-2 false positive shape (mediaTransport.ts:33: multiplication sign + en dash decodes as valid UTF-8 for Hebrew Zayin)", () => {
    // This is deliberately NOT run through simulateCp1252Corruption -- it's the real, uncorrupted source
    // text "0.5x-2x" where "x" is U+00D7 and "-" is U+2013 (en dash), and it happens to satisfy the
    // detector's round-trip check on its own: bytes [0xD7, 0x96] (the CP1252 bytes for those two
    // characters) are themselves strictly valid UTF-8 for U+05D6 (HEBREW LETTER ZAYIN). That coincidence
    // -- not a corrupted literal -- is exactly why src/lib/mediaTransport.ts:33 needs an ALLOWLIST entry.
    // Built from individual code points (not a literal "x-2x" glyph pair) so this source file itself
    // doesn't carry the adjacency and trip its own guard when the tree-wide scan reaches this file.
    const multiplicationSign = String.fromCharCode(0x00d7);
    const enDash = String.fromCharCode(0x2013);
    const realWorldCoincidence = `(0.5${multiplicationSign}${enDash}2${multiplicationSign})`;
    expect(findMojibake(realWorldCoincidence)).toHaveLength(1);
  });
});

describe("hasLeadingBom (CPE-1771)", () => {
  it("detects a UTF-8 BOM", () => {
    expect(hasLeadingBom(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b]))).toBe(true);
  });

  it("does not false-positive on ordinary content", () => {
    expect(hasLeadingBom(new Uint8Array([0x7b, 0x0a, 0x20]))).toBe(false);
  });

  it("does not throw on a too-short buffer", () => {
    expect(hasLeadingBom(new Uint8Array([0xef]))).toBe(false);
    expect(hasLeadingBom(new Uint8Array([]))).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------
// The tree-wide CI guard.
// ---------------------------------------------------------------------------------------------------

/** Directory prefixes never scanned, each for a stated, ticketed reason -- everything else TRACKED is
 *  scanned (see `listTrackedFiles` below), so a new file can't hide in a directory nobody thought to
 *  include the way attempt 1's extension include-list did. */
const EXCLUDE_PREFIXES = [
  // Ticketing/Tickets & Ticketing/Epics: measured at CPE-1771 time to carry 683 pre-existing mojibake
  // occurrences across 14 files and a UTF-8 BOM in 12 files -- a much larger, separate cleanup (filed as
  // CPE-1784) than this ticket's two named manifests. Scanning it here would red CI for a backlog this
  // ticket didn't create and isn't sized to clear. Remove this exclusion when CPE-1784 lands.
  "Ticketing/",
  // samples/**: CPE-1042 requires this tree to be a byte-exact reproducible baseline (`.gitattributes`
  // marks it `-text`, i.e. deliberately opted out of normal text handling). A future encoding/BOM test
  // fixture in this corpus (a very plausible addition for a file explorer's text-preview samples) would
  // be a deliberately mojibake'd or BOM'd file, and this guard's only remedy -- "repair it byte-exact" --
  // would destroy the fixture it's supposed to be. Excluded outright rather than half-included by
  // extension (attempt 1's mistake: 7 samples/** files were scanned, the rest silently weren't).
  "samples/",
];

interface AllowlistEntry {
  file: string; // repo-relative, forward-slash
  line: number;
  kind: "mojibake" | "bom";
  reason: string;
  /** A distinctive substring the entry's line must still contain -- the staleness check below verifies
   *  this instead of re-matching the detector, so a genuine false-positive entry (which is SUPPOSED to
   *  never match, that's why it's a false positive rather than tracked corruption) doesn't itself read as
   *  stale. If a line no longer contains this substring, the entry has drifted and must be re-verified. */
  contains: string;
}

/** Exact (file, line, kind) triples excluded from the guard, each with a recorded reason -- per CPE-1771's
 *  acceptance criteria, "allowlisted by location with a reason", not by weakening the detector. `kind` is
 *  checked (not just file+line) so a future mojibake entry landing on the same line as a BOM check can
 *  never silently suppress the OTHER kind of violation too -- the exact shape of bug that shipped release
 *  0.57.66 (commit 86888aed added a BOM alongside mojibake to the same two files; a kind-blind allowlist
 *  entry for one would have hidden the other). Two classes of entry:
 *   - Genuine false positives: text that legitimately satisfies the detector's round-trip check without
 *     being corruption (a coincidental valid-UTF-8 reading of unrelated bytes, or a deliberate literal
 *     BOM-handling regex). Recorded here anyway, by exact location, so the claim is auditable.
 *   - Known, tracked, currently-out-of-scope real corruption: `crates/` was off-limits during CPE-1771's
 *     sprint slot (a concurrent worker was live in it), so these three lines ARE real, un-repaired
 *     mojibake the guard would otherwise (correctly) fail on. Tracked via CPE-1783 instead of fixed here
 *     -- the reason says so plainly; it does not claim they're false positives. */
const ALLOWLIST: AllowlistEntry[] = [
  {
    file: "src/lib/i18n.ts",
    line: 5320,
    kind: "mojibake",
    reason:
      'Legitimate Portuguese "NÃO" (N + A-tilde + O) -- Ã followed by the ASCII letter "O", not a CP1252 ' +
      "continuation byte. Confirmed by the #927/CPE-1752 review and re-verified for CPE-1771 (both attempts).",
    contains: "NÃO",
  },
  {
    file: "src/lib/docs.ts",
    line: 25,
    kind: "mojibake",
    reason:
      "Literal U+FEFF (BOM) character embedded in a regex literal (`raw.replace(/^\\uFEFF/, \"\")`) that " +
      "strips a BOM from loaded doc markdown -- deliberate BOM-handling code, not a mojibake artifact. " +
      "(The detector's own round-trip check does not flag this line at all -- a bare U+FEFF is not a CP1252 " +
      "lead byte -- but it is recorded here per the ticket's \"allowlisted by location with a reason\" AC.)",
    contains: "raw.replace(/^",
  },
  {
    file: "src/lib/epicsQueueLayout.test.ts",
    line: 24,
    kind: "mojibake",
    reason: "Same literal U+FEFF BOM-stripping regex as src/lib/docs.ts:25, duplicated for this guard's own frontmatter parsing.",
    contains: "const body = md.replace(/^",
  },
  {
    file: "src/lib/epicsQueueLayout.test.ts",
    line: 35,
    kind: "mojibake",
    reason: "Same literal U+FEFF BOM-stripping regex as src/lib/docs.ts:25, a second call site in this file.",
    contains: "md.replace(/^",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 6,
    kind: "mojibake",
    reason:
      "Real, un-repaired mojibake (an arrow, U+2192, corrupted the same way as CLAUDE.md's) that CPE-1752's " +
      "repair of this same file missed. crates/ was off-limits during CPE-1771's sprint slot (a concurrent " +
      "worker was live in it) -- tracked as CPE-1783, NOT a false positive. Remove this entry when CPE-1783 lands.",
    contains: "an unknown method",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 7,
    kind: "mojibake",
    reason: "Same CPE-1783 arrow corruption as line 6, a second occurrence two lines down.",
    contains: "deserialize",
  },
  {
    file: "crates/server/src/dispatch.rs",
    line: 11,
    kind: "mojibake",
    reason: "Same CPE-1783 arrow corruption as line 6, a third occurrence.",
    contains: "A handler never panics the dispatcher.",
  },
  {
    file: "src/lib/mediaTransport.ts",
    line: 33,
    kind: "mojibake",
    reason:
      'Genuine algorithmic false positive found by the #936 review of attempt 2: "0.5x-2x" (multiplication ' +
      "sign, U+00D7, immediately followed by an en dash, U+2013). Their CP1252 bytes, 0xD7 0x96, are " +
      "themselves strictly valid UTF-8 for U+05D6 (HEBREW LETTER ZAYIN), so the round-trip check is " +
      "satisfied by pure coincidence. Not corruption -- see the dedicated unit test above.",
    // Built from code points, not the literal glyph pair -- see the comment on the unit test above for why.
    contains: `0.5${String.fromCharCode(0x00d7)}${String.fromCharCode(0x2013)}2${String.fromCharCode(0x00d7)}`,
  },
];

function isAllowlisted(relFile: string, line: number, kind: "mojibake" | "bom"): boolean {
  return ALLOWLIST.some((e) => e.file === relFile && e.line === line && e.kind === kind);
}

interface Violation {
  file: string;
  line: number;
  kind: "mojibake" | "bom";
  detail: string;
}

/** Every file `git` tracks, as repo-relative forward-slash paths -- the walk this guard scans. Driven by
 *  the index rather than `readdirSync` so gitignored output (build artifacts, `coverage/`, a stray local
 *  `.results/` from a prior `npm test` run) is never scanned, and so nothing needs a hand-maintained
 *  extension list to be included: whatever `git` tracks, this guard sees, matching exactly what a fresh
 *  CI checkout contains. `-z` NUL-delimits entries so a path containing a literal newline can't corrupt
 *  the split. */
function listTrackedFiles(): string[] {
  const out = execFileSync("git", ["ls-files", "-z"], { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  return out.split("\0").filter(Boolean);
}

/** True if `bytes`' first 4 KB contains a NUL byte -- the standard cheap binary/text heuristic (`git`
 *  itself uses the same signal for `-text`/diff-ability), used here instead of a file-extension list so
 *  a tracked icon/font/archive is skipped without needing to be named, while a tracked `.svg`/`.xml`/
 *  extensionless config file (attempt 1 silently missed all of these) is scanned like any other text. */
function looksBinary(bytes: Buffer): boolean {
  return bytes.subarray(0, 4096).includes(0);
}

function scanRepo(): Violation[] {
  const violations: Violation[] = [];
  for (const relFile of listTrackedFiles()) {
    if (EXCLUDE_PREFIXES.some((prefix) => relFile.startsWith(prefix))) continue;
    const abs = join(ROOT, ...relFile.split("/"));
    let bytes: Buffer;
    try {
      bytes = readFileSync(abs);
    } catch {
      continue; // e.g. a tracked symlink whose target isn't materialized on this checkout/OS
    }
    if (looksBinary(bytes)) continue;
    if (hasLeadingBom(bytes) && !isAllowlisted(relFile, 1, "bom")) {
      violations.push({ file: relFile, line: 1, kind: "bom", detail: "file opens with a UTF-8 BOM (EF BB BF)" });
    }
    const text = bytes.toString("utf8"); // Buffer#toString never throws -- lossy on genuinely non-UTF-8
    // content (substitutes U+FFFD). That is safe in one direction only: a lossy decode can never
    // FABRICATE a match (U+FFFD is not CP1252-producible), so it cannot turn a clean file into a false
    // failure. It can, however, LOSE one -- and that is a real residual gap, not a harmless detail.
    //
    // Two shapes of the very corruption this guard exists for pass it silently (CPE-1788):
    //   - A file rewritten as UTF-16 (PowerShell 5.1's > and Out-File default to UTF-16LE) has NUL
    //     bytes in its first 4 KB, so looksBinary skips it entirely, and hasLeadingBom only matches
    //     the UTF-8 BOM EF BB BF, never FF FE / FE FF.
    //   - A file rewritten as ANSI/CP1252 (PowerShell 5.1 Set-Content's default) is not valid UTF-8,
    //     so this decode yields U+FFFD and the scan reports zero hits on a genuinely corrupted file.
    // Both are the same root cause -- a PowerShell text round-trip -- in a different output encoding.
    // Named here rather than left implied as covered; tracked in CPE-1788.
    for (const offender of findMojibake(text)) {
      if (isAllowlisted(relFile, offender.line, "mojibake")) continue;
      violations.push({
        file: relFile,
        line: offender.line,
        kind: "mojibake",
        detail: `contains the mojibake signature "${offender.match}"`,
      });
    }
  }
  return violations;
}

describe("repo-wide mojibake guard (CPE-1771)", () => {
  it("scans essentially the whole repo (git-tracked, non-binary, minus the two documented exclusions)", () => {
    // A coarse sanity floor, not a maintained magic number: catches the walk silently collapsing back
    // down to a tiny hand-picked set (attempt 1's failure mode) without needing to update this number
    // every time a file is added or removed elsewhere in the tree.
    const tracked = listTrackedFiles().filter((f) => !EXCLUDE_PREFIXES.some((p) => f.startsWith(p)));
    expect(tracked.length).toBeGreaterThan(500);
  });

  it("has no un-allowlisted mojibake signature or UTF-8 BOM anywhere in the scanned tree", () => {
    const violations = scanRepo();
    if (violations.length > 0) {
      const lines = violations.map((v) => `  ${v.file}:${v.line} [${v.kind}] -- ${v.detail}`).join("\n");
      throw new Error(
        `Found ${violations.length} un-allowlisted mojibake/BOM violation(s) (CPE-1771):\n${lines}\n\n` +
          "If this is a genuine corruption, repair it byte-exact (iconv/sed/python/an editor tool -- " +
          "never a PowerShell text round-trip, which is the root cause of this bug class). If it's a " +
          "deliberate literal or a coincidental false positive, add it to ALLOWLIST in " +
          "src/lib/mojibakeGuard.test.ts with its exact kind (\"mojibake\" or \"bom\") and a reason.",
      );
    }
    expect(violations).toEqual([]);
  });

  it("every ALLOWLIST entry still points at the line it describes (no stale entries)", () => {
    // An entry whose `contains` substring is no longer on the recorded line means the file moved on
    // without the allowlist -- either the corruption was fixed (delete the entry; see CPE-1783) or lines
    // shifted (re-point it).
    const stale: string[] = [];
    for (const entry of ALLOWLIST) {
      const abs = join(ROOT, ...entry.file.split("/"));
      const text = readFileSync(abs, "utf8");
      const lineText = text.split(/\r?\n/)[entry.line - 1] ?? "";
      if (!lineText.includes(entry.contains)) stale.push(`${entry.file}:${entry.line}[${entry.kind}]`);
    }
    expect(stale, `stale ALLOWLIST entries (no longer match their recorded content): ${stale.join(", ")}`).toEqual([]);
  });
});
