import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import BinaryPreview from "./BinaryPreview.svelte";

// CPE-1597 (epic CPE-1562 slice 4): jsdom render-spec for the Binary Inspector, wiring the CPE-1572/1581
// `binaryInfo`/`binaryDisasm` backend commands into a standalone component (same mocking recipe as
// CertPreview.test.ts/JwtPreview.test.ts: mock `../bindings.gen`'s `commands` object).

const { binaryInfoMock, binaryDisasmMock } = vi.hoisted(() => ({
  binaryInfoMock: vi.fn(),
  binaryDisasmMock: vi.fn(),
}));

vi.mock("../bindings.gen", () => ({
  commands: { binaryInfo: binaryInfoMock, binaryDisasm: binaryDisasmMock },
}));

interface Section { name: string; address: number; size: number }
interface Import { name: string; library: string | null }
interface Export { name: string; address: number | null }
interface Symbol_ { name: string; address: number | null }
interface Instruction { address: number; bytes: string; text: string }
interface BinaryInfo {
  format: "Pe" | "Elf" | "MachO";
  arch: string | null;
  is_64: boolean;
  sections: Section[];
  imports: Import[];
  exports: Export[];
  symbols: Symbol_[];
  disasm: Instruction[];
}

function ok<T>(data: T) {
  return { status: "ok" as const, data };
}

const nativePe: BinaryInfo = {
  format: "Pe",
  arch: "x86-64",
  is_64: true,
  sections: [{ name: ".text", address: 0x1000, size: 4096 }],
  imports: [
    { name: "CreateFileW", library: "KERNEL32.dll" },
    { name: "MessageBoxW", library: "USER32.dll" },
  ],
  exports: [{ name: "DllMain", address: 0x2000 }],
  symbols: [],
  disasm: [],
};

const managedPe: BinaryInfo = {
  ...nativePe,
  imports: [{ name: "_CorDllMain", library: "mscoree.dll" }],
  exports: [],
};

// The real shape of C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll, as measured during
// this ticket's manual testing: format=Pe, is_64=true, sections=2, imports=0, exports=0 — a modern
// x64/AnyCPU pure-IL assembly loaded off its CLR header, with no classic mscoree.dll import at all. The
// "confirmed" signal above (mscoree.dll import / _Cor*Main export) completely misses this real file; only
// the "possible" (zero imports AND zero exports) signal catches it.
const mscorlibShapedPe: BinaryInfo = {
  format: "Pe",
  arch: "x86-64",
  is_64: true,
  sections: [
    { name: ".text", address: 0x2000, size: 8192 },
    { name: ".reloc", address: 0x4000, size: 512 },
  ],
  imports: [],
  exports: [],
  symbols: [],
  disasm: [],
};

function manyImports(n: number): Import[] {
  return Array.from({ length: n }, (_, i) => ({ name: `Fn${i}`, library: "KERNEL32.dll" }));
}

const someDisasm: Instruction[] = [
  { address: 0x1000, bytes: "48 89 e5", text: "mov rbp, rsp" },
  { address: 0x1003, bytes: "c3", text: "ret" },
];

beforeEach(() => {
  binaryInfoMock.mockReset();
  binaryDisasmMock.mockReset();
});

describe("BinaryPreview (CPE-1597)", () => {
  it("loads binaryInfo for the given path and renders the Overview tab by default", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));

    render(BinaryPreview, { path: "/x/app.dll", size: 24_000_000 });

    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    expect(binaryInfoMock).toHaveBeenCalledWith("/x/app.dll");
    expect(screen.getByText("x86-64")).toBeTruthy();
    expect(screen.getByText("64-bit")).toBeTruthy();
  });

  // ---- THE #1 priority: lazy per-tab disassembly fetch ----
  it("never calls binaryDisasm on load/selection — only once the Disassembly tab is opened", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));
    binaryDisasmMock.mockResolvedValueOnce(ok(someDisasm));

    render(BinaryPreview, { path: "/x/app.dll", size: 100 });

    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    // Still on Overview: binaryDisasm must NOT have been called, even though binaryInfo already
    // resolved and the component has had time to react.
    expect(binaryDisasmMock).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));

    await waitFor(() => expect(binaryDisasmMock).toHaveBeenCalledTimes(1));
    expect(binaryDisasmMock).toHaveBeenCalledWith("/x/app.dll");
    await waitFor(() => expect(screen.getByText("mov rbp, rsp")).toBeTruthy());

    // Switching tabs away and back must not refetch.
    await fireEvent.click(screen.getByRole("tab", { name: "Overview" }));
    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));
    expect(binaryDisasmMock).toHaveBeenCalledTimes(1);
  });

  it("shows a disasm empty state when the backend returns no instructions", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));
    binaryDisasmMock.mockResolvedValueOnce(ok([]));

    render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));

    await waitFor(() => expect(screen.getByTestId("binary-disasm-empty")).toBeTruthy());
  });

  // ---- THE #2 priority: never present CIL-as-x86 nonsense as fact ----
  it("caveats the Disassembly tab for a CONFIRMED managed .NET assembly (mscoree.dll import) instead of decoding it", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));

    render(BinaryPreview, { path: "/x/managed.dll", extension: "dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    expect(screen.getByTestId("binary-managed-badge")).toBeTruthy();

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));
    await waitFor(() => expect(screen.getByTestId("binary-managed-disasm-caveat")).toBeTruthy());
    // Never silently decodes it in the background just because the tab is open.
    expect(binaryDisasmMock).not.toHaveBeenCalled();

    // Opt-in "show anyway" still works, transparently, and only fetches once asked.
    await fireEvent.click(screen.getByTestId("binary-managed-show-anyway"));
    await waitFor(() => expect(binaryDisasmMock).toHaveBeenCalledTimes(1));
  });

  it("caveats the Disassembly tab for the REAL mscorlib.dll shape (zero imports, zero exports) — the confirmed signal alone would miss this file", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(mscorlibShapedPe));

    render(BinaryPreview, { path: "/x/mscorlib.dll", extension: "dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    expect(screen.getByTestId("binary-managed-badge").textContent).toMatch(/no imports or exports/);

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));
    await waitFor(() => expect(screen.getByTestId("binary-managed-disasm-caveat")).toBeTruthy());
    // The hedged wording never asserts this AS a .NET assembly (only "confirmed" does) — it says "if".
    expect(screen.getByTestId("binary-managed-disasm-caveat").textContent).toMatch(/no imports or exports/);
    expect(binaryDisasmMock).not.toHaveBeenCalled();
  });

  // ---- Code-review fix: .efi (and .sys) must NOT get the "possible managed" caveat for an empty table
  // — an empty import/export table is NORMAL BY DESIGN for UEFI images (they reach firmware services via
  // EFI_SYSTEM_TABLE, never a PE import table), so the real, valid x86/x64 disassembly must render
  // directly, with no caveat gating it behind an extra click. ----
  it(".efi with zero imports AND zero exports: NO managed badge, a lighter non-gating note instead, and the Disassembly tab shows the REAL decode with no opt-in click required", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(mscorlibShapedPe)); // same 0-imports/0-exports shape as mscorlib.dll
    binaryDisasmMock.mockResolvedValueOnce(ok(someDisasm));

    render(BinaryPreview, { path: "/x/driver.efi", extension: "efi" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    // No "possibly managed" badge — that would be actively wrong for a UEFI image.
    expect(screen.queryByTestId("binary-managed-badge")).toBeNull();
    // Instead, a lighter, purely informational note explains the empty tables as NORMAL for this format
    // (never implying anything about managed .NET) — the opposite of the old, wrong "unusual" framing.
    const note = screen.getByTestId("binary-empty-tables-normal-note");
    expect(note.textContent).toMatch(/normal for a EFI image/);
    expect(note.textContent).toMatch(/not a sign of anything unusual/);

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));

    // No caveat, no "Show anyway" gate — the real disassembly is fetched and shown directly.
    expect(screen.queryByTestId("binary-managed-disasm-caveat")).toBeNull();
    expect(screen.queryByTestId("binary-managed-show-anyway")).toBeNull();
    await waitFor(() => expect(binaryDisasmMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText("mov rbp, rsp")).toBeTruthy());
  });

  it(".sys with zero imports AND zero exports: same treatment as .efi — no managed badge, no gate", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(mscorlibShapedPe));

    render(BinaryPreview, { path: "/x/driver.sys", extension: "sys" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    expect(screen.queryByTestId("binary-managed-badge")).toBeNull();
    expect(screen.getByTestId("binary-empty-tables-normal-note")).toBeTruthy();
  });

  it(".efi with a CONFIRMED managed signal (an actual mscoree.dll import) still caveats — the extension carve-out only ever suppresses the WEAK zero/zero signal", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe)); // real mscoree.dll import

    render(BinaryPreview, { path: "/x/oddball.efi", extension: "efi" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    expect(screen.getByTestId("binary-managed-badge").textContent).toMatch(/managed \.NET assembly/);

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));
    await waitFor(() => expect(screen.getByTestId("binary-managed-disasm-caveat")).toBeTruthy());
    expect(binaryDisasmMock).not.toHaveBeenCalled();
  });

  it(".dll (not in the empty-tables-normal set) still gets the hedged 'possible' caveat for the same zero/zero shape", async () => {
    // Same underlying data as the .efi test above, different extension — proves the carve-out is
    // extension-specific, not a blanket change to the zero/zero signal itself.
    binaryInfoMock.mockResolvedValueOnce(ok(mscorlibShapedPe));

    render(BinaryPreview, { path: "/x/mystery.dll", extension: "dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    expect(screen.getByTestId("binary-managed-badge")).toBeTruthy();
    expect(screen.queryByTestId("binary-empty-tables-normal-note")).toBeNull();
  });

  // ---- THE #3 priority: big tables must not stall the pane ----
  it("caps a large imports table and labels the cap honestly", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok({ ...nativePe, imports: manyImports(1274) }));

    render(BinaryPreview, { path: "/x/kernel32.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: /^Imports/ }));

    const note = await waitFor(() => screen.getByTestId("binary-imports-capped"));
    expect(note.textContent).toMatch(/first 1,000 of 1,274/);
    expect(screen.getAllByText("KERNEL32.dll").length).toBe(1000);
  });

  it("does not show a cap note when the table is under the cap", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));

    render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: /^Imports/ }));

    expect(screen.queryByTestId("binary-imports-capped")).toBeNull();
  });

  // ---- calm, human error/empty states ----
  it("shows a calm message for a file over the preview size cap, not a raw dump", async () => {
    binaryInfoMock.mockResolvedValueOnce({
      status: "error",
      error: "File is too large to preview (134217728 bytes; limit 134217728).",
    });

    render(BinaryPreview, { path: "/x/huge.dll" });

    await waitFor(() => expect(screen.getByTestId("binary-error-too-large")).toBeTruthy());
    expect(screen.getByTestId("binary-error-too-large").textContent).not.toMatch(/134217728/);
  });

  it("shows a calm message for a permission error", async () => {
    binaryInfoMock.mockRejectedValueOnce(new Error("Access is denied. (os error 5)"));

    render(BinaryPreview, { path: "/x/locked.dll" });

    await waitFor(() => expect(screen.getByTestId("binary-error-permission")).toBeTruthy());
  });

  it("shows a calm message for a file that isn't a recognized binary", async () => {
    binaryInfoMock.mockResolvedValueOnce({ status: "error", error: "Malformed entity: cannot parse ELF header" });

    render(BinaryPreview, { path: "/x/notreally.so" });

    await waitFor(() => expect(screen.getByTestId("binary-error-unrecognized")).toBeTruthy());
  });

  it("shows an empty state for an import-free binary instead of a blank table", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok({ ...nativePe, imports: [] }));

    render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: /^Imports/ }));

    expect(screen.getByTestId("binary-imports-empty")).toBeTruthy();
  });

  it("explains PE's normal lack of a symbol table rather than an unexplained empty grid", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe)); // symbols: []

    render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: /^Symbols/ }));

    expect(screen.getByTestId("binary-symbols-empty").textContent).toMatch(/only object files and PDBs/);
  });

  it("resets to the Overview tab and clears disasm state when the previewed file changes", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));
    binaryDisasmMock.mockResolvedValueOnce(ok(someDisasm));

    const { rerender } = render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));
    await waitFor(() => expect(binaryDisasmMock).toHaveBeenCalledTimes(1));

    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    await rerender({ path: "/x/other.dll" });

    await waitFor(() => expect(binaryInfoMock).toHaveBeenCalledWith("/x/other.dll"));
    // Back on Overview for the new file, not still showing the previous file's Disassembly tab.
    expect(screen.getByRole("tab", { name: "Overview" }).className).toMatch(/active/);
  });
});
