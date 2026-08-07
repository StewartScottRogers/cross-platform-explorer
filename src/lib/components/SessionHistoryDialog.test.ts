import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import SessionHistoryDialog from "./SessionHistoryDialog.svelte";
import { toJson, redactEvents, type AuditEvent } from "../auditExport";

// The dialog talks to the backend via the typed `commands.auditSessions` / `commands.auditRead`
// client (CPE-964), which routes `TAURI_INVOKE` through `../invoke` -> `@tauri-apps/api/core`'s
// `invoke` for the local transport. Mocking that module is the seam other component specs use
// (see DuplicatesDialog.test.ts). On success the raw Tauri command resolves with the payload
// directly (a string[] / AuditEvent[]); on failure it REJECTS (Tauri's Result<T, E> convention),
// which the generated bindings catch and turn into `{status:"error", error}` for `unwrap()` to
// throw — so error cases below reject the mock rather than resolving an error-shaped object.
let sessionsResult: string[] | Error = [];
let eventsBySession: Record<string, AuditEvent[] | Error> = {};

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "audit_sessions") {
    if (sessionsResult instanceof Error) throw sessionsResult;
    return sessionsResult;
  }
  if (cmd === "audit_read") {
    const r = eventsBySession[args.session];
    if (r instanceof Error) throw r;
    return r ?? [];
  }
  throw new Error(`unexpected command ${cmd}`);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

function ev(over: Partial<AuditEvent> = {}): AuditEvent {
  return { ts: 1_700_000_000_000, session: "s1", kind: "created", path: "/repo/a.txt", ...over };
}

beforeEach(() => {
  invoke.mockClear();
  sessionsResult = [];
  eventsBySession = {};
});

describe("SessionHistoryDialog (CPE-1394)", () => {
  it("loads sessions on mount, auto-selects the newest, and renders its events", async () => {
    sessionsResult = ["session-1", "session-2"];
    eventsBySession["session-2"] = [
      ev({ ts: 1, kind: "created", path: "/repo/a.txt" }),
      ev({ ts: 2, kind: "modified", path: "/repo/b.txt" }),
    ];

    render(SessionHistoryDialog, { home: "" });

    await waitFor(() => expect(screen.getAllByTestId("session-item")).toHaveLength(2));
    expect(invoke).toHaveBeenCalledWith("audit_sessions", undefined);

    // Auto-selects the LAST id in the list (session-2), not the first.
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("audit_read", { session: "session-2" }));
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(2));

    expect(screen.getByText("/repo/a.txt")).toBeTruthy();
    expect(screen.getByText("/repo/b.txt")).toBeTruthy();
    expect(screen.getByTestId("event-count").textContent).toBe("2 events");

    const active = screen.getAllByTestId("session-item").find((el) => el.className.includes("active"));
    expect(active?.textContent?.trim()).toBe("session-2");
  });

  it("selecting a different session calls auditRead and swaps the event list", async () => {
    sessionsResult = ["session-1", "session-2"];
    eventsBySession["session-2"] = [ev({ path: "/repo/newest.txt" })];
    eventsBySession["session-1"] = [ev({ path: "/repo/older-a.txt" }), ev({ path: "/repo/older-b.txt" })];

    render(SessionHistoryDialog, { home: "" });
    await waitFor(() => expect(screen.getByText("/repo/newest.txt")).toBeTruthy());

    const items = screen.getAllByTestId("session-item");
    const first = items.find((el) => el.textContent?.trim() === "session-1")!;
    await fireEvent.click(first);

    expect(invoke).toHaveBeenCalledWith("audit_read", { session: "session-1" });
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(2));
    expect(screen.getByText("/repo/older-a.txt")).toBeTruthy();
    expect(screen.queryByText("/repo/newest.txt")).toBeNull();
    expect(screen.getByTestId("event-count").textContent).toBe("2 events");
  });

  it("shows the empty-sessions state and never calls auditRead", async () => {
    sessionsResult = [];

    render(SessionHistoryDialog, { home: "" });

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("audit_sessions", undefined));
    expect(screen.getByText("No recorded sessions yet.")).toBeTruthy();
    expect(screen.queryByTestId("session-item")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("audit_read", expect.anything());
    // No session selected -> filtered events list is empty too.
    expect(screen.getByText("No events match.")).toBeTruthy();
    expect(screen.getByTestId("event-count").textContent).toBe("0 events");
  });

  it("shows an error when audit_sessions fails", async () => {
    sessionsResult = new Error("disk unavailable");

    render(SessionHistoryDialog, { home: "" });

    await waitFor(() => expect(screen.getByText("Error: disk unavailable")).toBeTruthy());
    // The error branch replaces the event list/count entirely.
    expect(screen.queryByTestId("event-count")).toBeNull();
  });

  it("shows an error when auditRead fails for the selected session", async () => {
    sessionsResult = ["session-1"];
    eventsBySession["session-1"] = new Error("corrupt journal");

    render(SessionHistoryDialog, { home: "" });

    await waitFor(() => expect(screen.getByText("Error: corrupt journal")).toBeTruthy());
    expect(screen.queryByTestId("event-count")).toBeNull();
  });

  it("filters events by kind and by path substring", async () => {
    sessionsResult = ["session-1"];
    eventsBySession["session-1"] = [
      ev({ ts: 1, kind: "created", path: "/repo/a.txt" }),
      ev({ ts: 2, kind: "modified", path: "/repo/b.txt" }),
      ev({ ts: 3, kind: "removed", path: "/other/c.txt" }),
    ];

    render(SessionHistoryDialog, { home: "" });
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(3));

    // Kind filter: only "created" checked -> just a.txt.
    await fireEvent.click(screen.getByLabelText("created"));
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(1));
    expect(screen.getByText("/repo/a.txt")).toBeTruthy();

    // Un-check it to restore all three, then filter by path substring.
    await fireEvent.click(screen.getByLabelText("created"));
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(3));

    await fireEvent.input(screen.getByLabelText("Path filter"), { target: { value: "/repo/" } });
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(2));
    expect(screen.queryByText("/other/c.txt")).toBeNull();
    expect(screen.getByTestId("event-count").textContent).toBe("2 events");
  });

  it("exports the currently-filtered events as JSON with the expected payload shape", async () => {
    sessionsResult = ["session-1"];
    const events = [ev({ ts: 1, kind: "created", path: "/repo/a.txt" })];
    eventsBySession["session-1"] = events;

    const { component } = render(SessionHistoryDialog, { home: "" });
    const onExport = vi.fn();
    component.$on("export", (e: CustomEvent<{ format: string; ext: string; content: string }>) => onExport(e.detail));

    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(1));
    await fireEvent.click(screen.getByTestId("export-json"));

    expect(onExport).toHaveBeenCalledWith({ format: "json", ext: "json", content: toJson(events) });
  });

  it("redacts the home-dir prefix in exported content once 'redact home' is checked", async () => {
    sessionsResult = ["session-1"];
    const events = [ev({ ts: 1, kind: "read", path: "/home/alice/secret.txt" })];
    eventsBySession["session-1"] = events;

    const { component } = render(SessionHistoryDialog, { home: "/home/alice" });
    const onExport = vi.fn();
    component.$on("export", (e: CustomEvent<{ format: string; ext: string; content: string }>) => onExport(e.detail));

    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(1));

    await fireEvent.click(screen.getByLabelText("redact home"));
    await fireEvent.click(screen.getByTestId("export-json"));

    const expected = toJson(redactEvents(events, { home: "/home/alice" }));
    expect(onExport).toHaveBeenCalledWith({ format: "json", ext: "json", content: expected });
    expect(expected).toContain("~/secret.txt"); // sanity: home dir collapsed to "~"
  });

  it("disables the export buttons when the filtered event list is empty", async () => {
    sessionsResult = ["session-1"];
    eventsBySession["session-1"] = [ev({ path: "/repo/a.txt" })];

    render(SessionHistoryDialog, { home: "" });
    await waitFor(() => expect(screen.getAllByTestId("event-row")).toHaveLength(1));
    expect((screen.getByTestId("export-json") as HTMLButtonElement).disabled).toBe(false);

    await fireEvent.input(screen.getByLabelText("Path filter"), { target: { value: "no-match-anywhere" } });
    await waitFor(() => expect(screen.getByText("No events match.")).toBeTruthy());

    expect((screen.getByTestId("export-json") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId("export-csv") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId("export-md") as HTMLButtonElement).disabled).toBe(true);
  });

  it("dispatches cancel on the Close button, backdrop click, and Escape", async () => {
    sessionsResult = [];
    const { component, container } = render(SessionHistoryDialog, { home: "" });
    const onCancel = vi.fn();
    component.$on("cancel", onCancel);

    await waitFor(() => expect(screen.getByText("No recorded sessions yet.")).toBeTruthy());

    await fireEvent.click(screen.getByText("Close"));
    expect(onCancel).toHaveBeenCalledTimes(1);

    const backdrop = container.querySelector(".backdrop")!;
    await fireEvent.click(backdrop);
    expect(onCancel).toHaveBeenCalledTimes(2);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(3);
  });
});
