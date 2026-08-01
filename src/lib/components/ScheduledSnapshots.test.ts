/**
 * ScheduledSnapshots section test (CPE-1199, epic CPE-735) — the Settings surface for the background
 * snapshot scheduler. Uses a command-router `invoke` mock (the typed `commands` client dispatches
 * through `@tauri-apps/api/core`'s `invoke`), plus a mocked folder picker, and asserts:
 *   - Add builds the right `ScheduleRule` and calls `snapshot_schedule_set`.
 *   - Browse routes through the native folder dialog and fills the path field (path-input picker rule).
 *   - Remove calls `snapshot_schedule_remove` for that root.
 *   - The enable/pause toggle re-saves the rule with `enabled` flipped (inline instant control).
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import ScheduledSnapshots from "./ScheduledSnapshots.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const openMock = openFolderDialog as unknown as ReturnType<typeof vi.fn>;

interface Rule {
  root: string;
  interval_s: number;
  retention: { hourly: number; daily: number; weekly: number; monthly: number };
  enabled: boolean;
}

/** Route the schedule commands over an in-memory catalog so list/set/remove behave realistically
 *  (the component re-lists after every mutation). Returns the recorded calls for assertions. */
function route(initial: Rule[] = []) {
  const catalog = new Map<string, Rule>(initial.map((r) => [r.root, r]));
  const calls: { cmd: string; args: any }[] = [];
  invokeMock.mockImplementation(async (cmd: string, args: any) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case "snapshot_schedule_list":
        return [...catalog.values()];
      case "snapshot_schedule_set":
        catalog.set(args.rule.root, args.rule);
        return null;
      case "snapshot_schedule_remove":
        catalog.delete(args.root);
        return null;
      default:
        return null;
    }
  });
  return calls;
}

const DEFAULT_RETENTION = { hourly: 24, daily: 7, weekly: 4, monthly: 12 };

beforeEach(() => {
  invokeMock.mockReset();
  openMock.mockReset();
});

describe("ScheduledSnapshots", () => {
  it("lists on mount and shows the empty state when there are no rules", async () => {
    route([]);
    render(ScheduledSnapshots);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("snapshot_schedule_list"));
    expect(await screen.findByTestId("schedule-empty")).toBeTruthy();
  });

  it("adds a folder with the chosen interval, defaulting retention and enabled", async () => {
    const calls = route([]);
    render(ScheduledSnapshots);
    await screen.findByTestId("schedule-empty");

    await fireEvent.input(screen.getByTestId("schedule-path"), { target: { value: "D:\\photos" } });
    await fireEvent.input(screen.getByTestId("schedule-interval"), { target: { value: "2" } });
    await fireEvent.change(screen.getByTestId("schedule-unit"), { target: { value: "hours" } });
    await fireEvent.click(screen.getByTestId("schedule-add-btn"));

    await waitFor(() => expect(calls.some((c) => c.cmd === "snapshot_schedule_set")).toBe(true));
    const set = calls.find((c) => c.cmd === "snapshot_schedule_set")!;
    expect(set.args.rule).toEqual({
      root: "D:\\photos",
      interval_s: 2 * 3600, // 2 hours
      retention: DEFAULT_RETENTION,
      enabled: true,
    });
    // The new rule then renders (the component re-lists after the save).
    expect(await screen.findByTestId("schedule-rule")).toBeTruthy();
  });

  it("fills the path field from the native Browse picker (path-input picker rule)", async () => {
    route([]);
    openMock.mockResolvedValue("E:\\work\\docs");
    render(ScheduledSnapshots);
    await screen.findByTestId("schedule-empty");

    await fireEvent.click(screen.getByTestId("schedule-browse"));

    await waitFor(() => expect(openMock).toHaveBeenCalledWith(expect.objectContaining({ directory: true })));
    await waitFor(() =>
      expect((screen.getByTestId("schedule-path") as HTMLInputElement).value).toBe("E:\\work\\docs"),
    );
  });

  it("removes a folder's rule", async () => {
    const calls = route([
      { root: "D:\\photos", interval_s: 86400, retention: DEFAULT_RETENTION, enabled: true },
    ]);
    render(ScheduledSnapshots);
    await screen.findByTestId("schedule-rule");

    await fireEvent.click(screen.getByTestId("schedule-remove"));

    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "snapshot_schedule_remove" && c.args.root === "D:\\photos")).toBe(true),
    );
  });

  it("pauses a rule via the enable toggle without losing it (inline instant control)", async () => {
    const calls = route([
      { root: "D:\\photos", interval_s: 86400, retention: DEFAULT_RETENTION, enabled: true },
    ]);
    render(ScheduledSnapshots);
    await screen.findByTestId("schedule-rule");

    await fireEvent.click(screen.getByTestId("schedule-enabled"));

    await waitFor(() => expect(calls.some((c) => c.cmd === "snapshot_schedule_set")).toBe(true));
    const set = calls.find((c) => c.cmd === "snapshot_schedule_set")!;
    expect(set.args.rule.root).toBe("D:\\photos");
    expect(set.args.rule.enabled).toBe(false);
  });
});
