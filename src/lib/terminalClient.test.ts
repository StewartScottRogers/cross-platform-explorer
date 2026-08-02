/**
 * Tests for the terminal dock client (CPE-1243, epic CPE-714) — the module that wires an xterm.js
 * terminal to the backend's `open_pty`/`write_pty`/`resize_pty`/`close_pty` (CPE-1242) and the
 * `terminal_dock_*` tab model (CPE-947). Mirrors the repo's established streaming-client mocking
 * (`thumbnailClient.test.ts`, `InstantSearch.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`,
 * since `rawInvoke`/`createChannel`/the `commands.*` client (`./invoke`, `./bindings.gen`) ultimately flow
 * through that module. A fake `XTermLike` (no real `@xterm/xterm`, no canvas/DOM) stands in for the
 * terminal, so this exercises the actual wiring — output -> write, input -> write_pty, resize -> resize_pty,
 * close -> close_pty, open-at-current-folder — headlessly and directly, not just "it renders".
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

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

// Imported AFTER the mock is registered (vi.mock is hoisted by vitest, so this is safe either way).
import {
  PtyBridge,
  openTerminalTab,
  closeTerminalTab,
  followNavigation,
  cdCommand,
  shellChoicesFor,
  base64ToBytes,
  bytesToBase64,
  basename,
  type XTermLike,
} from "./terminalClient";

/** A fake xterm.js `Terminal`: records everything written to it and lets tests fire input like the user
 *  typed/pasted it (the real `onData` callback registered by `PtyBridge.open`). */
class FakeTerm implements XTermLike {
  written: (string | Uint8Array)[] = [];
  private dataCb: ((data: string) => void) | null = null;
  write(data: string | Uint8Array): void {
    this.written.push(data);
  }
  onData(cb: (data: string) => void): void {
    this.dataCb = cb;
  }
  /** Simulate the user typing/pasting `data` into the terminal. */
  type(data: string): void {
    this.dataCb?.(data);
  }
}

function lastChannel(cmd: string): { onmessage: ((v: string) => void) | null } {
  const call = [...calls].reverse().find((c) => c.cmd === cmd);
  return call!.args.onOutput;
}

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  nextDockId = 1;
  nextSessionId = 100;
});

describe("base64 <-> bytes round trip (CPE-1243)", () => {
  it("round-trips arbitrary byte values, including ones outside plain ASCII", () => {
    const bytes = new Uint8Array([0, 1, 27, 65, 97, 200, 255]); // 27 = ESC, an ANSI-escape byte
    expect(base64ToBytes(bytesToBase64(bytes))).toEqual(bytes);
  });

  it("handles a large chunk without blowing the call stack (chunked btoa)", () => {
    const bytes = new Uint8Array(50_000).fill(65);
    const b64 = bytesToBase64(bytes);
    expect(base64ToBytes(b64)).toEqual(bytes);
  });
});

describe("PtyBridge output -> term.write (CPE-1243)", () => {
  it("open() spawns the PTY and streams base64 output chunks into the terminal as decoded bytes", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    const id = await bridge.open("/repo", null, 24, 80);

    expect(id).toBe(100);
    expect(bridge.sessionId).toBe(100);
    const openCall = calls.find((c) => c.cmd === "open_pty")!;
    expect(openCall.args).toMatchObject({ cwd: "/repo", shell: null, rows: 24, cols: 80 });

    // The backend streams base64 chunks over the channel passed as `onOutput` — simulate one arriving.
    const channel = lastChannel("open_pty");
    const bytes = new TextEncoder().encode("hello\r\n");
    channel.onmessage?.(bytesToBase64(bytes));

    expect(term.written).toHaveLength(1);
    // Array.from(...) sidesteps a jsdom-vs-Node typed-array realm mismatch in toEqual (values are
    // identical either way — this is purely about which global `Uint8Array` constructor each side has).
    expect(Array.from(term.written[0] as Uint8Array)).toEqual(Array.from(bytes));
  });
});

describe("PtyBridge input -> write_pty (CPE-1243)", () => {
  it("term.onData -> write_pty, base64-encoding the typed bytes", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    await bridge.open("/repo", null, 24, 80);

    term.type("echo hi\r");
    await Promise.resolve(); // sendInput is fire-and-forget from onData; let its promise settle

    const writeCall = calls.find((c) => c.cmd === "write_pty")!;
    expect(writeCall.args.sessionId).toBe(100);
    expect(Array.from(base64ToBytes(writeCall.args.data))).toEqual(
      Array.from(new TextEncoder().encode("echo hi\r")),
    );
  });

  it("sendInput is a no-op before open() and after close()", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    await bridge.sendInput("too early");
    expect(calls.find((c) => c.cmd === "write_pty")).toBeUndefined();

    await bridge.open("/repo", null, 24, 80);
    await bridge.close();
    calls = [];
    await bridge.sendInput("too late");
    expect(calls.find((c) => c.cmd === "write_pty")).toBeUndefined();
  });
});

describe("PtyBridge resize -> resize_pty (CPE-1243)", () => {
  it("resize() forwards rows/cols for the open session", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    await bridge.open("/repo", null, 24, 80);

    await bridge.resize(40, 120);

    const resizeCall = calls.find((c) => c.cmd === "resize_pty")!;
    expect(resizeCall.args).toEqual({ sessionId: 100, rows: 40, cols: 120 });
  });

  it("resize() before open() is a no-op (no call, no throw)", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    await bridge.resize(10, 10);
    expect(calls.find((c) => c.cmd === "resize_pty")).toBeUndefined();
  });
});

describe("PtyBridge close -> close_pty (CPE-1243)", () => {
  it("close() kills the session and is idempotent (no second close_pty call)", async () => {
    const term = new FakeTerm();
    const bridge = new PtyBridge(term);
    await bridge.open("/repo", null, 24, 80);

    await bridge.close();
    expect(bridge.sessionId).toBeNull();
    const closeCalls = calls.filter((c) => c.cmd === "close_pty");
    expect(closeCalls).toHaveLength(1);
    expect(closeCalls[0].args).toEqual({ sessionId: 100 });

    await bridge.close(); // no session left — must not call close_pty again
    expect(calls.filter((c) => c.cmd === "close_pty")).toHaveLength(1);
  });
});

describe("openTerminalTab: open-at-current-folder (CPE-1243)", () => {
  it("opens a dock tab AND its PTY session at the same cwd, correlating the two ids on the frontend", async () => {
    const term = new FakeTerm();
    const currentFolder = "/home/me/project";

    const { dockId, bridge } = await openTerminalTab(currentFolder, null, term);

    expect(dockId).toBe(1);
    expect(bridge.sessionId).toBe(100);

    const dockOpenCall = calls.find((c) => c.cmd === "terminal_dock_open")!;
    const ptyOpenCall = calls.find((c) => c.cmd === "open_pty")!;
    expect(dockOpenCall.args).toEqual({ cwd: currentFolder, title: "" });
    expect(ptyOpenCall.args.cwd).toBe(currentFolder); // SAME folder — "rooted at the current folder"

    // The dock tab was opened before the PTY, so a UI observer sees the tab appear even if the PTY spawn
    // is momentarily slow.
    expect(calls.findIndex((c) => c.cmd === "terminal_dock_open")).toBeLessThan(
      calls.findIndex((c) => c.cmd === "open_pty"),
    );
  });

  it("passes the chosen shell through to open_pty unchanged", async () => {
    const term = new FakeTerm();
    await openTerminalTab("/repo", "powershell.exe", term);
    const ptyOpenCall = calls.find((c) => c.cmd === "open_pty")!;
    expect(ptyOpenCall.args.shell).toBe("powershell.exe");
  });
});

describe("closeTerminalTab (CPE-1243)", () => {
  it("closes the PTY THEN the dock tab, leaving no live session behind", async () => {
    const term = new FakeTerm();
    const { dockId, bridge } = await openTerminalTab("/repo", null, term);
    calls = [];

    await closeTerminalTab(dockId, bridge);

    expect(calls.map((c) => c.cmd)).toEqual(["close_pty", "terminal_dock_close"]);
    expect(calls[1].args).toEqual({ id: dockId });
    expect(bridge.sessionId).toBeNull();
  });
});

describe("followNavigation (CPE-1243)", () => {
  it("re-cwds the dock tab and cd's the live shell to the new folder", async () => {
    const term = new FakeTerm();
    const { dockId, bridge } = await openTerminalTab("/old", null, term);
    calls = [];

    await followNavigation(dockId, bridge, "/new/folder");

    const setCwdCall = calls.find((c) => c.cmd === "terminal_dock_set_cwd")!;
    expect(setCwdCall.args).toEqual({ id: dockId, cwd: "/new/folder" });

    const writeCall = calls.find((c) => c.cmd === "write_pty")!;
    const sent = new TextDecoder().decode(base64ToBytes(writeCall.args.data));
    expect(sent).toContain("/new/folder");
    expect(sent).toContain("cd");
  });
});

describe("cdCommand (CPE-1243)", () => {
  it("uses `cd /d` for a Windows drive path", () => {
    expect(cdCommand("C:\\Users\\me\\project")).toBe('cd /d "C:\\Users\\me\\project"\r\n');
  });

  it("single-quotes a POSIX path", () => {
    expect(cdCommand("/home/me/project")).toBe("cd '/home/me/project'\n");
  });

  it("neutralizes POSIX command injection via an embedded double-quote/semicolon (CPE-1243 security fix)", () => {
    // A folder literally named this is a legal POSIX filename. The old double-quote implementation
    // (`cd "${path}"`) would close the shell's quoting right after `foo`, letting everything from `; rm`
    // onward execute as a real command in the live terminal.
    const malicious = '/tmp/foo"; rm -rf ~; echo "';
    const result = cdCommand(malicious);

    // Single-quoting makes every character literal, including `"`, `;`, and spaces — assert the exact
    // safely-escaped output rather than a loose substring check, so the whole string is pinned down.
    expect(result).toBe(`cd '${malicious}'\n`);

    // Belt-and-suspenders: the dangerous `; rm -rf ~` sequence must never appear OUTSIDE a quoted region.
    // Here the entire path — `"` and all — sits inside a single unbroken `'...'` pair, so a POSIX shell
    // treats it as one literal argument and never reaches a second command.
    const singleQuoted = result.match(/^cd '(.*)'\n$/s);
    expect(singleQuoted).not.toBeNull();
    expect(singleQuoted![1]).toBe(malicious); // no embedded `'` here, so nothing needed escaping
    expect(singleQuoted![1]).not.toMatch(/(?<!')'(?!')/); // the only `'`s are the two wrapping delimiters
  });

  it("escapes an embedded single quote using the '\\'' close/escape/reopen idiom (CPE-1243 security fix)", () => {
    const result = cdCommand("/tmp/it's mine");
    expect(result).toBe("cd '/tmp/it'\\''s mine'\n");

    // Sanity: reconstructing what a POSIX shell would actually see (each `'\''` decodes back to a literal
    // `'`) must round-trip to the original path, proving the escape is correct, not just present.
    const decoded = result
      .replace(/^cd '/, "")
      .replace(/'\n$/, "")
      .replace(/'\\''/g, "'");
    expect(decoded).toBe("/tmp/it's mine");
  });
});

describe("shellChoicesFor (CPE-1243 per-OS shell picker)", () => {
  it("offers PowerShell + Command Prompt on Windows", () => {
    const choices = shellChoicesFor("Win32");
    expect(choices.map((c) => c.label)).toContain("PowerShell");
    expect(choices.map((c) => c.label)).toContain("Command Prompt");
    expect(choices[0]).toEqual({ label: "System default", value: null });
  });

  it("offers zsh on macOS but not on Linux", () => {
    const mac = shellChoicesFor("MacIntel");
    const linux = shellChoicesFor("Linux x86_64");
    expect(mac.some((c) => c.label === "zsh")).toBe(true);
    expect(linux.some((c) => c.label === "zsh")).toBe(false);
    expect(linux.some((c) => c.label === "bash")).toBe(true);
  });
});

describe("basename (CPE-1243 tab auto-title)", () => {
  it("takes the last path segment on either separator", () => {
    expect(basename("/home/me/project")).toBe("project");
    expect(basename("C:\\Users\\me\\project")).toBe("project");
    expect(basename("/home/me/project/")).toBe("project"); // trailing slash ignored
  });

  it("falls back to the whole path when there's no basename to extract", () => {
    expect(basename("/")).toBe("/");
  });
});
