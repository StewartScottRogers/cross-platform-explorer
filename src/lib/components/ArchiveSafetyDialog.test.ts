import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import ArchiveSafetyDialog from "./ArchiveSafetyDialog.svelte";

// Plain (non-streaming) call through `invoke` from `src/lib/invoke.ts` (CPE-1318), which itself wraps
// `@tauri-apps/api/core`'s `invoke` — mocking that core module is the standard seam every dialog test in
// this codebase uses (see DuplicatesDialog.test.ts / NearDuplicatesDialog.test.ts).
let calls: Array<{ cmd: string; args: unknown }> = [];
let responder: ((cmd: string, args: unknown) => unknown) | null = null;

const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
  if (!responder) throw new Error(`no responder set for ${cmd}`);
  return responder(cmd, args);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  responder = null;
});

const SAFE_REPORT = {
  report: { total_compressed: 1024, total_uncompressed: 2048, overall_ratio: 2.0, flagged: [], dangerous: false },
  entries_scanned: 3,
  truncated: false,
};

const DANGEROUS_REPORT = {
  report: {
    total_compressed: 1024,
    total_uncompressed: 200_000_000,
    overall_ratio: 195312.5,
    flagged: [{ name: "bomb.bin", ratio: 200000.0 }],
    dangerous: true,
  },
  entries_scanned: 2,
  truncated: false,
};

describe("ArchiveSafetyDialog (CPE-1318)", () => {
  it("scans automatically on mount and calls analyze_archive_safety with the path", async () => {
    responder = () => SAFE_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/archive.zip" });

    expect(screen.getByTestId("as-loading")).toBeTruthy();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("analyze_archive_safety", { path: "/repo/archive.zip" }));
    expect(calls).toEqual([{ cmd: "analyze_archive_safety", args: { path: "/repo/archive.zip" } }]);
  });

  it("renders the ratio and human-readable compressed → uncompressed sizes", async () => {
    responder = () => SAFE_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/archive.zip" });

    await waitFor(() => expect(screen.getByTestId("as-ratio")).toBeTruthy());
    expect(screen.getByTestId("as-ratio").textContent).toBe("2.0x");
    // formatSize: 1024 B -> "1.0 KB", 2048 B -> "2.0 KB".
    expect(screen.getByTestId("as-sizes").textContent).toBe("1.0 KB → 2.0 KB");
    expect(screen.getByTestId("as-entries").textContent?.trim()).toBe("3");
  });

  it("shows the safe state (no danger indicator) when nothing is flagged", async () => {
    responder = () => SAFE_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/archive.zip" });

    await waitFor(() => expect(screen.getByTestId("as-safe")).toBeTruthy());
    expect(screen.queryByTestId("as-danger")).toBeNull();
    expect(screen.getByTestId("as-none-flagged")).toBeTruthy();
    expect(screen.queryByTestId("as-flagged")).toBeNull();
  });

  it("renders flagged-entry rows and the DANGER indicator when the report is dangerous", async () => {
    responder = () => DANGEROUS_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/bomb.zip" });

    await waitFor(() => expect(screen.getByTestId("as-danger")).toBeTruthy());
    expect(screen.queryByTestId("as-safe")).toBeNull();
    const flagged = screen.getByTestId("as-flagged");
    expect(flagged.textContent).toContain("bomb.bin");
    expect(flagged.textContent).toContain("200000.0x");
    expect(screen.queryByTestId("as-none-flagged")).toBeNull();
  });

  it("surfaces a backend error instead of hanging in the loading state, with a retry action", async () => {
    responder = () => {
      throw "not a valid archive"; // a plain (non-Error) rejection, mirroring a Tauri Err(String)
    };
    render(ArchiveSafetyDialog, { path: "/repo/broken.zip" });

    await waitFor(() => expect(screen.getByTestId("as-error")).toBeTruthy());
    expect(screen.getByTestId("as-error").textContent).toBe("not a valid archive");
    expect(screen.queryByTestId("as-loading")).toBeNull();
    expect(screen.getByTestId("as-retry-btn")).toBeTruthy();

    // Retry re-invokes the same command.
    responder = () => SAFE_REPORT;
    await fireEvent.click(screen.getByTestId("as-retry-btn"));
    await waitFor(() => expect(screen.getByTestId("as-ratio")).toBeTruthy());
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("closing dispatches close", async () => {
    responder = () => SAFE_REPORT;
    const { component } = render(ArchiveSafetyDialog, { path: "/repo/archive.zip" });
    let closed = false;
    component.$on("close", () => (closed = true));

    await waitFor(() => expect(screen.getByTestId("as-close-btn")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("as-close-btn"));
    expect(closed).toBe(true);
  });
});
