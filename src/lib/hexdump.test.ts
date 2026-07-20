import { describe, it, expect } from "vitest";
import { hexRows, detectSignature } from "./hexdump";

const bytes = (...n: number[]) => Uint8Array.from(n);

describe("hexRows (CPE-770)", () => {
  it("returns no rows for empty input", () => {
    expect(hexRows(bytes())).toEqual([]);
  });

  it("formats a single byte with a padded, aligned row", () => {
    const [r] = hexRows(bytes(0x41), 0, 4);
    expect(r.offset).toBe("00000000");
    // one real cell "41" + three blank cells, space-joined → full-row width 4*2 + 3 = 11
    expect(r.hex).toBe("41" + " ".repeat(9));
    expect(r.hex.length).toBe(11);
    expect(r.hex.trimEnd()).toBe("41");
    expect(r.ascii).toBe("A");
    expect(r.bytes).toBe(1);
  });

  it("maps printable vs non-printable in the ascii gutter", () => {
    const [r] = hexRows(bytes(0x00, 0x41, 0x7f, 0x7e, 0x1f, 0x20), 0, 16);
    // 0x00 non-print, A printable, 0x7f non-print (DEL), 0x7e printable (~), 0x1f non-print, 0x20 space
    expect(r.ascii).toBe(".A.~. ");
    expect(r.hex.startsWith("00 41 7F 7E 1F 20")).toBe(true);
  });

  it("splits into full rows + a short final row, honoring baseOffset", () => {
    const data = Uint8Array.from({ length: 18 }, (_, i) => i);
    const rows = hexRows(data, 0x10, 16);
    expect(rows).toHaveLength(2);
    expect(rows[0].offset).toBe("00000010");
    expect(rows[0].bytes).toBe(16);
    expect(rows[1].offset).toBe("00000020"); // 0x10 + 16
    expect(rows[1].bytes).toBe(2);
    // both rows' hex strings are the same width (short row padded)
    expect(rows[1].hex.length).toBe(rows[0].hex.length);
  });

  it("guards a bogus bytesPerRow", () => {
    expect(hexRows(bytes(1, 2, 3), 0, 0)).toHaveLength(3); // falls back to 1 per row
  });
});

describe("detectSignature (CPE-770)", () => {
  const cases: Array<[string, number[], string]> = [
    ["PNG", [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], "png"],
    ["JPEG", [0xff, 0xd8, 0xff, 0xe0], "jpg"],
    ["GIF", [0x47, 0x49, 0x46, 0x38, 0x39, 0x61], "gif"],
    ["PDF", [0x25, 0x50, 0x44, 0x46, 0x2d], "pdf"],
    ["ZIP", [0x50, 0x4b, 0x03, 0x04], "zip"],
    ["GZIP", [0x1f, 0x8b, 0x08], "gz"],
    ["ELF", [0x7f, 0x45, 0x4c, 0x46], "elf"],
    ["WASM", [0x00, 0x61, 0x73, 0x6d, 0x01], "wasm"],
    ["Java class", [0xca, 0xfe, 0xba, 0xbe], "class"],
    ["PE", [0x4d, 0x5a, 0x90, 0x00], "exe"],
  ];
  for (const [name, sig, ext] of cases) {
    it(`identifies ${name}`, () => {
      expect(detectSignature(bytes(...sig))?.ext).toBe(ext);
    });
  }

  it("identifies WAV only with the WAVE tag at offset 8", () => {
    const wav = bytes(0x52, 0x49, 0x46, 0x46, 1, 2, 3, 4, 0x57, 0x41, 0x56, 0x45);
    expect(detectSignature(wav)?.ext).toBe("wav");
    // RIFF without WAVE (e.g. AVI) is not claimed as WAV
    const notWav = bytes(0x52, 0x49, 0x46, 0x46, 1, 2, 3, 4, 0x41, 0x56, 0x49, 0x20);
    expect(detectSignature(notWav)).toBeNull();
  });

  it("returns null for unknown or too-short input", () => {
    expect(detectSignature(bytes(1, 2, 3, 4))).toBeNull();
    expect(detectSignature(bytes(0x89))).toBeNull(); // partial PNG magic
    expect(detectSignature(bytes())).toBeNull();
  });
});
