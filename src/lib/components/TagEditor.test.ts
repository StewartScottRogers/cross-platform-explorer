import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import TagEditor from "./TagEditor.svelte";
import * as settings from "../settings";

// The native-sync helpers (CPE-828) route through the typed `commands.*` client → `../invoke` →
// `@tauri-apps/api/core`, same chokepoint every other component test mocks at.
const invoke = vi.fn(async (cmd: string) => {
  if (cmd === "native_tags_name") return "NTFS alternate data streams";
  if (cmd === "load_tags") return {};
  if (cmd === "set_tags") return {};
  if (cmd === "native_tags_pull") return {};
  if (cmd === "native_tags_push") return null;
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [string])) }));

describe("TagEditor — native-bridge opt-in gating (CPE-1177)", () => {
  beforeEach(() => invoke.mockClear());
  afterEach(() => settings.saveNativeBridgeEnabled(false)); // reset shared settings state between tests

  it("hides the native pull/push controls when nativeBridgeEnabled is off (the default)", async () => {
    settings.saveNativeBridgeEnabled(false);
    render(TagEditor, { paths: ["/a.txt"], name: "a.txt", count: 1 });
    // Give any pending onMount work a tick, then assert the native section never appears.
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByTestId("native-sync")).toBeNull();
    expect(screen.queryByTestId("native-pull")).toBeNull();
    expect(screen.queryByTestId("native-push")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("native_tags_name");
  });

  it("shows the native pull/push controls once nativeBridgeEnabled is on", async () => {
    settings.saveNativeBridgeEnabled(true);
    render(TagEditor, { paths: ["/a.txt"], name: "a.txt", count: 1 });
    await waitFor(() => expect(screen.getByTestId("native-sync")).toBeTruthy());
    expect(screen.getByTestId("native-pull")).toBeTruthy();
    expect(screen.getByTestId("native-push")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("native_tags_name");
  });

  it("stays hidden in batch mode even when the bridge is on (native metadata is per-path)", async () => {
    settings.saveNativeBridgeEnabled(true);
    render(TagEditor, { paths: ["/a.txt", "/b.txt"], count: 2 });
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByTestId("native-sync")).toBeNull();
  });
});
