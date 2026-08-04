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
let registeredState = false;

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
    // Default-apps registration (CPE-1277).
    if (cmd === "default_file_manager_status") return registeredState;
    if (cmd === "set_default_file_manager") {
      registeredState = true; // registers CPE + (in prod) opens Windows Default apps
      return undefined;
    }
    if (cmd === "unset_default_file_manager") {
      registeredState = false;
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
  registeredState = false;
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

  it("on Linux, reflects not-installed state and installs on check (CPE-1306)", async () => {
    setPlatform("Linux x86_64");
    render(ShellIntegration);
    // Linux now drives the same backend path as Windows — queries state on mount; checkbox starts
    // unchecked + enabled.
    await waitFor(() => expect(calls).toContain("shell_integration_installed"));
    const box = screen.getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(box.disabled).toBe(false);

    await fireEvent.change(box, { target: { checked: true } });
    await waitFor(() => expect(box.checked).toBe(true));
    expect(calls).toContain("install_shell_integration");
    expect(calls.filter((c) => c === "shell_integration_installed").length).toBeGreaterThanOrEqual(2);
    // No "coming soon" note once the OS is supported.
    expect(screen.queryByText(/coming to Linux soon/i)).toBeFalsy();
  });

  it("on Linux, uninstalls when already installed and unchecked (CPE-1306)", async () => {
    setPlatform("Linux x86_64");
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

  it("off Windows and Linux (macOS), the control is disabled with a coming-soon note and never calls the backend", async () => {
    setPlatform("MacIntel");
    render(ShellIntegration);
    const box = screen.getByRole("checkbox") as HTMLInputElement;
    expect(box.disabled).toBe(true);
    expect(screen.getByText(/coming to macOS soon/i)).toBeTruthy();
    // No install/uninstall/query calls on an unsupported OS.
    expect(calls).not.toContain("install_shell_integration");
    expect(calls).not.toContain("shell_integration_installed");
  });
});

describe("Default file manager registration (CPE-1277)", () => {
  it("on Windows, registers CPE and shows the honest confirm-in-Windows note", async () => {
    setPlatform("Win32");
    render(ShellIntegration);
    await waitFor(() => expect(calls).toContain("default_file_manager_status"));

    // Honest copy: never claims to force the default; directs to Windows Default apps.
    expect(screen.getByText(/confirm it in Settings → Default apps/i)).toBeTruthy();
    expect(screen.getByText(/never lets an app set itself as the default/i)).toBeTruthy();

    const register = screen.getByRole("button", { name: /^(re-?)?register$/i });
    await fireEvent.click(register);
    await waitFor(() => expect(calls).toContain("set_default_file_manager"));
    // Re-reads registration state after the mutation and reflects it.
    expect(calls.filter((c) => c === "default_file_manager_status").length).toBeGreaterThanOrEqual(2);
    await waitFor(() => expect(screen.getByText(/Registered —/i)).toBeTruthy());
  });

  it("on Windows, unregisters when already registered", async () => {
    setPlatform("Win32");
    registeredState = true;
    render(ShellIntegration);
    const unregister = await waitFor(() => {
      const b = screen.getByRole("button", { name: /^unregister$/i }) as HTMLButtonElement;
      expect(b.disabled).toBe(false);
      return b;
    });

    await fireEvent.click(unregister);
    await waitFor(() => expect(calls).toContain("unset_default_file_manager"));
  });

  it("off Windows, the register/unregister actions are disabled and the backend is never called", async () => {
    setPlatform("Linux x86_64");
    render(ShellIntegration);
    const register = screen.getByRole("button", { name: /^(re-?)?register$/i }) as HTMLButtonElement;
    const unregister = screen.getByRole("button", { name: /^unregister$/i }) as HTMLButtonElement;
    expect(register.disabled).toBe(true);
    expect(unregister.disabled).toBe(true);
    expect(calls).not.toContain("set_default_file_manager");
    expect(calls).not.toContain("default_file_manager_status");
  });
});
