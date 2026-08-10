/**
 * Component test for the Settings dialog's "Appearance" section (CPE-1536, foundation slice of epic
 * CPE-1492 "light/dark theme"). The dialog hosts several self-contained child sections (native bridge,
 * vaults, scheduled snapshots, tray, shell integration, spotlight hotkey, AI content search, copilot,
 * sidecar platform) that each own their own persistence and may probe the backend on mount — this test
 * mocks `@tauri-apps/api/core`'s `invoke` permissively (defaulting to `null`/`false`) so those children
 * mount without throwing, and focuses assertions on the one thing CPE-1536 adds: the theme `<select>`.
 *
 * Load-bearing guarantees under test:
 *  - The select reflects `settings.loadTheme()` on mount.
 *  - Changing it calls `settings.saveTheme` with the new value AND `theme.applyTheme` (stamping
 *    `document.documentElement.dataset.theme`), same instant-apply feel as the other rows on this page.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SettingsDialog from "./SettingsDialog.svelte";
import { loadTheme, saveTheme, loadContrast, saveContrast, resetSettings } from "../settings";

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(async (_cmd?: string, _args?: unknown) => null as unknown),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd?: string) => {
    // ScheduledSnapshots (mounted unconditionally on this dialog) expects an array back from its
    // unguarded onMount load — everything else here is wrapped in try/catch by its owning component and
    // tolerates a bare `null`.
    if (cmd === "snapshot_schedule_list") return [];
    return null;
  });
  document.documentElement.removeAttribute("data-theme");
});

describe("SettingsDialog Appearance section (CPE-1536)", () => {
  it("the theme select shows the persisted value on mount", async () => {
    saveTheme("light");
    render(SettingsDialog);
    const select = screen.getByTestId("theme-select") as HTMLSelectElement;
    expect(select.value).toBe("light");
  });

  it("defaults to system when nothing was persisted yet", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("theme-select") as HTMLSelectElement;
    expect(select.value).toBe("system");
    expect(loadTheme()).toBe("system");
  });

  it("changing the select persists via saveTheme and applies via applyTheme", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("theme-select") as HTMLSelectElement;
    expect(select.value).toBe("system");

    await fireEvent.change(select, { target: { value: "light" } });

    // Persisted (saveTheme) …
    expect(loadTheme()).toBe("light");
    // … and applied live (applyTheme stamps the dataset attribute CPE-1534's CSS layer selects on).
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("offers System, Light, and Dark as the three theme options (CPE-1541)", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("theme-select") as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toEqual(["system", "light", "dark"]);
  });

  it("selecting Dark persists via saveTheme and applies via applyTheme (CPE-1541)", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("theme-select") as HTMLSelectElement;

    await fireEvent.change(select, { target: { value: "dark" } });

    // Persisted (saveTheme) …
    expect(loadTheme()).toBe("dark");
    // … and applied live (applyTheme resolves "dark" to itself and stamps the dataset attribute).
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});

describe("SettingsDialog Appearance section — Contrast control (CPE-1545)", () => {
  it("the contrast select shows the persisted value on mount", async () => {
    saveContrast("high");
    render(SettingsDialog);
    const select = screen.getByTestId("contrast-select") as HTMLSelectElement;
    expect(select.value).toBe("high");
  });

  it("defaults to off when nothing was persisted yet", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("contrast-select") as HTMLSelectElement;
    expect(select.value).toBe("off");
    expect(loadContrast()).toBe("off");
  });

  it("offers Off, System, and High as the three contrast options", async () => {
    render(SettingsDialog);
    const select = screen.getByTestId("contrast-select") as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toEqual(["off", "system", "high"]);
  });

  it("selecting High persists via saveContrast and composes hc- into dataset.theme", async () => {
    saveTheme("dark");
    render(SettingsDialog);
    const select = screen.getByTestId("contrast-select") as HTMLSelectElement;

    await fireEvent.change(select, { target: { value: "high" } });

    // Persisted (saveContrast) …
    expect(loadContrast()).toBe("high");
    // … and applied live (applyTheme composes the hc- prefix onto the current base theme).
    expect(document.documentElement.dataset.theme).toBe("hc-dark");
  });

  it("switching back to Off drops the hc- prefix from dataset.theme", async () => {
    saveTheme("dark");
    saveContrast("high");
    render(SettingsDialog);
    const select = screen.getByTestId("contrast-select") as HTMLSelectElement;
    expect(select.value).toBe("high");

    // The dialog itself doesn't stamp dataset.theme on mount (that's main.ts's bootstrap job) — drive
    // it to "hc-dark" via a real change first, same as production use, before checking it drops.
    await fireEvent.change(select, { target: { value: "high" } });
    expect(document.documentElement.dataset.theme).toBe("hc-dark");

    await fireEvent.change(select, { target: { value: "off" } });

    expect(loadContrast()).toBe("off");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("changing the theme select keeps the current contrast composed (hc- survives a theme switch)", async () => {
    saveContrast("high");
    render(SettingsDialog);
    const themeSelect = screen.getByTestId("theme-select") as HTMLSelectElement;

    await fireEvent.change(themeSelect, { target: { value: "dark" } });

    expect(loadTheme()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("hc-dark");
  });
});

describe("SettingsDialog Keyboard shortcuts section (CPE-1548)", () => {
  it("the viewer is closed by default", async () => {
    render(SettingsDialog);
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).toBeNull();
  });

  it("clicking 'Customize shortcuts…' opens the KeyboardBindingsDialog viewer", async () => {
    render(SettingsDialog);
    await fireEvent.click(screen.getByTestId("keyboard-shortcuts-btn"));
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeTruthy();
  });
});
