import { describe, it, expect, vi, beforeEach } from "vitest";

// vi.mock is hoisted above declarations, so create the mock fn via vi.hoisted or the
// factory closes over an uninitialised binding (matches App.test.ts).
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { listSidecars, platformActive, parseUiAnnouncement, startAiConsole } from "./sidecar";

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
});
