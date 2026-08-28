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
//
// CPE-1788 extends the guard to the same PowerShell round-trip landing in UTF-16 or ANSI instead of UTF-8
// (see `mojibakeGuard.ts`'s header for the full root-cause account) and adds a fourth kind of test:
//   - Unit tests of `detectUtf16Bom`/`findFirstInvalidUtf8Byte` against hand-built byte arrays (these are
//     BYTE-level checks, not text-level, so there is no lossy-decode round trip to reuse the way
//     `simulateCp1252Corruption` lets the text-level tests avoid hand-typed literals).
//   - `simulatePowerShellAnsiRewrite`/`simulatePowerShellUtf16Rewrite` (in `mojibakeGuard.ts`), the
//     byte-level equivalent of `simulateCp1252Corruption`: explicit, documented, provably-inverse-table
//     reproductions of what `Set-Content`'s ANSI default and `Out-File`/`>`'s UTF-16LE default actually
//     write, cross-checked independently against python's own `str.encode("cp1252")`/`"utf-16-le"` during
//     development (see this ticket's PR description) rather than trusted on assertion alone.
//   - A red-proof block that stages genuinely corrupted fixture FILES on disk (a real `fs.writeFileSync`,
//     never PowerShell, using the two simulator functions above) and reads them back with the exact same
//     `fs.readFileSync` + `scanOneFile` code path the tree-wide scan below uses -- not a re-implementation
//     of the check for the test's benefit. Fixtures live in an untracked OS temp directory, never in the
//     repo tree, so they can never reach `scanRepo()`'s own `git ls-files` walk.
import { describe, it, expect } from "vitest";
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  findMojibake,
  hasLeadingBom,
  simulateCp1252Corruption,
  detectUtf16Bom,
  findFirstInvalidUtf8Byte,
  simulatePowerShellAnsiRewrite,
  simulatePowerShellUtf16Rewrite,
} from "./mojibakeGuard";

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

describe("detectUtf16Bom (CPE-1788)", () => {
  it("detects a UTF-16LE BOM (FF FE) -- PowerShell 5.1 Out-File/> default", () => {
    expect(detectUtf16Bom(new Uint8Array([0xff, 0xfe, 0x61, 0x00]))).toBe("LE");
  });

  it("detects a UTF-16BE BOM (FE FF)", () => {
    expect(detectUtf16Bom(new Uint8Array([0xfe, 0xff, 0x00, 0x61]))).toBe("BE");
  });

  it("does not false-positive on ordinary UTF-8 content, including a leading UTF-8 BOM", () => {
    expect(detectUtf16Bom(new Uint8Array([0x7b, 0x0a, 0x20]))).toBeNull();
    expect(detectUtf16Bom(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b]))).toBeNull();
  });

  it("does not throw on a too-short buffer", () => {
    expect(detectUtf16Bom(new Uint8Array([0xff]))).toBeNull();
    expect(detectUtf16Bom(new Uint8Array([]))).toBeNull();
  });
});

describe("findFirstInvalidUtf8Byte (CPE-1788)", () => {
  it("returns null for plain ASCII", () => {
    expect(findFirstInvalidUtf8Byte(new TextEncoder().encode("hello world"))).toBeNull();
  });

  it("returns null for well-formed multi-byte UTF-8 (2-, 3-, and 4-byte sequences)", () => {
    expect(findFirstInvalidUtf8Byte(new TextEncoder().encode("café"))).toBeNull(); // 2-byte (é)
    expect(findFirstInvalidUtf8Byte(new TextEncoder().encode("日本語"))).toBeNull(); // 3-byte (CJK)
    expect(findFirstInvalidUtf8Byte(new TextEncoder().encode("\u{1f389}"))).toBeNull(); // 4-byte (emoji)
  });

  it("catches a lone CP1252-rewrite byte: an em dash re-encoded as CP1252 0x97 is not valid UTF-8 on its own", () => {
    // This is the exact shape Set-Content's ANSI default produces: a real em dash (U+2014, correctly
    // read) re-encoded as the single CP1252 byte 0x97, which -- read back as UTF-8 -- is a bare
    // continuation-range byte with no lead byte in front of it.
    const bytes = new TextEncoder().encode("before ");
    const withBadByte = Uint8Array.from([...bytes, 0x97, ...new TextEncoder().encode(" after")]);
    expect(findFirstInvalidUtf8Byte(withBadByte)).toBe(bytes.length);
  });

  it("catches a truncated multi-byte sequence at EOF", () => {
    // "a" (1 byte) + the first two of a 3-byte sequence (e.g. E2 82 AC, the euro sign), with the third
    // byte missing entirely.
    expect(findFirstInvalidUtf8Byte(new Uint8Array([0x61, 0xe2, 0x82]))).toBe(1);
  });

  it("catches an overlong encoding (U+0000 re-encoded as a 3-byte sequence instead of 1 byte)", () => {
    expect(findFirstInvalidUtf8Byte(new Uint8Array([0xe0, 0x80, 0x80]))).toBe(0);
  });

  it("catches an encoded UTF-16 surrogate (never legal in UTF-8, only in UTF-16)", () => {
    expect(findFirstInvalidUtf8Byte(new Uint8Array([0xed, 0xa0, 0x80]))).toBe(0); // encodes U+D800
  });

  it("catches a code point past U+10FFFF", () => {
    expect(findFirstInvalidUtf8Byte(new Uint8Array([0xf4, 0x90, 0x80, 0x80]))).toBe(0); // encodes U+110000
  });

  it("catches a bare continuation byte with nothing in front of it", () => {
    expect(findFirstInvalidUtf8Byte(new Uint8Array([0x80]))).toBe(0);
  });
});

describe("simulatePowerShellAnsiRewrite / simulatePowerShellUtf16Rewrite (CPE-1788 fixture helpers)", () => {
  it("simulatePowerShellUtf16Rewrite produces bytes detectUtf16Bom flags as UTF-16LE", () => {
    const bytes = simulatePowerShellUtf16Rewrite("ordinary text");
    expect(detectUtf16Bom(bytes)).toBe("LE");
  });

  it("simulatePowerShellUtf16Rewrite round-trips content correctly (it is not itself corrupting the text)", () => {
    const bytes = simulatePowerShellUtf16Rewrite("hello");
    expect(Buffer.from(bytes.slice(2)).toString("utf16le")).toBe("hello");
  });

  it("simulatePowerShellAnsiRewrite produces bytes findFirstInvalidUtf8Byte flags as not valid UTF-8", () => {
    const bytes = simulatePowerShellAnsiRewrite("a normal dependency — it pulls only serde.");
    expect(findFirstInvalidUtf8Byte(bytes)).not.toBeNull();
  });

  it("simulatePowerShellAnsiRewrite round-trips plain ASCII unchanged (no corruption when nothing is CP1252-special)", () => {
    const bytes = simulatePowerShellAnsiRewrite("plain ascii, no surprises");
    expect(findFirstInvalidUtf8Byte(bytes)).toBeNull();
    expect(Buffer.from(bytes).toString("ascii")).toBe("plain ascii, no surprises");
  });

  it("simulatePowerShellAnsiRewrite throws rather than fabricating a byte for a character CP1252 cannot encode", () => {
    expect(() => simulatePowerShellAnsiRewrite("中文")).toThrow(/no CP1252 encoding/);
  });
});

// ---------------------------------------------------------------------------------------------------
// CPE-1788 red-proof: genuinely corrupted fixture FILES, staged on disk and read back exactly the way
// scanRepo() reads the real tree (fs.readFileSync), not just in-memory byte arrays. The bytes are built
// with simulatePowerShellUtf16Rewrite/simulatePowerShellAnsiRewrite (explicit, byte-exact construction --
// see those functions' doc comments) and written with node's fs.writeFileSync, never PowerShell, which is
// the very tool that produces this corruption in the first place and has broken a release here before.
// Fixtures live in a temp directory outside the repo tree (mkdtempSync under the OS temp dir, removed in
// `finally`), never git-tracked, so they can never be picked up by scanRepo()'s own git-ls-files walk.
// ---------------------------------------------------------------------------------------------------

describe("CPE-1788 red-proof: real fixture files on disk", () => {
  it("catches a UTF-16LE-rewritten file (Out-File/> shape) as kind utf16, at line 1 byte 0", () => {
    const dir = mkdtempSync(join(tmpdir(), "mojibake-guard-utf16-"));
    try {
      const abs = join(dir, "rewritten.txt");
      writeFileSync(abs, simulatePowerShellUtf16Rewrite("clean ASCII content\nsecond line\n"));
      const violations = scanOneFile("fixtures/rewritten.txt", readFileSync(abs));
      expect(violations).toHaveLength(1);
      expect(violations[0].kind).toBe("utf16");
      expect(violations[0].line).toBe(1);
      expect(violations[0].detail).toMatch(/UTF-16LE BOM \(FF FE\).*at byte 0/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("catches an ANSI/CP1252-rewritten file (Set-Content default shape) as kind not-utf8, with the correct line", () => {
    const dir = mkdtempSync(join(tmpdir(), "mojibake-guard-ansi-"));
    try {
      const abs = join(dir, "rewritten.txt");
      // The em dash lands on line 2, so the guard's reported line must be 2, not 1.
      writeFileSync(abs, simulatePowerShellAnsiRewrite("clean first line\na normal dependency — end.\n"));
      const violations = scanOneFile("fixtures/rewritten.txt", readFileSync(abs));
      expect(violations).toHaveLength(1);
      expect(violations[0].kind).toBe("not-utf8");
      expect(violations[0].line).toBe(2);
      expect(violations[0].detail).toMatch(/invalid byte 0x97 at offset \d+/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("does NOT flag a clean UTF-8 file written the same way (no false positive from the fixture machinery itself)", () => {
    const dir = mkdtempSync(join(tmpdir(), "mojibake-guard-clean-"));
    try {
      const abs = join(dir, "clean.txt");
      writeFileSync(abs, Buffer.from("plain, correctly-encoded UTF-8 text with no surprises.\n", "utf8"));
      expect(scanOneFile("fixtures/clean.txt", readFileSync(abs))).toEqual([]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
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

/** CPE-1788 adds "utf16" (a UTF-16 BOM where a repo source file should be UTF-8) and "not-utf8" (bytes
 *  that fail UTF-8 well-formedness outright, PowerShell `Set-Content`'s ANSI/CP1252 default) alongside
 *  CPE-1771's original "mojibake"/"bom" split -- same reasoning as that split: a future entry for one kind
 *  must never silently swallow a different kind of violation landing on the same (file, line). */
type ViolationKind = "mojibake" | "bom" | "utf16" | "not-utf8";

interface AllowlistEntry {
  file: string; // repo-relative, forward-slash
  line: number;
  kind: ViolationKind;
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
    // 5320 -> 5330: CPE-1775 inserted `notice.archiveSkipped{One,Many}` into each of the 12 locale
    // blocks (2 lines apiece), so every entry below this file's Spanish block moved down. The `contains`
    // check is what makes a re-number safe: it re-anchors on the text, not on my arithmetic.
    // 5330 -> 5342: CPE-1803 added one `trash.degraded` line to several locale blocks above this line
    // (including this line's own Portuguese block, ahead of "NÃO" itself). Re-anchored on `contains`
    // below, same as the note above -- the exact arithmetic doesn't matter, only that this stays in sync.
    // 5342 -> 5357: CPE-1804 added `trash.skipped{One,Many}` to every locale block (2 lines apiece, plus
    // a 5-line explanatory comment in the English one), so 15 lines landed above this one. Same story.
    // 5357 -> 5366: CPE-1816 added one `trash.stillLoading` line to each of the 5 locale blocks above
    // this one (en, es, de, fr, it -- Portuguese's own block gets the same line too, but further down,
    // after this line, so it doesn't shift it). English's carries a 4-line explanatory comment, so that's
    // 9 lines total (en's 5 + es/de/fr/it's 1 apiece), not 9 blocks. Same story, re-anchored on `contains`.
    // 5366 -> 5379: CPE-1827 added `trash.moreActions` + `trash.docs` (2 lines apiece) to en/es/de/fr/it's
    // locale blocks, all of which sit above this Portuguese line -- Portuguese's own pair of new lines
    // lands after `trash.refresh`, further down than this one, so it doesn't shift it (same shape as the
    // CPE-1816 note above). Same story, re-anchored on `contains`.
    // 5379 -> 5384: CPE-1879 added one `notice.autoBackupFirstFailure` line to each of en/es/de/fr/it's
    // locale blocks (1 line apiece), all of which sit above this Portuguese line, so 5 lines landed above
    // it. Same story, re-anchored on `contains`.
    // 5384 -> 5394: CPE-1935 added `notice.archiveFailed{One,Many}` to every locale (2 lines apiece).
    // en/es/de/fr/it's pairs sit above this Portuguese line; Portuguese's own pair lands in its own
    // block, further down than this line, so only those 10 shift it. Same story, `contains` unchanged.
    line: 5394,
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

function isAllowlisted(relFile: string, line: number, kind: ViolationKind): boolean {
  return ALLOWLIST.some((e) => e.file === relFile && e.line === line && e.kind === kind);
}

interface Violation {
  file: string;
  line: number;
  kind: ViolationKind;
  detail: string;
}

/** 1-based line number containing byte offset `offset` in `bytes` -- counts `\n` (0x0A) bytes before it,
 *  the same convention `findMojibake` uses for text, but at the raw byte level (needed here because the
 *  file's content around `offset` is, by construction, not valid UTF-8 text to search line-by-line). */
function lineAtByteOffset(bytes: Uint8Array, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset && i < bytes.length; i++) {
    if (bytes[i] === 0x0a) line++;
  }
  return line;
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

/** Runs every check this guard has (CPE-1771's `mojibake`/`bom`, CPE-1788's `utf16`/`not-utf8`) against
 *  one already-read file and returns whatever it finds, unfiltered by the allowlist -- callers decide
 *  whether to consult `isAllowlisted`. Extracted out of `scanRepo`'s loop body so the exact production
 *  check sequence can also be exercised directly against on-disk fixture files (see the "CPE-1788 red-proof"
 *  describe block below), not re-implemented or approximated for the test.
 *
 * Order matters and is deliberate:
 *   1. UTF-16 BOM, unconditionally, BEFORE the binary skip -- per CPE-1788, this is the check that used
 *      to never run at all, because the same NULs a UTF-16 rewrite produces are exactly what `looksBinary`
 *      treats as "this isn't text, skip it". If found, this is the one loud fact worth reporting for this
 *      file: the rest of its content is UTF-16 code units, not UTF-8 bytes, so decoding it as UTF-8 for
 *      the checks below would scan garbage and could only add noise, never signal. Stop here.
 *   2. `looksBinary` -- unchanged from CPE-1771: a genuine binary (icon, font, archive) has NULs
 *      throughout for reasons that have nothing to do with UTF-16, and was never meant to be scanned.
 *   3. UTF-8 BOM -- unchanged from CPE-1771.
 *   4. Byte-level UTF-8 well-formedness (CPE-1788's `not-utf8`) -- catches `Set-Content`'s ANSI/CP1252
 *      default, which a lossy `bytes.toString("utf8")` decode alone reports as clean (see the header note
 *      above). If the bytes are not valid UTF-8 at all, the lossy text below is U+FFFD-riddled garbage;
 *      scanning IT for mojibake would add noise, not signal, for the same reason step 1 stops early. Stop
 *      here too.
 *   5. `findMojibake` over the lossy-decoded text -- unchanged from CPE-1771, and only reached once steps
 *      1-4 have confirmed the bytes are actually valid UTF-8 to begin with. */
function scanOneFile(relFile: string, bytes: Buffer): Violation[] {
  const violations: Violation[] = [];

  const utf16 = detectUtf16Bom(bytes);
  if (utf16 !== null) {
    const bomHex = utf16 === "LE" ? "FF FE" : "FE FF";
    violations.push({
      file: relFile,
      line: 1,
      kind: "utf16",
      detail: `file opens with a UTF-16${utf16} BOM (${bomHex}) -- this file should be UTF-8, not UTF-16, at byte 0`,
    });
    return violations; // rest of the bytes are UTF-16 code units, not UTF-8 -- nothing below applies
  }

  if (looksBinary(bytes)) return violations;

  if (hasLeadingBom(bytes)) {
    violations.push({ file: relFile, line: 1, kind: "bom", detail: "file opens with a UTF-8 BOM (EF BB BF)" });
  }

  const badByteOffset = findFirstInvalidUtf8Byte(bytes);
  if (badByteOffset !== null) {
    const line = lineAtByteOffset(bytes, badByteOffset);
    const byteHex = bytes[badByteOffset].toString(16).padStart(2, "0");
    violations.push({
      file: relFile,
      line,
      kind: "not-utf8",
      detail:
        `file is not valid UTF-8 -- invalid byte 0x${byteHex} at offset ${badByteOffset}, likely a ` +
        "PowerShell Set-Content ANSI/CP1252 rewrite (a multi-byte UTF-8 character re-encoded as a single " +
        "CP1252 byte that isn't, on its own, a valid UTF-8 sequence)",
    });
    return violations; // the lossy decode below is U+FFFD-riddled garbage for a file already known bad
  }

  const text = bytes.toString("utf8"); // safe here: findFirstInvalidUtf8Byte just confirmed bytes IS
  // valid UTF-8, so this decode is lossless and exact, not the lossy substitution CPE-1788 was filed about.
  for (const offender of findMojibake(text)) {
    violations.push({
      file: relFile,
      line: offender.line,
      kind: "mojibake",
      detail: `contains the mojibake signature "${offender.match}"`,
    });
  }

  return violations;
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
    for (const v of scanOneFile(relFile, bytes)) {
      if (!isAllowlisted(v.file, v.line, v.kind)) violations.push(v);
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

  it("has no un-allowlisted mojibake/BOM/UTF-16/not-UTF-8 violation anywhere in the scanned tree", () => {
    const violations = scanRepo();
    if (violations.length > 0) {
      const lines = violations.map((v) => `  ${v.file}:${v.line} [${v.kind}] -- ${v.detail}`).join("\n");
      throw new Error(
        `Found ${violations.length} un-allowlisted violation(s) (CPE-1771/CPE-1788):\n${lines}\n\n` +
          "If this is a genuine corruption, repair it byte-exact (iconv/sed/python/an editor tool -- " +
          "never a PowerShell text round-trip, which is the root cause of this bug class). If it's a " +
          "deliberate literal or a coincidental false positive, add it to ALLOWLIST in " +
          'src/lib/mojibakeGuard.test.ts with its exact kind ("mojibake", "bom", "utf16", or "not-utf8") ' +
          "and a reason.",
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
