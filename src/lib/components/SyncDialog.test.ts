import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import SyncDialog from "./SyncDialog.svelte";

// SyncDialog (CPE-495) is a dry-run preview of `forge_repo_status`'s SyncPlan with a per-repo
// on-diverge policy; confirming runs each planned step via `forge_sync`. Isolate the localStorage
// policy/auto-mirror maps (`cpe.syncPolicy`, `cpe.autoMirror`) it persists to, like ContentSearchDialog.
beforeEach(() => { try { localStorage.clear(); } catch { /* ignore */ } });

let statusResponse: any = {
  is_repo: true,
  branch: "main",
  upstream: "origin/main",
  ahead: 1,
  behind: 2,
  dirty: false,
  actions: ["pull-ff", "push"],
  up_to_date: false,
  conflicts_possible: false,
  blocked: null,
  warnings: [],
  conflicted: false,
};
// forge_repo_status returns the plain `RepoSyncStatus` (no Result wrapper — see bindings.gen.ts).
// forge_sync returns `Result<string, string>`; on the wire a successful Rust command resolves with
// the bare Ok value (the wrapping into `{status:"ok", data:...}` happens inside the generated
// `commands.forgeSync`), so the mock below resolves with the plain string.
let syncOutput = "up to date";

function defaultImpl(cmd: string, _args?: any) {
  if (cmd === "forge_repo_status") return Promise.resolve(statusResponse);
  if (cmd === "forge_sync") return Promise.resolve(syncOutput);
  return Promise.reject(new Error("unexpected command " + cmd));
}
const invoke = vi.fn(defaultImpl);
vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(defaultImpl);
});

describe("SyncDialog (CPE-495)", () => {
  it("plans on mount and renders ahead/behind chips + planned steps", async () => {
    render(SyncDialog, { path: "/repo" });

    expect(screen.getByText("Planning…")).toBeTruthy();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_repo_status", { path: "/repo", onDiverge: "merge" }));

    await waitFor(() => expect(screen.getByText("↓ 2 behind")).toBeTruthy());
    expect(screen.getByText("↑ 1 ahead")).toBeTruthy();
    expect(screen.queryByText("● Uncommitted changes")).toBeNull();
    expect(screen.queryByText("✓ In sync")).toBeNull();

    // Planned steps use syncActionLabel's human labels.
    expect(screen.getByText("Fast-forward pull")).toBeTruthy();
    expect(screen.getByText("Push")).toBeTruthy();
    expect(screen.getByText(/Tracking/)).toBeTruthy();
    expect(screen.getByText("origin/main")).toBeTruthy();
  });

  it("shows a dirty chip and an 'up to date' state, suppressing the In-sync chip when dirty", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 0, behind: 0, dirty: true,
      actions: [], up_to_date: true, conflicts_possible: false, blocked: null, warnings: [], conflicted: false,
    };
    render(SyncDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("● Uncommitted changes")).toBeTruthy());
    expect(screen.getByText("Already up to date — nothing to do.")).toBeTruthy();
    // ahead/behind are both falsy but dirty is true, so the "In sync" chip is intentionally
    // suppressed (the component's guard is `!behind && !ahead && !dirty`) — pin as shipped.
    expect(screen.queryByText("✓ In sync")).toBeNull();
  });

  it("re-plans against the new policy when the on-divergence select changes, and shows conflict warnings", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 1, behind: 1, dirty: false,
      actions: ["pull-merge", "push"], up_to_date: false, conflicts_possible: true, blocked: null,
      warnings: ["heads up"], conflicted: false,
    };
    render(SyncDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("Pull (merge)")).toBeTruthy());
    expect(screen.getByText("⚠ Histories have diverged — this may produce merge conflicts to resolve.")).toBeTruthy();
    expect(screen.getByText("⚠ heads up")).toBeTruthy();

    const select = screen.getByLabelText("On divergence") as HTMLSelectElement;
    expect(select.value).toBe("merge");
    invoke.mockClear();
    await fireEvent.change(select, { target: { value: "rebase" } });

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_repo_status", { path: "/repo", onDiverge: "rebase" }));
    expect(localStorage.getItem("cpe.syncPolicy")).toBe(JSON.stringify({ "/repo": "rebase" }));
  });

  it("shows the blocked reason and disables Run sync when the plan is blocked", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 0, behind: 0, dirty: false,
      actions: [], up_to_date: false, conflicts_possible: false, blocked: "unresolved conflicts",
      warnings: [], conflicted: false,
    };
    render(SyncDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("unresolved conflicts")).toBeTruthy());
    expect((screen.getByText("Run sync") as HTMLButtonElement).disabled).toBe(true);
  });

  it("runs each planned step via forge_sync in order, logs success, and dispatches done", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 1, behind: 1, dirty: false,
      actions: ["pull-ff", "push"], up_to_date: false, conflicts_possible: false, blocked: null,
      warnings: [], conflicted: false,
    };
    syncOutput = "up to date";
    const { component } = render(SyncDialog, { path: "/repo" });
    const onDone = vi.fn();
    component.$on("done", onDone);
    // Wait for planning to actually finish — the button always reads "Run sync" while idle (planning
    // or not), so gate on the steps preview instead or the click below no-ops against a stale canRun.
    await waitFor(() => expect(screen.getByText("Fast-forward pull")).toBeTruthy());

    const runBtn = screen.getByText("Run sync") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(false);
    invoke.mockClear();
    await fireEvent.click(runBtn);

    await waitFor(() => expect(screen.getByText("✓ Fast-forward pull — up to date")).toBeTruthy());
    expect(screen.getByText("✓ Push — up to date")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("forge_sync", { path: "/repo", action: "pull-ff" });
    expect(invoke).toHaveBeenCalledWith("forge_sync", { path: "/repo", action: "push" });
    expect(onDone).toHaveBeenCalled();
    // Re-plans after the run completes (the post-run replan calls forge_repo_status again).
    expect(invoke).toHaveBeenCalledWith("forge_repo_status", { path: "/repo", onDiverge: "merge" });
  });

  it("shows 'Syncing…' and disables Close mid-run, then halts on the first failed step without running the rest", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 1, behind: 1, dirty: false,
      actions: ["pull-ff", "push"], up_to_date: false, conflicts_possible: false, blocked: null,
      warnings: [], conflicted: false,
    };
    let rejectSync: (e: unknown) => void;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "forge_repo_status") return Promise.resolve(statusResponse);
      if (cmd === "forge_sync") return new Promise((_resolve, reject) => { rejectSync = reject; });
      return Promise.reject(new Error("unexpected command " + cmd));
    });
    render(SyncDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("Fast-forward pull")).toBeTruthy());

    await fireEvent.click(screen.getByText("Run sync"));
    await waitFor(() => expect(screen.getByText("Syncing…")).toBeTruthy());
    expect((screen.getByText("Close") as HTMLButtonElement).disabled).toBe(true);

    // A backend Err surfaces as a rejected invoke with the plain error string (matching a real Tauri
    // command failure), which `commands.forgeSync`'s catch turns into `{status:"error", error:...}`.
    rejectSync!("conflict in file.txt");
    await waitFor(() => expect(screen.getByText("✗ Fast-forward pull — conflict in file.txt")).toBeTruthy());
    // The loop halts after the first failure — push is never attempted.
    expect(invoke).not.toHaveBeenCalledWith("forge_sync", { path: "/repo", action: "push" });
    expect(document.querySelector(".log.failed")).toBeTruthy();
  });

  it("surfaces a read error from forge_repo_status without throwing, and keeps Run sync disabled", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "forge_repo_status") return Promise.reject(new Error("git not found"));
      return Promise.reject(new Error("unexpected command " + cmd));
    });
    render(SyncDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("Could not read repo status: git not found")).toBeTruthy());
    // status is null, so no chips/steps render and Run sync stays disabled.
    expect((screen.getByText("Run sync") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows 'Resolve conflicts…' only when the plan reports a conflicted tree, and dispatches resolve", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 0, behind: 0, dirty: false,
      actions: [], up_to_date: true, conflicts_possible: false, blocked: null, warnings: [], conflicted: true,
    };
    const { component } = render(SyncDialog, { path: "/repo" });
    const onResolve = vi.fn();
    component.$on("resolve", onResolve);
    await waitFor(() => expect(screen.getByText("Resolve conflicts…")).toBeTruthy());
    await fireEvent.click(screen.getByText("Resolve conflicts…"));
    expect(onResolve).toHaveBeenCalled();
  });

  it("dispatches close on Close click, Escape, and backdrop click", async () => {
    statusResponse = {
      is_repo: true, branch: "main", upstream: "origin/main", ahead: 1, behind: 1, dirty: false,
      actions: ["pull-ff", "push"], up_to_date: false, conflicts_possible: false, blocked: null,
      warnings: [], conflicted: false,
    };
    const { component, container } = render(SyncDialog, { path: "/repo" });
    const onClose = vi.fn();
    component.$on("close", onClose);
    await waitFor(() => expect(screen.getByText("Run sync")).toBeTruthy());

    await fireEvent.click(screen.getByText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);

    const backdrop = container.querySelector(".backdrop") as HTMLElement;
    await fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});
