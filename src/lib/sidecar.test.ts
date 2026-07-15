import { describe, it, expect, vi, beforeEach } from "vitest";

// vi.mock is hoisted above declarations, so create the mock fn via vi.hoisted or the
// factory closes over an uninitialised binding (matches App.test.ts).
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  listSidecars,
  platformActive,
  parseUiAnnouncement,
  startAiConsole,
  sidecarDiagnostics,
  emptyDiagnostics,
  consoleUrlWith,
  normalizeFsActivity,
  startAgentWatch,
  stopAgentWatch,
} from "./sidecar";

describe("consoleUrlWith (CPE-313 explorer→console hand-off)", () => {
  const base = "http://127.0.0.1:8731/";

  it("returns the base unchanged when no context is given", () => {
    expect(consoleUrlWith(base)).toBe(base);
    expect(consoleUrlWith(base, "", "  ")).toBe(base);
  });

  it("appends cwd and task as encoded query params", () => {
    const url = new URL(consoleUrlWith(base, "C:\\repos\\app", "Work on: a.ts, b.ts"));
    expect(url.searchParams.get("cwd")).toBe("C:\\repos\\app");
    expect(url.searchParams.get("task")).toBe("Work on: a.ts, b.ts");
  });

  it("uses & when the base already has a query string", () => {
    expect(consoleUrlWith("http://h/?x=1", "/repo")).toBe("http://h/?x=1&cwd=%2Frepo");
  });

  it("omits an absent value but keeps the present one", () => {
    const url = new URL(consoleUrlWith(base, "/repo"));
    expect(url.searchParams.get("cwd")).toBe("/repo");
    expect(url.searchParams.has("task")).toBe(false);
  });
});

beforeEach(() => invoke.mockReset());

describe("sidecar platform client", () => {
  it("returns the registered sidecar ids", async () => {
    invoke.mockResolvedValue(["ai-console", "agent-watch"]);
    expect(await listSidecars()).toEqual(["ai-console", "agent-watch"]);
  });

  // A missing/undefined result (e.g. the command shape changed) degrades to [] via the
  // Array.isArray guard. The rejection path (feature off → invoke rejects) is handled by
  // the try/catch in sidecar.ts and returns [] / false; it isn't asserted here because
  // this test setup flags any error routed through the mocked invoke spy as unhandled,
  // even when the SUT catches it.
  it("degrades to [] on a non-array result", async () => {
    invoke.mockResolvedValue(null);
    expect(await listSidecars()).toEqual([]);
  });

  it("platformActive is true when the command resolves", async () => {
    invoke.mockResolvedValue([]);
    expect(await platformActive()).toBe(true);
  });

  it("startAiConsole returns the served URL", async () => {
    invoke.mockResolvedValue("http://127.0.0.1:55937");
    expect(await startAiConsole()).toBe("http://127.0.0.1:55937");
  });

  it("startAiConsole returns null on an empty/invalid URL", async () => {
    invoke.mockResolvedValue("");
    expect(await startAiConsole()).toBeNull();
  });

  it("parses a loopback ui: announcement", () => {
    expect(parseUiAnnouncement("ui:http://127.0.0.1:55937")).toBe("http://127.0.0.1:55937");
    expect(parseUiAnnouncement("ui:http://localhost:1234")).toBe("http://localhost:1234");
  });

  it("rejects non-loopback or non-ui announcements", () => {
    expect(parseUiAnnouncement("ui:http://evil.example.com")).toBeNull();
    expect(parseUiAnnouncement("running")).toBeNull();
    expect(parseUiAnnouncement("ui:")).toBeNull();
  });

  it("sidecarDiagnostics passes through a well-formed record", async () => {
    invoke.mockResolvedValue({
      id: "ai-console",
      running: true,
      last_error: null,
      logs: [{ level: "info", message: "ui ready at http://127.0.0.1:5" }],
    });
    const d = await sidecarDiagnostics("ai-console");
    expect(d.running).toBe(true);
    expect(d.last_error).toBeNull();
    expect(d.logs).toHaveLength(1);
    expect(d.logs[0].level).toBe("info");
  });

  it("sidecarDiagnostics normalises a malformed record to safe defaults", async () => {
    // Missing logs array + non-string last_error must not throw or leak junk.
    invoke.mockResolvedValue({ id: "ai-console", running: 1, last_error: 42 });
    const d = await sidecarDiagnostics("ai-console");
    expect(d.running).toBe(true);
    expect(d.last_error).toBeNull();
    expect(d.logs).toEqual([]);
  });

  it("emptyDiagnostics is a valid, inert record", () => {
    expect(emptyDiagnostics("x")).toEqual({
      id: "x",
      running: false,
      last_error: null,
      logs: [],
    });
  });
});

describe("Agent Watch filesystem activity (CPE-398)", () => {
  it("normalizeFsActivity keeps well-formed items and drops the rest", () => {
    const items = normalizeFsActivity([
      { kind: "created", path: "/a.txt" },
      { kind: "modified", path: "/b.rs" },
      { kind: "bogus", path: "/c" }, // unknown kind → dropped
      { kind: "removed" }, // no path → dropped
      { kind: "renamed", path: "/d" },
      "nope", // not an object → dropped
    ]);
    expect(items).toEqual([
      { kind: "created", path: "/a.txt" },
      { kind: "modified", path: "/b.rs" },
      { kind: "renamed", path: "/d" },
    ]);
    expect(normalizeFsActivity("not an array")).toEqual([]);
  });

  it("startAgentWatch invokes the command and reports success/failure without throwing", async () => {
    invoke.mockResolvedValueOnce(undefined);
    expect(await startAgentWatch("Z:/repo")).toBe(true);
    expect(invoke).toHaveBeenCalledWith("agent_watch_start", { path: "Z:/repo" });
    invoke.mockRejectedValueOnce(new Error("platform off"));
    expect(await startAgentWatch("Z:/repo")).toBe(false); // degrades, never throws
  });

  it("stopAgentWatch is safe when the platform is off", async () => {
    invoke.mockRejectedValueOnce(new Error("off"));
    await expect(stopAgentWatch()).resolves.toBeUndefined();
  });
});
