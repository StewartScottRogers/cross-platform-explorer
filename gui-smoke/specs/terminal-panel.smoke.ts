// CPE-1243 (epic CPE-714) — headless GUI smoke/render pin for the embedded terminal dock. Drives the
// REAL built app: toggles the Terminal panel open via the command bar (CommandBar.svelte's
// `[data-testid="cmd-terminal"]`), waits for the xterm.js pane to actually mount a live PTY (a real
// `cmd.exe`/shell, CPE-1242), types a known marker into it, and asserts the shell's ECHOED OUTPUT shows
// up — proving output streaming, input, AND that a real shell is running, not just that a `<div>`
// rendered.
//
// DOM signal used for the assertion: xterm.js renders its own accessibility tree
// (`.xterm-accessibility-tree`, one row per terminal line, real text content) as a core, always-mounted
// part of the widget — independent of whether the visible surface paints via canvas or DOM (see the
// ticket's note: "if the canvas makes assertion hard, assert via term.buffer text"). This is exactly
// that stable text surface, without needing a custom `window.__CPE_TEST_*` hook into the Terminal object.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// A marker unlikely to collide with anything the shell itself prints (banner text, prompt, etc.).
const MARKER = "GUISMOKE_TERMINAL_MARKER_9f3c1a";

describe("CPE-1243 — headless GUI smoke: the terminal dock streams real shell output", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: leave a shot of the failing state (a passing run's shot is the inline `snap()` below).
  afterEach(async function () {
    await snapFailure(this.currentTest, "terminal-panel");
  });

  it("opens rooted at the current folder, echoes a typed command, and closes cleanly", async () => {
    // The command bar's trailing button group (search/Details/Terminal) sits past the right edge of the
    // app's default 1000x700 window — a pre-existing tight fit this ticket didn't cause (Details was
    // already right at the edge) but that this new Terminal button now tips past `window.innerWidth`,
    // making it un-clickable under classic WebDriver ("element not interactable"). Widen the WINDOW
    // (not app code) so every command-bar button is reachable; scoped to this spec/session only.
    await browser.setWindowSize(1400, 900);

    // Wait for the initial `--open=<tmpDir>` navigation (open-dir.smoke.ts) so `currentPath` is settled
    // before opening the terminal "at the current folder".
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Open the terminal panel via the command bar.
    const toggle = await $('[data-testid="cmd-terminal"]');
    await toggle.waitForExist({ timeout: 10_000 });
    await toggle.click();

    const panel = await $('[data-testid="terminal-panel"]');
    await panel.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the terminal dock (TerminalPanel.svelte) to render after the command-bar toggle",
    });

    // The PTY spawned and xterm actually mounted: its accessibility tree (always-present real DOM text,
    // independent of canvas vs DOM rendering) has at least one non-empty row once the shell prints its
    // first prompt.
    const accessibilityTree = await $('[data-testid="terminal-body"] .xterm-accessibility-tree');
    await browser.waitUntil(
      async () => {
        if (!(await accessibilityTree.isExisting())) return false;
        const text = await accessibilityTree.getText();
        return text.trim().length > 0;
      },
      { timeout: 20_000, timeoutMsg: "expected the terminal to print an initial shell prompt" },
    );

    // Focus the live terminal (xterm's own screen element owns keyboard focus) and type a command whose
    // echoed output we can find afterwards — proving BOTH directions: keystrokes reach the real PTY
    // (write_pty) and its output streams back into xterm (open_pty's `on_output` channel -> term.write).
    const screen = await $('[data-testid="terminal-body"] .xterm-screen');
    await screen.waitForExist({ timeout: 10_000 });
    await screen.click();
    await browser.keys(`echo ${MARKER}`);
    await browser.keys("Enter");

    await browser.waitUntil(
      async () => (await accessibilityTree.getText()).includes(MARKER),
      { timeout: 20_000, timeoutMsg: `expected the shell's echoed "${MARKER}" to appear in the terminal` },
    );

    // CPE-1148 Part A: capture the live terminal with the echoed marker visible.
    await snap("terminal-panel");

    // Non-destructive tidy end state: close the panel — this closes the PTY (no leaked shell) and the
    // dock tab together (TerminalPanel.svelte's `onDestroy` -> `closeTerminalTab`).
    const closeBtn = await $('[data-testid="terminal-close-panel"]');
    await closeBtn.click();
    await panel.waitForExist({ reverse: true, timeout: 10_000, timeoutMsg: "expected the terminal panel to unmount after Close" });

    expect(await panel.isExisting()).to.equal(false);
  });
});
