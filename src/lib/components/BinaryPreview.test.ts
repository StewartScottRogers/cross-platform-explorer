import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import BinaryPreview from "./BinaryPreview.svelte";

// CPE-1597 (epic CPE-1562 slice 4) + CPE-1615 (slice 5): jsdom render-spec for the Binary Inspector,
// wiring the CPE-1572/1581/1596 `binaryInfo`/`binaryDisasm`/`dotnetMetadata` backend commands into a
// standalone component (same mocking recipe as CertPreview.test.ts/JwtPreview.test.ts: mock
// `../bindings.gen`'s `commands` object).

const { binaryInfoMock, binaryDisasmMock, dotnetMetadataMock } = vi.hoisted(() => ({
  binaryInfoMock: vi.fn(),
  binaryDisasmMock: vi.fn(),
  dotnetMetadataMock: vi.fn(),
}));

vi.mock("../bindings.gen", () => ({
  commands: { binaryInfo: binaryInfoMock, binaryDisasm: binaryDisasmMock, dotnetMetadata: dotnetMetadataMock },
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
  is_managed: boolean;
  sections: Section[];
  imports: Import[];
  exports: Export[];
  symbols: Symbol_[];
  disasm: Instruction[];
}
interface AssemblyIdentity {
  name: string;
  version: string;
  culture: string | null;
  public_key: string | null;
  flags: number;
}
interface AssemblyRef {
  name: string;
  version: string;
  culture: string | null;
  public_key_token: string | null;
}
interface TypeDef { name: string; namespace: string }
interface MethodDef { name: string }
interface DotnetMetadata {
  runtime_version: string;
  assembly: AssemblyIdentity | null;
  assembly_refs: AssemblyRef[];
  types: TypeDef[];
  methods: MethodDef[];
}

function ok<T>(data: T) {
  return { status: "ok" as const, data };
}

const nativePe: BinaryInfo = {
  format: "Pe",
  arch: "x86-64",
  is_64: true,
  is_managed: false,
  sections: [{ name: ".text", address: 0x1000, size: 4096 }],
  imports: [
    { name: "CreateFileW", library: "KERNEL32.dll" },
    { name: "MessageBoxW", library: "USER32.dll" },
  ],
  exports: [{ name: "DllMain", address: 0x2000 }],
  symbols: [],
  disasm: [],
};

// The real shape of C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll, as measured during
// CPE-1597's manual testing: format=Pe, is_64=true, imports=0, exports=0 — a modern x64/AnyCPU pure-IL
// assembly loaded off its CLR header, with no classic mscoree.dll import at all. Under CPE-1615, its
// managed-ness comes straight from the backend's `is_managed` flag (a real CLR-header read), not a guess
// from this shape.
const managedPe: BinaryInfo = {
  format: "Pe",
  arch: "x86-64",
  is_64: true,
  is_managed: true,
  sections: [
    { name: ".text", address: 0x2000, size: 8192 },
    { name: ".reloc", address: 0x4000, size: 512 },
  ],
  imports: [],
  exports: [],
  symbols: [],
  disasm: [],
};

const assemblyMeta: DotnetMetadata = {
  runtime_version: "v4.0.30319",
  assembly: {
    name: "mscorlib",
    version: "4.0.0.0",
    culture: null,
    public_key: "b77a5c561934e089",
    flags: 0x0001, // PublicKey
  },
  assembly_refs: [
    { name: "System.Core", version: "3.5.0.0", culture: "en-US", public_key_token: "b03f5f7f11d50a3a" },
  ],
  types: [
    { name: "Object", namespace: "System" },
    { name: "GlobalType", namespace: "" },
  ],
  methods: [{ name: "ToString" }, { name: "GetHashCode" }],
};

function manyImports(n: number): Import[] {
  return Array.from({ length: n }, (_, i) => ({ name: `Fn${i}`, library: "KERNEL32.dll" }));
}

function manyTypes(n: number): TypeDef[] {
  return Array.from({ length: n }, (_, i) => ({ name: `Type${i}`, namespace: "App" }));
}

const someDisasm: Instruction[] = [
  { address: 0x1000, bytes: "48 89 e5", text: "mov rbp, rsp" },
  { address: 0x1003, bytes: "c3", text: "ret" },
];

beforeEach(() => {
  binaryInfoMock.mockReset();
  binaryDisasmMock.mockReset();
  dotnetMetadataMock.mockReset();
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

  // ---- THE #2 priority: never present CIL-as-x86 nonsense as fact — now gated on the real is_managed
  // flag (CPE-1615), not the retired frontend heuristic. ----
  it("caveats the Disassembly tab for a managed .NET assembly (is_managed) instead of decoding it, and never shows a caveat for a native PE", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));

    render(BinaryPreview, { path: "/x/managed.dll" });
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

  it("a native PE with zero imports/exports gets NO managed badge and the Disassembly tab shows the real decode directly — is_managed=false is authoritative, no more hedged guessing", async () => {
    const nativeZeroTables: BinaryInfo = { ...nativePe, is_managed: false, imports: [], exports: [] };
    binaryInfoMock.mockResolvedValueOnce(ok(nativeZeroTables));
    binaryDisasmMock.mockResolvedValueOnce(ok(someDisasm));

    render(BinaryPreview, { path: "/x/driver.efi" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    expect(screen.queryByTestId("binary-managed-badge")).toBeNull();
    // The old "empty tables are normal for .efi/.sys" note is retired along with the heuristic.
    expect(screen.queryByTestId("binary-empty-tables-normal-note")).toBeNull();

    await fireEvent.click(screen.getByRole("tab", { name: "Disassembly" }));

    expect(screen.queryByTestId("binary-managed-disasm-caveat")).toBeNull();
    expect(screen.queryByTestId("binary-managed-show-anyway")).toBeNull();
    await waitFor(() => expect(binaryDisasmMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText("mov rbp, rsp")).toBeTruthy());
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

// ---- CPE-1615: the ".NET metadata" tab, wired to the real backend flag/reader ----
describe("BinaryPreview — .NET metadata tab (CPE-1615)", () => {
  it("shows no '.NET metadata' tab for a native (non-managed) binary", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));

    render(BinaryPreview, { path: "/x/app.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    expect(screen.queryByRole("tab", { name: ".NET metadata" })).toBeNull();
    expect(dotnetMetadataMock).not.toHaveBeenCalled();
  });

  it("shows the '.NET metadata' tab for a managed assembly, and never fetches until it's opened", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));

    render(BinaryPreview, { path: "/x/mscorlib.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    expect(screen.getByRole("tab", { name: ".NET metadata" })).toBeTruthy();
    expect(dotnetMetadataMock).not.toHaveBeenCalled();
  });

  it("opening the tab lazily fetches dotnetMetadata and renders assembly identity, refs, types, and methods", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok(assemblyMeta));

    render(BinaryPreview, { path: "/x/mscorlib.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());

    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));
    await waitFor(() => expect(dotnetMetadataMock).toHaveBeenCalledWith("/x/mscorlib.dll"));

    await waitFor(() => expect(screen.getByText("mscorlib")).toBeTruthy());
    expect(screen.getByText("4.0.0.0")).toBeTruthy();
    expect(screen.getByText("neutral")).toBeTruthy(); // null culture
    expect(screen.getByText("b77a5c561934e089")).toBeTruthy(); // public key, hex
    expect(screen.getByText("PublicKey")).toBeTruthy(); // decoded flag pill

    expect(screen.getByText("System.Core")).toBeTruthy(); // assembly ref
    expect(screen.getByText("Object")).toBeTruthy(); // type
    expect(screen.getByText("ToString")).toBeTruthy(); // method

    // Switching tabs away and back must not refetch.
    await fireEvent.click(screen.getByRole("tab", { name: "Overview" }));
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));
    expect(dotnetMetadataMock).toHaveBeenCalledTimes(1);
  });

  it("a null result (metadata root absent/unparseable) renders a distinct explanation, never a blank pane or a clean-empty-table look", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok(null));

    render(BinaryPreview, { path: "/x/odd.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-null")).toBeTruthy());
    // Structurally distinct testid from every "valid, just empty" state below — never rendered the same way.
    expect(screen.queryByTestId("binary-dotnet-error")).toBeNull();
  });

  it("UAT CPE-1615: a corrupted-metadata-root (null) result never renders like a genuine empty module — the two are structurally distinct", async () => {
    // Reproduces the exact blocking UAT finding: a real managed DLL whose metadata root couldn't be
    // parsed (backend now honestly reports `Ok(None)` — see crates/server/src/dotnet_metadata.rs)
    // must NOT be indistinguishable from a genuine, tiny, valid module that legitimately has no
    // assembly manifest and empty ref/type/method tables. Render both side by side and assert none of
    // the "populated, just empty" markers ever appear for the corrupt/null case.
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok(null)); // corrupted "BSJB" signature -> Ok(None)

    render(BinaryPreview, { path: "/x/corrupted.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-null")).toBeTruthy());
    expect(screen.getByText(/metadata root couldn't be located or parsed/)).toBeTruthy();

    // None of the "we found a real (empty) structure" markers a genuine module renders — see the next
    // test — may appear here. A corrupt file must never present as a clean/empty result.
    expect(screen.queryByTestId("binary-dotnet-no-assembly")).toBeNull();
    expect(screen.queryByTestId("binary-dotnet-refs-empty")).toBeNull();
    expect(screen.queryByTestId("binary-dotnet-types-empty")).toBeNull();
    expect(screen.queryByTestId("binary-dotnet-methods-empty")).toBeNull();
    expect(screen.queryByTestId("binary-dotnet-error")).toBeNull();
  });

  it("UAT CPE-1615: a genuine, tiny, valid module (real empty result) renders the populated-empty state, not the null/corrupt state", async () => {
    // The other half of the distinction: a real backend `Ok(Some(..))` for a module with no Assembly
    // row and empty ref/type/method tables (e.g. a genuinely tiny/valid .netmodule) must still render
    // as the populated branch with honest per-table empty notes — never demoted to the null/corrupt
    // rendering just because every field happens to be empty.
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(
      ok({ runtime_version: "v2.0.50727", assembly: null, assembly_refs: [], types: [], methods: [] }),
    );

    render(BinaryPreview, { path: "/x/genuine.netmodule" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-no-assembly")).toBeTruthy());
    expect(screen.getByTestId("binary-dotnet-refs-empty")).toBeTruthy();
    expect(screen.getByTestId("binary-dotnet-types-empty")).toBeTruthy();
    expect(screen.getByTestId("binary-dotnet-methods-empty")).toBeTruthy();

    // Structurally distinct from the corrupt/unparseable case above.
    expect(screen.queryByTestId("binary-dotnet-null")).toBeNull();
    expect(screen.queryByTestId("binary-dotnet-error")).toBeNull();
  });

  it("a command failure (unreadable/malformed) renders a distinct error state, never mistaken for the null-result case", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockRejectedValueOnce(new Error("truncated metadata stream"));

    render(BinaryPreview, { path: "/x/broken.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-error")).toBeTruthy());
    expect(screen.queryByTestId("binary-dotnet-null")).toBeNull();
  });

  it("a module (no assembly manifest) still shows refs/types/methods, with a distinct note instead of assembly identity fields", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok({ ...assemblyMeta, assembly: null }));

    render(BinaryPreview, { path: "/x/module.netmodule" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-no-assembly")).toBeTruthy());
    expect(screen.getByText("System.Core")).toBeTruthy(); // refs still render
  });

  it("empty assembly_refs/types/methods lists get an honest empty state, not a blank table", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(
      ok({ runtime_version: "v4.0.30319", assembly: assemblyMeta.assembly, assembly_refs: [], types: [], methods: [] }),
    );

    render(BinaryPreview, { path: "/x/tiny.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    await waitFor(() => expect(screen.getByTestId("binary-dotnet-refs-empty")).toBeTruthy());
    expect(screen.getByTestId("binary-dotnet-types-empty")).toBeTruthy();
    expect(screen.getByTestId("binary-dotnet-methods-empty")).toBeTruthy();
  });

  it("caps a large types table and labels the cap honestly", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok({ ...assemblyMeta, types: manyTypes(1500) }));

    render(BinaryPreview, { path: "/x/big.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));

    const note = await waitFor(() => screen.getByTestId("binary-dotnet-types-capped"));
    expect(note.textContent).toMatch(/first 1,000 of 1,500/);
  });

  it("resets .NET metadata state when the previewed file changes", async () => {
    binaryInfoMock.mockResolvedValueOnce(ok(managedPe));
    dotnetMetadataMock.mockResolvedValueOnce(ok(assemblyMeta));

    const { rerender } = render(BinaryPreview, { path: "/x/mscorlib.dll" });
    await waitFor(() => expect(screen.getByText("PE (Windows executable/library)")).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: ".NET metadata" }));
    await waitFor(() => expect(dotnetMetadataMock).toHaveBeenCalledTimes(1));

    binaryInfoMock.mockResolvedValueOnce(ok(nativePe));
    await rerender({ path: "/x/other.dll" });

    await waitFor(() => expect(binaryInfoMock).toHaveBeenCalledWith("/x/other.dll"));
    // Native binary: no ".NET metadata" tab at all for the new file.
    expect(screen.queryByRole("tab", { name: ".NET metadata" })).toBeNull();
  });
});
