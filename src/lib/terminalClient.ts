// Terminal dock client (CPE-1243, epic CPE-714): wires an xterm.js terminal instance to a backend PTY
// session (CPE-1242's open_pty/write_pty/resize_pty/close_pty) and to the terminal_tabs dock model
// (terminal_dock_*, CPE-947). Framework-agnostic on purpose — `TerminalPanel.svelte` owns the real
// `@xterm/xterm` `Terminal`/`FitAddon` instances and the DOM; this module only needs the minimal
// `XTermLike` shape below, so the wiring is unit-testable with a fake terminal and no real xterm/canvas
// (mirrors `thumbnailClient.ts`'s framework-agnostic client + jsdom-safe split).
//
// PTY I/O (open/write/resize/close) goes through `rawInvoke`/`createChannel` directly — NOT the
// busy-cursor `invoke` (bindings.gen's `commands.*`) — because a live terminal already renders its own
// continuous "progress" (its own output), and `write_pty` fires on every keystroke; routing that through
// the busy tracker would flicker the wait cursor while typing (CPE-550, mirrors `thumbnailClient.ts`'s
// same choice for `thumbnails_stream`). The one-shot dock bookkeeping calls (open/close/set_cwd a TAB)
// are quick + infrequent, so they use the typed `commands.*` client — busy cursor is appropriate there.

import { rawInvoke, createChannel } from "./invoke";
import { commands } from "./bindings.gen";

/** The minimal xterm.js `Terminal` surface this module needs. Real usage passes an actual `@xterm/xterm`
 *  `Terminal`; tests pass a fake, so no canvas/DOM is required to exercise the wiring. */
export interface XTermLike {
  write(data: string | Uint8Array): void;
  onData(cb: (data: string) => void): void;
}

// ---- base64 <-> bytes ---------------------------------------------------------------------------------
// `open_pty`/`write_pty` speak base64 so exact bytes (ANSI escapes, split multibyte UTF-8) survive the
// Tauri IPC round trip unmangled (see `lib.rs`'s `open_pty` doc comment) — mirroring the sidecar's own
// PTY wire format. `atob`/`btoa` operate on binary (latin1) strings; a `Uint8Array` is the exact-bytes
// shape xterm.js's own `write()` accepts (it does its own UTF-8 decode), so output round-trips
// byte-for-byte rather than going through a lossy JS-string decode first.

/** Decode one base64 chunk (as `open_pty` streams it over its `ipc::Channel`) into raw bytes. */
export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/** Encode bytes (typed/pasted input from xterm's `onData`, UTF-8-encoded) into base64 for `write_pty`. */
export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  // Chunked so a large paste doesn't blow the call stack via `String.fromCharCode(...bytes)` spread.
  const CHUNK = 8192;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

// ---- PtyBridge: one tab's live PTY session -------------------------------------------------------------

/** Ties one xterm.js `Terminal` to one live PTY session: streamed output -> `term.write`, typed input ->
 *  `write_pty`, fit-driven resize -> `resize_pty`, teardown -> `close_pty`. One instance per open dock
 *  tab (`TerminalPanel` owns the id -> `PtyBridge` map). */
export class PtyBridge {
  sessionId: number | null = null;
  private readonly channel = createChannel<string>();

  constructor(private readonly term: XTermLike) {
    this.channel.onmessage = (chunk) => this.term.write(base64ToBytes(chunk));
  }

  /** Spawn the PTY (`shell`, or the backend's OS default when `null`/blank) at `cwd`, sized `rows`x`cols`,
   *  and start streaming its output into the terminal. Also wires `term.onData` -> `write_pty`, so once
   *  this resolves the terminal is a live, two-way shell. Returns the new session id. */
  async open(cwd: string | null, shell: string | null, rows: number, cols: number): Promise<number> {
    const id = await rawInvoke<number>("open_pty", { cwd, shell, rows, cols, onOutput: this.channel });
    this.sessionId = id;
    this.term.onData((data) => void this.sendInput(data));
    return id;
  }

  /** Send typed/pasted input to the shell. No-op before `open()`/after `close()`. */
  async sendInput(data: string): Promise<void> {
    if (this.sessionId == null) return;
    await rawInvoke("write_pty", {
      sessionId: this.sessionId,
      data: bytesToBase64(new TextEncoder().encode(data)),
    });
  }

  /** Resize the shell's PTY (call after the fit addon recomputes rows/cols). No-op before `open()`. */
  async resize(rows: number, cols: number): Promise<void> {
    if (this.sessionId == null) return;
    await rawInvoke("resize_pty", { sessionId: this.sessionId, rows, cols });
  }

  /** Close the PTY session. Idempotent — safe to call more than once (e.g. a panel close racing an
   *  individual tab close), or on a session that never finished opening. */
  async close(): Promise<void> {
    const id = this.sessionId;
    if (id == null) return;
    this.sessionId = null;
    await rawInvoke("close_pty", { sessionId: id });
  }
}

// ---- Dock tab open/close: pairs a `terminal_tabs` dock tab with its PtyBridge ---------------------------

export interface OpenedTerminalTab {
  dockId: number;
  bridge: PtyBridge;
}

/** Open a new dock tab AT `cwd` — the ticket's "opens rooted at the current folder" — and spawn its PTY
 *  there. The two ids (`terminal_dock_open`'s tab id and `open_pty`'s session id) are correlated purely
 *  on the frontend: the dock model is pure bookkeeping and has no idea a PTY exists (see
 *  `terminal_dock_open`'s doc comment in `lib.rs` — the two stay decoupled by design). */
export async function openTerminalTab(
  cwd: string,
  shell: string | null,
  term: XTermLike,
  rows = 24,
  cols = 80,
): Promise<OpenedTerminalTab> {
  const dockId = await commands.terminalDockOpen(cwd, "");
  const bridge = new PtyBridge(term);
  await bridge.open(cwd, shell, rows, cols);
  return { dockId, bridge };
}

/** Close a dock tab AND its PTY together. `terminal_dock_close` alone does not kill any PTY (the dock
 *  model keeps the two decoupled by design), so the frontend is the one place that closes both — leaving
 *  no leaked PTY behind when a tab (or the whole panel) closes. */
export async function closeTerminalTab(dockId: number, bridge: PtyBridge): Promise<void> {
  await bridge.close();
  await commands.terminalDockClose(dockId);
}

// ---- Follow navigation ----------------------------------------------------------------------------------

/** The shell command to `cd` a live interactive shell to `path` — the chosen "follow navigation" behavior
 *  (the ticket asks to pick the sensible one and note it): a PTY's OS-process cwd can't be changed from
 *  outside once it's spawned, and respawning the shell on every explorer navigation would kill whatever
 *  the user has running in it. So "follow" means typing a `cd` into the still-live shell — exactly what a
 *  user would do by hand — while the dock's own `cwd` bookkeeping is updated alongside it via
 *  `terminal_dock_set_cwd`. Windows' `cmd.exe`/PowerShell both accept `cd /d "path"` (the `/d` also
 *  crosses drive letters); POSIX shells use `cd '...'`.
 *
 *  Security (command injection, CPE-1243 review): the path is attacker-controlled — it is whatever a
 *  folder on disk is named, and "follow folder" types this straight into a LIVE interactive shell. NTFS
 *  can't contain `"`, so the Windows double-quote wrap is safe as-is. POSIX filesystems, though, allow
 *  `"` (and `;`, `$()`, backticks, spaces, ...) in a filename, so double-quoting there would let a folder
 *  named e.g. `foo"; rm -rf ~; echo "` break out of the quotes and execute arbitrary shell. POSIX shells'
 *  SINGLE quotes are the one quoting style that treats every character literally with no exceptions (not
 *  even `\`), so the POSIX branch single-quotes the path and escapes any embedded `'` the standard
 *  close-quote/escaped-quote/reopen-quote way: `'` -> `'\''`. */
export function cdCommand(path: string): string {
  const isWindowsPath = /^[a-zA-Z]:[\\/]/.test(path) || path.includes("\\");
  if (isWindowsPath) return `cd /d "${path}"\r\n`;
  const escaped = path.replace(/'/g, `'\\''`);
  return `cd '${escaped}'\n`;
}

/** Follow navigation for an already-open tab: re-cwd the dock tab, then `cd` its live shell to match. */
export async function followNavigation(dockId: number, bridge: PtyBridge, newCwd: string): Promise<void> {
  await commands.terminalDockSetCwd(dockId, newCwd);
  await bridge.sendInput(cdCommand(newCwd));
}

// ---- Per-OS shell picker --------------------------------------------------------------------------------

export interface ShellChoice {
  label: string;
  /** `null` = the backend's own OS default (`pty::default_shell()`); a program name/path overrides it. */
  value: string | null;
}

/** The shell picker's choices for `platform` (pass `navigator.platform || navigator.userAgent` — mirrors
 *  `ShellIntegration.svelte`'s own OS sniff, since the webview's `navigator` reflects the host OS). Pure
 *  so it's directly unit-testable without a DOM. */
export function shellChoicesFor(platform: string): ShellChoice[] {
  if (/win/i.test(platform)) {
    return [
      { label: "System default", value: null },
      { label: "PowerShell", value: "powershell.exe" },
      { label: "Command Prompt", value: "cmd.exe" },
    ];
  }
  const isMac = /mac/i.test(platform);
  return [
    { label: "System default", value: null },
    { label: "bash", value: "/bin/bash" },
    { label: "sh", value: "/bin/sh" },
    ...(isMac ? [{ label: "zsh", value: "/bin/zsh" }] : []),
  ];
}

// ---- Tab title -------------------------------------------------------------------------------------------

/** The last path segment of `path`, for the tab strip's auto-title — mirrors
 *  `cpe_server::terminal_tabs::basename` (the backend's own auto-title rule) so a tab's displayed title
 *  matches what the dock model would compute, without an extra round trip to fetch it. */
export function basename(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "");
  const parts = trimmed.split(/[/\\]/);
  const name = parts[parts.length - 1];
  return name || path;
}
