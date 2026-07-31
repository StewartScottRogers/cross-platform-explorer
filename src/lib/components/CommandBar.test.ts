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
