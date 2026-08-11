import { describe, it, expect } from "vitest";
import {
  BINARY_TABLE_ROW_CAP,
  capRows,
  classifyBinaryError,
  managedDotNetConfidence,
  emptyImportExportIsNormalFor,
  formatLabel,
  hexAddress,
} from "./binaryInspector";
import type { BinaryExport, BinaryImport } from "../bindings.gen";

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

describe("managedDotNetConfidence (CPE-1597 — never present CIL-as-x86 nonsense as fact)", () => {
  const imp = (library: string | null, name = "Fn"): BinaryImport => ({ name, library });
  const exp = (name: string): BinaryExport => ({ name, address: 0x1000 });

  it("'confirmed': a PE that imports mscoree.dll", () => {
    expect(
      managedDotNetConfidence(
        { format: "Pe", imports: [imp("KERNEL32.dll"), imp("mscoree.dll")], exports: [] },
        "dll",
      ),
    ).toBe("confirmed");
  });

  it("is case-insensitive on the library name", () => {
    expect(managedDotNetConfidence({ format: "Pe", imports: [imp("MSCOREE.DLL")], exports: [] }, "dll")).toBe(
      "confirmed",
    );
  });

  it("'confirmed': a PE exporting the CLR loader entry points even with no matching import", () => {
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [exp("_CorExeMain")] }, "dll")).toBe(
      "confirmed",
    );
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [exp("_CorDllMain")] }, "dll")).toBe(
      "confirmed",
    );
  });

  it("'confirmed' applies to ANY extension — real CLR evidence, not format-dependent", () => {
    // A code-review finding on the first version of this heuristic: don't let a per-format carve-out
    // (added for the "possible" tier below) accidentally weaken the strong signal too.
    expect(managedDotNetConfidence({ format: "Pe", imports: [imp("mscoree.dll")], exports: [] }, "efi")).toBe(
      "confirmed",
    );
    expect(managedDotNetConfidence({ format: "Pe", imports: [imp("mscoree.dll")], exports: [] }, "sys")).toBe(
      "confirmed",
    );
  });

  it("'possible': a PE with zero imports AND zero exports — the real mscorlib.dll shape", () => {
    // Verified against the real 64-bit .NET Framework mscorlib.dll during this ticket's manual testing:
    // it reports imports:0, exports:0 (a modern x64/AnyCPU pure-IL assembly loads straight off its CLR
    // header, with no legacy mscoree.dll import-table stub at all) — the "confirmed" signal alone misses
    // it entirely, which is exactly why this weaker, hedged signal exists.
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [] }, "dll")).toBe("possible");
  });

  // ---- code-review fix: .efi/.sys must NOT get the "possible" tier for an empty table ----
  it("'none' — NOT 'possible' — for a .efi with zero imports AND zero exports (UEFI: normal by design, not a signal)", () => {
    // UEFI drivers/boot apps reach firmware services via EFI_SYSTEM_TABLE, never a PE import table — an
    // empty import/export table is the NORM for this format, not evidence of anything. Flagging it as
    // "possibly managed" was a real bug: every legitimate .efi got a wrong caveat and had its real,
    // valid disassembly hidden behind an unnecessary extra click.
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [] }, "efi")).toBe("none");
  });

  it("'none' — NOT 'possible' — for a .sys with zero imports AND zero exports (kernel drivers can be near-empty too)", () => {
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [] }, "sys")).toBe("none");
  });

  it("extension matching for the .efi/.sys carve-out is case-insensitive", () => {
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [] }, "EFI")).toBe("none");
    expect(managedDotNetConfidence({ format: "Pe", imports: [], exports: [] }, "SYS")).toBe("none");
  });

  it("'none': an ordinary native PE with real imports and exports", () => {
    expect(
      managedDotNetConfidence(
        {
          format: "Pe",
          imports: [imp("KERNEL32.dll"), imp("USER32.dll"), imp(null)],
          exports: [exp("SomeExport")],
        },
        "exe",
      ),
    ).toBe("none");
  });

  it("'none': a native PE with imports but no exports (the common EXE shape) is not flagged", () => {
    expect(managedDotNetConfidence({ format: "Pe", imports: [imp("KERNEL32.dll")], exports: [] }, "exe")).toBe(
      "none",
    );
  });

  it("never flags a non-PE format, even one that happens to import something named mscoree.dll", () => {
    expect(managedDotNetConfidence({ format: "Elf", imports: [imp("mscoree.dll")], exports: [] }, "so")).toBe(
      "none",
    );
    expect(managedDotNetConfidence({ format: "MachO", imports: [imp("mscoree.dll")], exports: [] }, "dylib")).toBe(
      "none",
    );
    // Even the zero/zero shape never fires outside PE — Elf/MachO's own linking conventions differ, and
    // this heuristic is scoped to what's known about CLR-hosted PE images.
    expect(managedDotNetConfidence({ format: "Elf", imports: [], exports: [] }, "so")).toBe("none");
    expect(managedDotNetConfidence({ format: "MachO", imports: [], exports: [] }, "dylib")).toBe("none");
  });
});

describe("emptyImportExportIsNormalFor (CPE-1597 code-review fix)", () => {
  it("flags .efi and .sys — formats where an empty import/export table is normal by design", () => {
    expect(emptyImportExportIsNormalFor("efi")).toBe(true);
    expect(emptyImportExportIsNormalFor("sys")).toBe(true);
  });

  it("is case-insensitive", () => {
    expect(emptyImportExportIsNormalFor("EFI")).toBe(true);
  });

  it("does not flag exe/dll/so/dylib — an empty table there is genuinely worth a hedged caveat", () => {
    for (const ext of ["exe", "dll", "so", "dylib", "ocx", "scr", "cpl"]) {
      expect(emptyImportExportIsNormalFor(ext), ext).toBe(false);
    }
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
