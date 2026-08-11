import { describe, it, expect } from "vitest";
import {
  BINARY_TABLE_ROW_CAP,
  capRows,
  classifyBinaryError,
  decodeAssemblyFlags,
  cultureLabel,
  hexOrDash,
  formatLabel,
  hexAddress,
} from "./binaryInspector";

describe("capRows (CPE-1597 — big binaries must not stall the pane)", () => {
  it("does not cap a short list", () => {
    const r = capRows([1, 2, 3], 10);
    expect(r).toEqual({ rows: [1, 2, 3], total: 3, capped: false });
  });

  it("caps a list over the limit and reports the real total honestly", () => {
    const all = Array.from({ length: 1693 }, (_, i) => i); // ~kernel32.dll export count
    const r = capRows(all, BINARY_TABLE_ROW_CAP);
    expect(r.rows.length).toBe(BINARY_TABLE_ROW_CAP);
    expect(r.total).toBe(1693);
    expect(r.capped).toBe(true);
  });

  it("a list exactly at the cap is not reported as capped", () => {
    const all = Array.from({ length: 1000 }, (_, i) => i);
    const r = capRows(all, 1000);
    expect(r.capped).toBe(false);
    expect(r.rows.length).toBe(1000);
  });

  it("defaults to BINARY_TABLE_ROW_CAP when no cap is given", () => {
    const all = Array.from({ length: BINARY_TABLE_ROW_CAP + 1 }, (_, i) => i);
    expect(capRows(all).capped).toBe(true);
  });

  it("an empty list is never capped", () => {
    expect(capRows([]).capped).toBe(false);
  });
});

describe("classifyBinaryError (CPE-1597 — calm error states, never a raw dump)", () => {
  it("recognizes the backend's oversized-file message", () => {
    expect(classifyBinaryError("File is too large to preview (134217728 bytes; limit 134217728).")).toBe(
      "too-large",
    );
  });

  it("recognizes Windows and Unix permission errors", () => {
    expect(classifyBinaryError("Access is denied. (os error 5)")).toBe("permission");
    expect(classifyBinaryError("Permission denied (os error 13)")).toBe("permission");
  });

  it("falls back to 'unrecognized' for a goblin parse failure / anything else", () => {
    expect(classifyBinaryError("Malformed entity: cannot parse ELF header")).toBe("unrecognized");
    expect(classifyBinaryError("unexpected error")).toBe("unrecognized");
    expect(classifyBinaryError("")).toBe("unrecognized");
  });

  it("is case-insensitive", () => {
    expect(classifyBinaryError("ACCESS IS DENIED. (OS ERROR 5)")).toBe("permission");
  });
});

describe("decodeAssemblyFlags (CPE-1615 — recognized ECMA-335 AssemblyFlags bits, as pill labels)", () => {
  it("decodes a single known bit", () => {
    expect(decodeAssemblyFlags(0x0001)).toEqual(["PublicKey"]);
    expect(decodeAssemblyFlags(0x0100)).toEqual(["Retargetable"]);
  });

  it("decodes multiple set bits, in declaration order", () => {
    expect(decodeAssemblyFlags(0x0001 | 0x0100)).toEqual(["PublicKey", "Retargetable"]);
  });

  it("a zero value decodes to no flags", () => {
    expect(decodeAssemblyFlags(0)).toEqual([]);
  });

  it("an unrecognized bit contributes nothing (never throws, never a garbage label)", () => {
    expect(decodeAssemblyFlags(0x8000)).toEqual([]);
    // A recognized bit alongside unrecognized ones still surfaces cleanly.
    expect(decodeAssemblyFlags(0x0001 | 0x8000)).toEqual(["PublicKey"]);
  });
});

describe("cultureLabel (CPE-1615 — the neutral CLR culture is null/empty, per DotnetAssemblyIdentity/Ref's own doc comments)", () => {
  it("labels null as 'neutral'", () => {
    expect(cultureLabel(null)).toBe("neutral");
  });

  it("labels an empty string as 'neutral' too", () => {
    expect(cultureLabel("")).toBe("neutral");
  });

  it("passes through a real culture string unchanged", () => {
    expect(cultureLabel("en-US")).toBe("en-US");
  });
});

describe("hexOrDash (CPE-1615 — public_key/public_key_token: null means no key/token present)", () => {
  it("renders null as an em dash", () => {
    expect(hexOrDash(null)).toBe("—");
  });

  it("renders an empty string as an em dash", () => {
    expect(hexOrDash("")).toBe("—");
  });

  it("passes through a real hex blob unchanged", () => {
    expect(hexOrDash("b77a5c561934e089")).toBe("b77a5c561934e089");
  });
});

describe("formatLabel", () => {
  it("labels every BinaryFormat", () => {
    expect(formatLabel("Pe")).toMatch(/PE/);
    expect(formatLabel("Elf")).toMatch(/ELF/);
    expect(formatLabel("MachO")).toMatch(/Mach-O/);
  });
});

describe("hexAddress", () => {
  it("formats a null address as an em dash, never 0x0", () => {
    expect(hexAddress(null)).toBe("—");
  });

  it("zero-pads to a readable width", () => {
    expect(hexAddress(0x401000)).toBe("0x00401000");
    expect(hexAddress(0x1000)).toBe("0x1000");
    expect(hexAddress(0)).toBe("0x0000");
  });

  it("does not truncate a wide address (pads up to the next tier, never cuts digits)", () => {
    expect(hexAddress(0x7ffabcdef123)).toBe("0x00007ffabcdef123");
  });
});
