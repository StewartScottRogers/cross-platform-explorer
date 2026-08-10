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
import { loadTheme, saveTheme, resetSettings } from "../settings";

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(async () => null),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => {
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
});
