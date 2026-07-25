/**
 * Component tests for the Settings › Shell integration toggle (CPE-1023, epic CPE-712). The backend
 * install/uninstall/query commands are mocked; these assert the toggle reflects installed state, calls the
 * right command on flip, re-reads state after, and degrades to a disabled "coming soon" control off Windows.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import ShellIntegration from "./ShellIntegration.svelte";

const calls: string[] = [];
let installedState = false;

vi.mock("@tauri-apps/api/core", () => ({
  // Channel is imported by ../invoke; provide a stub so the module loads.
  Channel: class {},
  invoke: vi.fn(async (cmd: string) => {
    calls.push(cmd);
    if (cmd === "shell_integration_installed") return installedState;
    if (cmd === "install_shell_integration") {
      installedState = true;
      return undefined;
    }
    if (cmd === "uninstall_shell_integration") {
      installedState = false;
      return undefined;
    }
    return undefined;
  }),
}));

function setPlatform(value: string) {
  Object.defineProperty(window.navigator, "platform", { value, configurable: true });
}

beforeEach(() => {
  calls.length = 0;
  installedState = false;
});
afterEach(() => setPlatform(""));

describe("ShellIntegration toggle (CPE-1023)", () => {
  it("on Windows, reflects not-installed state and installs on check", async () => {
    setPlatform("Win32");
    render(ShellIntegration);
    // Queries state on mount; checkbox starts unchecked + enabled.
    await waitFor(() => expect(calls).toContain("shell_integration_installed"));
    const box = screen.getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(box.disabled).toBe(false);

    await fireEvent.change(box, { target: { checked: true } });
    await waitFor(() => expect(box.checked).toBe(true));
    expect(calls).toContain("install_shell_integration");
    // Re-queried installed state after the mutation.
    expect(calls.filter((c) => c === "shell_integration_installed").length).toBeGreaterThanOrEqual(2);
  });

  it("on Windows, uninstalls when already installed and unchecked", async () => {
    setPlatform("Win32");
    installedState = true;
    render(ShellIntegration);
    const box = await waitFor(() => {
      const b = screen.getByRole("checkbox") as HTMLInputElement;
      expect(b.checked).toBe(true);
      return b;
    });

    await fireEvent.change(box, { target: { checked: false } });
    await waitFor(() => expect(box.checked).toBe(false));
    expect(calls).toContain("uninstall_shell_integration");
  });

  it("off Windows, the control is disabled with a coming-soon note and never calls the backend", async () => {
    setPlatform("Linux x86_64");
    render(ShellIntegration);
    const box = screen.getByRole("checkbox") as HTMLInputElement;
    expect(box.disabled).toBe(true);
    expect(screen.getByText(/coming to Linux soon/i)).toBeTruthy();
    // No install/uninstall/query calls on an unsupported OS.
    expect(calls).not.toContain("install_shell_integration");
    expect(calls).not.toContain("shell_integration_installed");
  });
});
