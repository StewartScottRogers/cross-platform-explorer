import { describe, it, expect } from "vitest";
import { validateFileName, displaySafeName, displaySafePath } from "./filename";

// Built from decimal code points, not literal characters or `\uXXXX` string escapes, for the same
// reason `filename.ts` does it: a literal bidi-override character sitting in THIS test file would
// itself reorder the surrounding source/output. See filename.ts's `BIDI_FORMAT_CODES` doc comment.
const RLO = String.fromCharCode(0x202e); // RIGHT-TO-LEFT OVERRIDE — the reported spoof
const LRO = String.fromCharCode(0x202d);
const LRE = String.fromCharCode(0x202a);
const RLE = String.fromCharCode(0x202b);
const POP = String.fromCharCode(0x202c); // POP DIRECTIONAL FORMATTING — tagged [POP], not [PDF]
const LRI = String.fromCharCode(0x2066);
const RLI = String.fromCharCode(0x2067);
const FSI = String.fromCharCode(0x2068);
const PDI = String.fromCharCode(0x2069);
const LRM = String.fromCharCode(0x200e);
const RLM = String.fromCharCode(0x200f);
const ALM = String.fromCharCode(0x061c); // ARABIC LETTER MARK — the twelfth Bidi_Control code point

describe("validateFileName", () => {
  it("accepts ordinary names", () => {
    expect(validateFileName("report.txt")).toBeNull();
    expect(validateFileName("My Folder")).toBeNull();
    expect(validateFileName("data-2026_v2.tar.gz")).toBeNull();
  });

  it("rejects empty or whitespace-only names", () => {
    expect(validateFileName("")).toMatch(/empty/i);
    expect(validateFileName("   ")).toMatch(/empty/i);
  });

  it("rejects each illegal character", () => {
    for (const ch of ['<', '>', ':', '"', "/", "\\", "|", "?", "*"]) {
      expect(validateFileName(`bad${ch}name`)).toMatch(/can't contain/i);
    }
  });

  it("rejects control characters", () => {
    const withCtrl = `bad${String.fromCharCode(1)}name`;
    expect(validateFileName(withCtrl)).toMatch(/control character/i);
  });

  it("rejects a trailing space or period", () => {
    expect(validateFileName("name ")).toMatch(/space or a period/i);
    expect(validateFileName("name.")).toMatch(/space or a period/i);
  });

  it("rejects Windows reserved device names, any casing, with or without extension", () => {
    expect(validateFileName("CON")).toMatch(/reserved/i);
    expect(validateFileName("nul")).toMatch(/reserved/i);
    expect(validateFileName("Com1.txt")).toMatch(/reserved/i);
    expect(validateFileName("LPT9")).toMatch(/reserved/i);
  });

  it("does not treat reserved-name substrings as reserved", () => {
    expect(validateFileName("console.log")).toBeNull();
    expect(validateFileName("connection")).toBeNull();
  });
});

// CPE-1712 — `U+202E RIGHT-TO-LEFT OVERRIDE` is Unicode category Cf (format), not Cc (control), so it
// passes every `is_control()`-style guard untouched. A remote leaf `<RLO>gnp.txt` downloads intact and
// Windows Explorer displays it as `txt.png` — the bytes are honest, only the rendering lies. Every
// assertion below is on the STRING VALUE `displaySafeName` returns, i.e. exactly what ends up in the
// DOM text node the user reads — not on some intermediate flag — per Evidence Rule "assert what the
// user SEES".
describe("displaySafeName", () => {
  it("neutralizes the reported spoof: the true byte order is what's shown, not the reversed one", () => {
    const spoofed = `${RLO}gnp.txt`; // the ticket's own repro
    const shown = displaySafeName(spoofed);
    expect(shown).toBe("[RLO]gnp.txt");
    // The property that actually matters: the string a browser would paint contains no bidi/format
    // control character left to reorder it, so "what displaySafeName returns" and "what the user sees"
    // are the same string — and that string starts with the TRUE leaf, not the spoofed "txt.png".
    expect(shown.startsWith("[RLO]gnp")).toBe(true);
    expect(shown).not.toContain("txt.png");
  });

  it("enumerates the full 12-member Bidi_Control=Yes set, not only U+202E — the CPE-1709 lesson restated", () => {
    // Round 2 of the review caught the first cut at 11 of 12: U+061C ARABIC LETTER MARK, the direct
    // Arabic counterpart of RLM, was missing. All twelve are asserted here so that gap cannot recur
    // silently.
    const cases: Array<[string, string]> = [
      [ALM, "ALM"],
      [LRM, "LRM"], [RLM, "RLM"],
      [LRE, "LRE"], [RLE, "RLE"], [POP, "POP"], [LRO, "LRO"], [RLO, "RLO"],
      [LRI, "LRI"], [RLI, "RLI"], [FSI, "FSI"], [PDI, "PDI"],
    ];
    expect(cases).toHaveLength(12);
    for (const [ch, abbr] of cases) {
      expect(displaySafeName(`a${ch}b`)).toBe(`a[${abbr}]b`);
    }
  });

  it("tags U+202C as [POP], not [PDF] — a format-reference-looking tag beside [PDI] two entries away", () => {
    expect(displaySafeName(`report${POP}.doc`)).toBe("report[POP].doc");
  });

  it("leaves ordinary ASCII and percent-bearing names byte-identical", () => {
    for (const ok of ["report.txt", "50% off.txt", "report%2ffinal.txt", "résumé.pdf", "a.b.c"]) {
      expect(displaySafeName(ok)).toBe(ok);
    }
  });

  it("does NOT mangle a real Arabic filename — Arabic letters carry no bidi/format control chars", () => {
    // "مستند.pdf" — "document.pdf". Arabic letters are Unicode category Lo (letter), inherently
    // right-to-left by their own bidi class; they need no explicit LRE/RLE/RLM/etc. control character
    // to render correctly, so a name like this must survive completely untouched.
    const arabic = "مستند.pdf";
    expect(displaySafeName(arabic)).toBe(arabic);
  });

  it("does NOT mangle a real Hebrew filename — Hebrew letters carry no bidi/format control chars", () => {
    // "מסמך.txt" — "document.txt". Same reasoning as Arabic: Hebrew letters are their own bidi class,
    // no control character required.
    const hebrew = "מסמך.txt";
    expect(displaySafeName(hebrew)).toBe(hebrew);
  });

  it("a legitimate RLM some real RTL filenames carry before a Latin extension is shown, not silently dropped", () => {
    // A genuinely common real-world shape: a Hebrew name followed by an explicit RLM before the dot, so
    // the Latin extension keeps its own left-to-right order when rendered inside RTL text. This is the
    // exact case CPE-1712's "do not casually mangle real RTL filenames" caveat is about — the on-disk
    // name (crates/server/src/transfer.rs) is left untouched; this only governs what our own UI draws,
    // and it must not silently eat the mark or the surrounding text.
    const nameWithMark = `דוח${RLM}.pdf`;
    const shown = displaySafeName(nameWithMark);
    expect(shown).toBe("דוח[RLM].pdf");
    expect(shown).toContain("דוח");
    expect(shown).toContain(".pdf");
  });

  it("a legitimate ALM some real Arabic filenames carry before a Latin extension is shown, not dropped", () => {
    // "تقرير" ("report") followed by an explicit ALM before the dot — the Arabic mirror of the Hebrew
    // RLM case above.
    const nameWithMark = `تقرير${ALM}.pdf`;
    const shown = displaySafeName(nameWithMark);
    expect(shown).toBe("تقرير[ALM].pdf");
    expect(shown).toContain("تقرير");
    expect(shown).toContain(".pdf");
  });

  it("a name containing several bidi/format characters gets every one flagged, in place", () => {
    const messy = `${LRO}a${RLM}b${PDI}c`;
    expect(displaySafeName(messy)).toBe("[LRO]a[RLM]b[PDI]c");
  });
});

// CPE-1712 review round 2: `displaySafePath` is the named entry point for a full path (breadcrumbs,
// "Path" rows, tooltips, TrashView's original-location column). It produces byte-identical output to
// calling `displaySafeName` on the whole string — the escape is a stateless, position-independent
// substitution, so segmenting first changes nothing about the RESULT. The reason it exists as its own
// name is discoverability: round 1 of this review fixed a name span and left a PATH span sitting right
// next to it raw in four different components, because nothing at the call site signalled "this is
// also path-shaped and also needs escaping". See its doc comment in filename.ts for the full account.
describe("displaySafePath", () => {
  it("escapes an override anywhere in the path, leaving separators untouched", () => {
    const p = `C:\\Users\\alice\\${RLO}gnp.txt`;
    const shown = displaySafePath(p);
    expect(shown).toBe("C:\\Users\\alice\\[RLO]gnp.txt");
    expect(shown).not.toContain("txt.png");
  });

  it("escapes an override sitting in a DIRECTORY segment too, not just the leaf", () => {
    const p = `/home/${RLO}gnp.txt/notes.md`;
    const shown = displaySafePath(p);
    expect(shown).toBe("/home/[RLO]gnp.txt/notes.md");
    expect(shown).not.toContain("notes.md/gnp.txt");
  });

  it("produces byte-identical output to displaySafeName on the same string (no algorithmic difference)", () => {
    for (const p of [`C:\\Users\\${RLO}alice\\notes.md`, `/home/alice/${LRO}report.pdf`, "ordinary/path.txt"]) {
      expect(displaySafePath(p)).toBe(displaySafeName(p));
    }
  });

  it("handles a mix of `/` and `\\` separators (a Windows UNC-ish or mixed remote path)", () => {
    const p = `\\\\server\\share/${RLO}gnp.txt`;
    expect(displaySafePath(p)).toBe("\\\\server\\share/[RLO]gnp.txt");
  });

  it("leaves an ordinary path, and a real Arabic/Hebrew path, byte-identical", () => {
    for (const ok of [
      "C:\\Users\\alice\\report.txt",
      "/home/alice/photos/vacation.jpg",
      "/home/alice/מסמך.txt",
      "C:\\Users\\alice\\مستند.pdf",
    ]) {
      expect(displaySafePath(ok)).toBe(ok);
    }
  });
});
