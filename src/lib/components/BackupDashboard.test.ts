/**
 * CPE-1664 — jsdom render-spec for BackupDashboard's **consent**, the one thing standing between a
 * mirror job and `remove_dir_all` under the destination root (`apply_backup_plan_stream` refuses without
 * it, and a mirror plan's deletes have no Recycle Bin copy and no undo).
 *
 * Filed because the PR #855 security audit mutation-tested this decision and found it pinned by nothing:
 * inverting the `confirmed: true` this component sends left all 3867 frontend tests green. So these
 * cases assert the flag itself, and — more importantly — that it rides on a real click: rendering the
 * dashboard, or pressing **Dry-run**, must never reach the destructive backend command at all.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import BackupDashboard from "./BackupDashboard.svelte";

const { rawInvokeMock, scanTreeMock } = vi.hoisted(() => ({
  rawInvokeMock: vi.fn(),
  scanTreeMock: vi.fn(),
}));

vi.mock("../invoke", () => ({
  rawInvoke: rawInvokeMock,
  // The component only uses the channel as a sink for streamed per-file results.
  createChannel: () => ({ onmessage: null }),
  unwrap: (r: unknown) => r,
}));

vi.mock("../bindings.gen", () => ({ commands: { scanTree: scanTreeMock } }));

/** One job whose plan will contain a mirror DELETE — the destructive case worth confirming. */
const job = { id: "j1", name: "Photos", source: "S:\\pics", dest: "D:\\backup", mirror: true };

/** Calls to the streamed backup command only (the component also scans trees). */
const backupCalls = () => rawInvokeMock.mock.calls.filter((c) => c[0] === "apply_backup_plan_stream");

beforeEach(() => {
  rawInvokeMock.mockReset();
  scanTreeMock.mockReset();
  rawInvokeMock.mockResolvedValue(0);
  // dest holds a file the source doesn't → planBackup puts it in `delete` (mirror mode).
  scanTreeMock.mockImplementation((path: string) =>
    Promise.resolve(
      path === job.dest ? [{ name: "stale.txt", isDir: false, size: 1, modified: 1 }] : [],
    ),
  );
});

describe("BackupDashboard consent (CPE-1664)", () => {
  it("Run sends confirmed: true — the click IS the consent", async () => {
    render(BackupDashboard, { jobs: [job], history: {} });

    await fireEvent.click(screen.getByTestId("run-btn"));

    await waitFor(() => expect(backupCalls()).toHaveLength(1));
    const args = backupCalls()[0][1] as Record<string, unknown>;
    expect(args.confirmed).toBe(true);
    // Sanity: this really is the destructive shape — a mirror delete is in the plan being consented to.
    expect(args.deletePaths).toEqual(["stale.txt"]);
    expect(args.destRoot).toBe(job.dest);
  });

  it("Restore sends it too, with source and dest swapped", async () => {
    render(BackupDashboard, { jobs: [job], history: {} });

    await fireEvent.click(screen.getByTestId("restore-btn"));

    await waitFor(() => expect(backupCalls()).toHaveLength(1));
    const args = backupCalls()[0][1] as Record<string, unknown>;
    expect(args.confirmed).toBe(true);
    expect(args.destRoot).toBe(job.source); // reversed — the restore direction
    expect(args.sourceRoot).toBe(job.dest);
  });

  it("merely rendering the dashboard reaches the backend not at all", () => {
    render(BackupDashboard, { jobs: [job], history: {} });
    expect(rawInvokeMock).not.toHaveBeenCalled();
  });

  it("Dry-run inspects the plan without ever sending consent — nothing is deleted by looking", async () => {
    render(BackupDashboard, { jobs: [job], history: {} });

    await fireEvent.click(screen.getByTestId("dryrun-btn"));

    // It scans (to build the plan) but must never call the destructive command.
    await waitFor(() => expect(screen.getByTestId("plan-summary")).toBeTruthy());
    expect(backupCalls()).toHaveLength(0);
  });
});
