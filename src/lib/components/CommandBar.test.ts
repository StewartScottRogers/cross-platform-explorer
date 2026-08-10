/**
 * CommandBar tests — the Command Palette launcher (CPE-1164). The toolbar exposes a visible button
 * that dispatches the `command-palette` action (App maps it to `paletteOpen = true`), and its tooltip
 * names the palette and shows the Ctrl+Shift+P shortcut.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import CommandBar from "./CommandBar.svelte";
import { locale } from "../i18n";

beforeEach(() => {
  try { localStorage.clear(); } catch { /* ignore */ }
  locale.set("en");
});

describe("CommandBar Command Palette launcher (CPE-1164)", () => {
  it("shows a button whose tooltip names the palette and the Ctrl+Shift+P shortcut", () => {
    render(CommandBar);
    const btn = screen.getByTitle(/Command palette \(Ctrl\+Shift\+P\)/i);
    expect(btn).toBeTruthy();
  });

  it("dispatches the `command-palette` action when clicked (opens the palette)", async () => {
    const { component } = render(CommandBar);
    const action = vi.fn();
    component.$on("action", (e) => action((e as CustomEvent).detail));

    await fireEvent.click(screen.getByTitle(/Command palette \(Ctrl\+Shift\+P\)/i));
    expect(action).toHaveBeenCalledWith("command-palette");
  });
});

describe("CommandBar user-command Toolbar surface (CPE-1577: Toolbar surface wired)", () => {
  it("renders nothing extra when no commands are bound to Toolbar (default)", () => {
    render(CommandBar);
    expect(screen.queryByTitle(/user command/i)).toBeNull();
  });

  it("renders one button per bound command and dispatches uc:<id> on click", async () => {
    const { component } = render(CommandBar, {
      userCommands: [
        { id: "uc_a", name: "Open in VS Code" },
        { id: "uc_b", name: "Ripgrep here" },
      ],
    });
    const action = vi.fn();
    component.$on("action", (e) => action((e as CustomEvent).detail));

    const btn = screen.getByTitle("Ripgrep here (user command)");
    expect(btn).toBeTruthy();
    expect(screen.getByTitle("Open in VS Code (user command)")).toBeTruthy();

    await fireEvent.click(btn);
    expect(action).toHaveBeenCalledWith("uc:uc_b");
  });
});
