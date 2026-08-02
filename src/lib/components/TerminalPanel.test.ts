/**
 * TerminalPanel component wiring test (CPE-1243, epic CPE-714). `terminalClient.test.ts` already covers
 * the output->write/input->write_pty/resize->resize_pty/close->close_pty/open-at-current-folder logic
 * directly and headlessly; this test mounts the real Svelte component on top of that same client to prove
 * the UI actually calls it correctly (open-on-mount, tabs, follow folder, panel close = every session
 * closed) — with `@xterm/xterm`/`@xterm/addon-fit` stubbed (real xterm needs a canvas jsdom doesn't
 * implement) and `@tauri-apps/api/core` mocked, mirroring `ai-console-launcher.test.ts`'s stub pattern and
 * `thumbnailClient.test.ts`'s core-module mock.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";

interface Call {
  cmd: string;
  args: any;
}

let calls: Call[] = [];
let nextDockId = 1;
let nextSessionId = 100;

const invoke = vi.fn((cmd: string, args?: any) => {
  calls.push({ cmd, args });
  switch (cmd) {
    case "terminal_dock_open":
      return Promise.resolve(nextDockId++);
    case "terminal_dock_close":
    case "terminal_dock_activate":
    case "terminal_dock_set_cwd":
      return Promise.resolve(null);
    case "open_pty":
      return Promise.resolve(nextSessionId++);
    case "write_pty":
    case "resize_pty":
    case "close_pty":
      return Promise.resolve(null);
    default:
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
  }
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

// A minimal xterm.js stand-in — no canvas/DOM rendering, just enough surface for TerminalPanel's usage
// (loadAddon/open/onData/write/dispose + rows/cols for the resize path). Mirrors ai-console-launcher.
// test.ts's `FakeTerm`. Defined INSIDE each factory (not referenced from an outer variable) because
// `vi.mock` factories are hoisted above top-level declarations (see thumbnailClient.test.ts's `Channel`).
vi.mock("@xterm/xterm", () => {
  class Terminal {
    rows = 24;
    cols = 80;
    loadAddon() {}
    open() {}
    onData() {}
    write() {}
    dispose() {}
  }
  return { Terminal };
});
vi.mock("@xterm/addon-fit", () => {
  class FitAddon {
    fit() {}
  }
  return { FitAddon };
});
vi.mock("@xterm/xterm/css/xterm.css", () => ({}));

// Imported AFTER the mocks are registered (vi.mock is hoisted by vitest, so this is safe either way).
import TerminalPanel from "./TerminalPanel.svelte";

async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  nextDockId = 1;
  nextSessionId = 100;
});

afterEach(() => cleanup());

describe("TerminalPanel (CPE-1243)", () => {
  it("opens one tab rooted at the current folder on mount", async () => {
    render(TerminalPanel, { cwd: "/repo/project" });
    await flush();

    const dockOpen = calls.find((c) => c.cmd === "terminal_dock_open");
    const ptyOpen = calls.find((c) => c.cmd === "open_pty");
    expect(dockOpen?.args).toEqual({ cwd: "/repo/project", title: "" });
    expect(ptyOpen?.args.cwd).toBe("/repo/project");
  });

  it("shows the opened tab in the strip, titled with the folder's basename", async () => {
    const { getByTitle } = render(TerminalPanel, { cwd: "/repo/project" });
    await flush();
    expect(getByTitle("/repo/project")).toBeTruthy();
  });

  it("the + button opens another tab, also at the current folder", async () => {
    const { getByTitle } = render(TerminalPanel, { cwd: "/repo/project" });
    await flush();
    calls = [];

    await fireEvent.click(getByTitle("New terminal at the current folder"));
    await flush();

    const dockOpen = calls.find((c) => c.cmd === "terminal_dock_open");
    const ptyOpen = calls.find((c) => c.cmd === "open_pty");
    expect(dockOpen?.args.cwd).toBe("/repo/project");
    expect(ptyOpen?.args.cwd).toBe("/repo/project");
  });

  it("closing a tab closes its PTY and dock entry", async () => {
    const { getByTitle, queryByTitle } = render(TerminalPanel, { cwd: "/repo/project" });
    await flush();
    calls = [];

    await fireEvent.click(getByTitle("Close tab"));
    await flush();

    expect(calls.map((c) => c.cmd)).toEqual(["close_pty", "terminal_dock_close"]);
    expect(queryByTitle("/repo/project")).toBeNull(); // the tab is gone from the strip
  });

  it("the panel's own close button dispatches 'close' without itself closing sessions (the parent unmounts, which does)", async () => {
    const { component, getByLabelText } = render(TerminalPanel, { cwd: "/repo/project" });
    await flush();
    let closed = 0;
    component.$on("close", () => closed++);

    await fireEvent.click(getByLabelText("Close terminal panel"));
    expect(closed).toBe(1);
  });

  it("unmounting the panel (parent toggles it off) closes every still-open session — no leaked PTY", async () => {
    const { unmount, getByTitle } = render(TerminalPanel, { cwd: "/repo/project" });
    await flush();
    await fireEvent.click(getByTitle("New terminal at the current folder"));
    await flush();
    calls = [];

    unmount();
    await flush();

    const closePtyCalls = calls.filter((c) => c.cmd === "close_pty");
    const dockCloseCalls = calls.filter((c) => c.cmd === "terminal_dock_close");
    expect(closePtyCalls).toHaveLength(2); // both tabs
    expect(dockCloseCalls).toHaveLength(2);
  });

  it("Follow folder off (default): navigating the explorer does NOT cd the terminal", async () => {
    const { rerender } = render(TerminalPanel, { cwd: "/repo/a" });
    await flush();
    calls = [];

    await rerender({ cwd: "/repo/b" });
    await flush();

    expect(calls.find((c) => c.cmd === "write_pty")).toBeUndefined();
    expect(calls.find((c) => c.cmd === "terminal_dock_set_cwd")).toBeUndefined();
  });

  it("Follow folder on: navigating the explorer cd's the active tab's shell", async () => {
    const { getByLabelText, rerender } = render(TerminalPanel, { cwd: "/repo/a" });
    await flush();
    await fireEvent.click(getByLabelText("Follow folder"));
    calls = [];

    await rerender({ cwd: "/repo/b" });
    await flush();

    const setCwd = calls.find((c) => c.cmd === "terminal_dock_set_cwd");
    const write = calls.find((c) => c.cmd === "write_pty");
    expect(setCwd?.args.cwd).toBe("/repo/b");
    expect(write).toBeTruthy();
  });
});
