/**
 * Component tests for IntegrityDialog (CPE-792, epic CPE-737): Baseline a folder's checksums, Verify
 * a fresh scan against the stored baseline, and render the resulting corrupted/missing/edited/new/
 * intact groups. Mirrors DuplicatesDialog.test.ts / AgentTimeline.test.ts's pattern — mock the core
 * `invoke` (the typed `commands.*` client in bindings.gen.ts dispatches to it via `../invoke`).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import IntegrityDialog from "./IntegrityDialog.svelte";
import type { ChecksumEntry, IntegrityReport } from "../integrity";

let checksumResult: ChecksumEntry[] | Error = [];
let verifyResult: IntegrityReport | Error = { intact: [], edited: [], corrupted: [], missing: [], new: [] };

const invoke = vi.fn(async (cmd: string, _args?: unknown) => {
  if (cmd === "checksum_folder") {
    if (checksumResult instanceof Error) throw checksumResult;
    return checksumResult;
  }
  if (cmd === "verify_folder") {
    if (verifyResult instanceof Error) throw verifyResult;
    return verifyResult;
  }
  throw new Error(`unexpected command ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
}));

const entry = (over: Partial<ChecksumEntry> = {}): ChecksumEntry => ({
  path: "/repo/a.txt",
  sha256: "hash-a",
  size: 10,
  modified: 100,
  ...over,
});

const emptyReport = (): IntegrityReport => ({ intact: [], edited: [], corrupted: [], missing: [], new: [] });

beforeEach(() => {
  invoke.mockClear();
  checksumResult = [];
  verifyResult = emptyReport();
});

describe("IntegrityDialog (CPE-792)", () => {
  it("Baseline calls checksumFolder(path) and dispatches baseline with {path, entries}", async () => {
    checksumResult = [entry(), entry({ path: "/repo/b.txt", sha256: "hash-b" })];
    const { component } = render(IntegrityDialog, { initialPath: "/repo" });
    const onBaseline = vi.fn();
    component.$on("baseline", (e: CustomEvent<{ path: string; entries: ChecksumEntry[] }>) => onBaseline(e.detail));

    await fireEvent.click(screen.getByTestId("baseline-btn"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("checksum_folder", { path: "/repo" }));
    expect(onBaseline).toHaveBeenCalledWith({ path: "/repo", entries: checksumResult });
    await waitFor(() => expect(screen.getByTestId("note").textContent).toBe("Baselined 2 files."));
  });

  it("singular file count reads '1 file.' not '1 files.'", async () => {
    checksumResult = [entry()];
    render(IntegrityDialog, { initialPath: "/repo" });
    await fireEvent.click(screen.getByTestId("baseline-btn"));
    await waitFor(() => expect(screen.getByTestId("note").textContent).toBe("Baselined 1 file."));
  });

  it("Verify calls verifyFolder(path, baseline) with the stored baseline for that path", async () => {
    const baselineEntries = [entry()];
    verifyResult = { ...emptyReport(), intact: ["/repo/a.txt"] };
    render(IntegrityDialog, { initialPath: "/repo", baselines: { "/repo": baselineEntries } });

    await fireEvent.click(screen.getByTestId("verify-btn"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("verify_folder", { path: "/repo", baseline: baselineEntries }),
    );
  });

  it("renders the corrupted/missing/edited/new/intact counts and grouped lists", async () => {
    verifyResult = {
      intact: ["/repo/ok.txt"],
      edited: ["/repo/edited.txt"],
      corrupted: ["/repo/bad.txt"],
      missing: ["/repo/gone.txt"],
      new: ["/repo/fresh.txt"],
    };
    render(IntegrityDialog, { initialPath: "/repo", baselines: { "/repo": [entry()] } });
    await fireEvent.click(screen.getByTestId("verify-btn"));

    const counts = await screen.findByTestId("counts");
    expect(counts.textContent).toContain("corrupted 1");
    expect(counts.textContent).toContain("missing 1");
    expect(counts.textContent).toContain("edited 1");
    expect(counts.textContent).toContain("new 1");
    expect(counts.textContent).toContain("intact 1");
    // Any corrupted/missing entry raises the alarm styling (hasIssues).
    expect(counts.classList.contains("alarm")).toBe(true);

    expect(screen.getByTestId("group-corrupted").textContent).toContain("bad.txt");
    expect(screen.getByTestId("group-missing").textContent).toContain("gone.txt");
    expect(screen.getByTestId("group-edited").textContent).toContain("edited.txt");
    expect(screen.getByTestId("group-new").textContent).toContain("fresh.txt");
    // Issues present, so the all-ok banner must not render.
    expect(screen.queryByTestId("all-ok")).toBeNull();
  });

  it("shows the all-ok banner with the intact count when the scan is fully clean", async () => {
    verifyResult = { ...emptyReport(), intact: ["/repo/a.txt", "/repo/b.txt"] };
    render(IntegrityDialog, { initialPath: "/repo", baselines: { "/repo": [entry()] } });
    await fireEvent.click(screen.getByTestId("verify-btn"));

    const allOk = await screen.findByTestId("all-ok");
    expect(allOk.textContent).toContain("All 2 files intact.");
    expect(screen.queryByTestId("group-corrupted")).toBeNull();
    const counts = screen.getByTestId("counts");
    expect(counts.classList.contains("alarm")).toBe(false);
  });

  it("Verify with no stored baseline sends an empty baseline and notes 'No baseline yet'", async () => {
    verifyResult = { ...emptyReport(), new: ["/repo/a.txt"] };
    render(IntegrityDialog, { initialPath: "/repo" }); // no `baselines` prop entry for this path
    expect(screen.getByTestId("baseline-state").textContent).toBe("No baseline stored");

    await fireEvent.click(screen.getByTestId("verify-btn"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("verify_folder", { path: "/repo", baseline: [] }));
    await waitFor(() =>
      expect(screen.getByTestId("note").textContent).toBe("No baseline yet — everything shows as new. Baseline first."),
    );
  });

  it("baseline-state reflects the stored baseline's file count for the current path", () => {
    render(IntegrityDialog, {
      initialPath: "/repo",
      baselines: { "/repo": [entry(), entry({ path: "/repo/b.txt" })] },
    });
    expect(screen.getByTestId("baseline-state").textContent).toBe("Baseline: 2 files");
  });

  it("Baseline failure surfaces the error and does NOT dispatch baseline", async () => {
    checksumResult = new Error("scan failed: permission denied");
    const { component } = render(IntegrityDialog, { initialPath: "/repo" });
    const onBaseline = vi.fn();
    component.$on("baseline", (e: CustomEvent) => onBaseline(e.detail));

    await fireEvent.click(screen.getByTestId("baseline-btn"));

    await waitFor(() => expect(screen.getByTestId("report").textContent).toContain("permission denied"));
    expect(onBaseline).not.toHaveBeenCalled();
  });

  it("Verify failure surfaces the error instead of a report", async () => {
    verifyResult = new Error("verify failed: folder not found");
    render(IntegrityDialog, { initialPath: "/repo", baselines: { "/repo": [entry()] } });

    await fireEvent.click(screen.getByTestId("verify-btn"));

    await waitFor(() => expect(screen.getByTestId("report").textContent).toContain("folder not found"));
    expect(screen.queryByTestId("counts")).toBeNull();
  });

  it("Rebaseline re-scans and dispatches baseline exactly like the Baseline button", async () => {
    checksumResult = [entry()];
    const { component } = render(IntegrityDialog, { initialPath: "/repo" });
    const onBaseline = vi.fn();
    component.$on("baseline", (e: CustomEvent<{ path: string; entries: ChecksumEntry[] }>) => onBaseline(e.detail));

    await fireEvent.click(screen.getByTestId("rebaseline-btn"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("checksum_folder", { path: "/repo" }));
    expect(onBaseline).toHaveBeenCalledWith({ path: "/repo", entries: checksumResult });
  });

  it("the Baseline/Verify buttons are disabled when the path field is blank", () => {
    render(IntegrityDialog, { initialPath: "" });
    expect((screen.getByTestId("baseline-btn") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId("verify-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("verify-on-startup toggle dispatches setVerifyOnStartup with the new checked value, both directions", async () => {
    const { component, unmount } = render(IntegrityDialog, { initialPath: "/repo", verifyOnStartup: false });
    const onSet = vi.fn();
    component.$on("setVerifyOnStartup", (e: CustomEvent<boolean>) => onSet(e.detail));

    const toggle = screen.getByLabelText("Verify all baselined folders on startup") as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    await fireEvent.click(toggle);
    expect(onSet).toHaveBeenCalledWith(true);
    unmount();

    const onSet2 = vi.fn();
    const { component: component2 } = render(IntegrityDialog, { initialPath: "/repo", verifyOnStartup: true });
    component2.$on("setVerifyOnStartup", (e: CustomEvent<boolean>) => onSet2(e.detail));
    const toggle2 = screen.getByLabelText("Verify all baselined folders on startup") as HTMLInputElement;
    expect(toggle2.checked).toBe(true);
    await fireEvent.click(toggle2);
    expect(onSet2).toHaveBeenCalledWith(false);
  });

  it("dispatches cancel on Escape and on the Close button", async () => {
    const { component } = render(IntegrityDialog, { initialPath: "/repo" });
    const onCancel = vi.fn();
    component.$on("cancel", onCancel);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByText("Close"));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });
});
