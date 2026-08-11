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
  unreadable: false,
  unreadable_entries: 0,
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
  unreadable: false,
  unreadable_entries: 0,
};

// CPE-1320: a corrupt/unreadable ZIP — `analyze_archive_safety` can't tell "never opened" from "opened,
// nothing in it" without this flag; before the fix both shapes were identical (`entries_scanned: 0,
// report.dangerous: false`) and the dialog rendered the misleading "No zip-bomb risk · 0 entries" safe
// banner for a file that was never actually scanned.
const UNREADABLE_REPORT = {
  report: { total_compressed: 0, total_uncompressed: 0, overall_ratio: 0, flagged: [], dangerous: false },
  entries_scanned: 0,
  truncated: false,
  unreadable: true,
  unreadable_entries: 0,
};

// CPE-1591: a password-protected ZIP opens fine (unreadable stays false) but every entry inside needs a
// password the scan doesn't have — before the fix this collapsed to the exact shape of SAFE_REPORT
// (entries_scanned: 0, dangerous: false, unreadable: false), so the dialog rendered the reassuring safe
// banner having examined nothing. `unreadable_entries` makes this case structurally distinct.
const ENCRYPTED_REPORT = {
  report: { total_compressed: 0, total_uncompressed: 0, overall_ratio: 0, flagged: [], dangerous: false },
  entries_scanned: 0,
  truncated: false,
  unreadable: false,
  unreadable_entries: 3,
};

// A mix: some entries were readable and one tripped the danger threshold, but other entries in the same
// archive couldn't be read (encrypted). The danger verdict must still lead — it's real signal — while the
// dialog also discloses that the scan was incomplete.
const DANGEROUS_WITH_ENCRYPTED_REPORT = {
  report: {
    total_compressed: 1024,
    total_uncompressed: 200_000_000,
    overall_ratio: 195312.5,
    flagged: [{ name: "bomb.bin", ratio: 200000.0 }],
    dangerous: true,
  },
  entries_scanned: 2,
  truncated: false,
  unreadable: false,
  unreadable_entries: 1,
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

  it("shows an unreadable/error state (not the safe banner) for a corrupt or unopenable archive (CPE-1320)", async () => {
    responder = () => UNREADABLE_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/corrupt.zip" });

    await waitFor(() => expect(screen.getByTestId("as-unreadable")).toBeTruthy());
    // Must NOT render as "safe" — that would silently mislead the user about a file that was never
    // actually scanned.
    expect(screen.queryByTestId("as-safe")).toBeNull();
    expect(screen.queryByTestId("as-danger")).toBeNull();
    expect(screen.queryByTestId("as-entries")).toBeNull();
    expect(screen.queryByTestId("as-none-flagged")).toBeNull();
  });

  it("shows a 'couldn't be assessed' state (not the safe banner) for a password-protected archive with no readable entries (CPE-1591)", async () => {
    responder = () => ENCRYPTED_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/secret.zip" });

    await waitFor(() => expect(screen.getByTestId("as-encrypted")).toBeTruthy());
    expect(screen.getByTestId("as-encrypted").textContent).toContain("3");
    // Must NOT render as safe or dangerous — the archive was never actually scanned.
    expect(screen.queryByTestId("as-safe")).toBeNull();
    expect(screen.queryByTestId("as-danger")).toBeNull();
    expect(screen.queryByTestId("as-unreadable")).toBeNull();
  });

  it("still leads with the DANGER banner when danger was found among readable entries, even if some entries were unreadable (CPE-1591)", async () => {
    responder = () => DANGEROUS_WITH_ENCRYPTED_REPORT;
    render(ArchiveSafetyDialog, { path: "/repo/mixed.zip" });

    await waitFor(() => expect(screen.getByTestId("as-danger")).toBeTruthy());
    expect(screen.queryByTestId("as-safe")).toBeNull();
    expect(screen.queryByTestId("as-encrypted")).toBeNull();
    // The incomplete-scan note is still surfaced, alongside the real danger verdict.
    expect(screen.getByTestId("as-partial-note")).toBeTruthy();
    expect(screen.getByTestId("as-unreadable-entries").textContent).toBe("1");
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
