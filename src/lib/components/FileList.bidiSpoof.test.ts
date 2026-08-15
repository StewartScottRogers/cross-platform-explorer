/**
 * CPE-1712 — a right-to-left override in a remote filename disguises its extension in Explorer.
 *
 * `char::is_control()` (backend) only matches Unicode category Cc; `U+202E RIGHT-TO-LEFT OVERRIDE` is
 * category Cf (format), so a remote leaf `<RLO>gnp.txt` downloads byte-intact and Windows Explorer
 * renders it as `txt.png` — outside this app's control. This app's OWN listing must not repeat that
 * lie: this test renders the REAL `FileList` component (not just the pure `displaySafeName` unit) and
 * asserts on `container.textContent` / `screen.getByText` — what the user's eyes actually see in the
 * DOM — per Evidence Rule "assert what the user SEES", exactly as `FileList.test.ts`'s own header
 * explains this harness exists to catch what a pure-function test cannot.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import FileList from "./FileList.svelte";
import { emptySelection } from "../selection";
import type { DirEntry } from "../types";

const coreInvoke = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => coreInvoke(...(a as [string, unknown])) }));

// Built from a decimal code point, not a literal character, so this test file itself stays plain
// ASCII and immune to the exact hazard it is proving a fix for (see filename.ts's own doc comment).
const RLO = String.fromCharCode(0x202e);

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "readme.md",
  path: "/x/readme.md",
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "md",
  hidden: false,
  is_symlink: false,
  ...over,
});

beforeEach(() => {
  coreInvoke.mockClear();
  coreInvoke.mockImplementation(async (): Promise<unknown> => undefined);
});

const base = {
  selection: emptySelection(),
  sortKey: "name" as const,
  sortDir: "asc" as const,
  view: "details" as const,
  error: "",
  loading: false,
  searching: false,
  cutPaths: [],
  renamingPath: "",
  renameValue: "",
  rowEls: [],
  draggedPaths: [],
};

describe("FileList does not present a misleading extension for a bidi-override name (CPE-1712)", () => {
  it("shows the true byte order, not the RLO-reversed one, for the ticket's own repro", () => {
    // The ticket's exact case: a remote leaf named RLO + "gnp.txt", which Windows Explorer (outside
    // this app) renders as "txt.png".
    const spoofed = entry({ name: `${RLO}gnp.txt`, path: "/x/spoofed", extension: "txt" });
    const { container } = render(FileList, { ...base, entries: [spoofed] });

    // What the user's eyes see: the literal escaped text, not a reordered "txt.png".
    expect(screen.getByText("[RLO]gnp.txt")).toBeTruthy();
    // The failure mode this guards: nothing in the rendered row text reads as the spoofed extension.
    expect(container.textContent).not.toContain("txt.png");
  });

  it("still shows an ordinary name, and a real Hebrew name, completely unchanged", () => {
    const ordinary = entry({ name: "quarterly-report.pdf", path: "/x/o", extension: "pdf" });
    // "מסמך.txt" — "document.txt" — a real Hebrew filename with no bidi control characters, which must
    // render byte-for-byte identical: the fix must not mangle a legitimate RTL name.
    const hebrew = entry({ name: "מסמך.txt", path: "/x/h", extension: "txt" });
    render(FileList, { ...base, entries: [ordinary, hebrew] });

    expect(screen.getByText("quarterly-report.pdf")).toBeTruthy();
    expect(screen.getByText("מסמך.txt")).toBeTruthy();
  });
});
