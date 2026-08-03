/**
 * Component tests for the "AI file copilot" Settings section (CPE-1276, epic CPE-977) — the copilot's
 * model-endpoint config. Mirrors ContentEmbedderSettings.test.ts's mocking: mock `@tauri-apps/api/core`'s
 * `invoke`, since the typed `commands.*` client (`../bindings.gen`) flows through it.
 *
 * The load-bearing guarantees under test: the API key is WRITE-ONLY from the UI (saved to the keychain,
 * never echoed back into the field), and "Test connection" reports success or a clear error — never
 * throwing out of the component.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import CopilotSettings from "./CopilotSettings.svelte";
import {
  loadCopilotEnabled,
  saveCopilotEnabled,
  loadCopilotBaseUrl,
  saveCopilotBaseUrl,
  loadCopilotModel,
  saveCopilotModel,
} from "../settings";

let hasKey = false;
const calls: { cmd: string; args: any }[] = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
  if (cmd === "copilot_has_key") return hasKey;
  if (cmd === "copilot_set_key") {
    hasKey = String(args?.key ?? "").trim().length > 0;
    return null;
  }
  if (cmd === "copilot_test") {
    if (String(args?.config?.base_url ?? "").includes("bad")) throw "could not reach the model endpoint";
    return null;
  }
  throw new Error(`unexpected command: ${cmd}`);
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  Channel: class {},
}));

beforeEach(() => {
  invoke.mockClear();
  calls.length = 0;
  hasKey = false;
  saveCopilotEnabled(false);
  saveCopilotBaseUrl("");
  saveCopilotModel("");
});

describe("CopilotSettings (CPE-1276)", () => {
  it("toggling on persists the enabled flag", async () => {
    render(CopilotSettings);
    const toggle = screen.getByTestId("copilot-toggle") as HTMLInputElement;
    expect(loadCopilotEnabled()).toBe(false);
    await fireEvent.change(toggle, { target: { checked: true } });
    expect(loadCopilotEnabled()).toBe(true);
  });

  it("endpoint + model persist on blur", async () => {
    render(CopilotSettings);
    const url = screen.getByTestId("copilot-url") as HTMLInputElement;
    const model = screen.getByTestId("copilot-model") as HTMLInputElement;
    await fireEvent.input(url, { target: { value: "  http://localhost:1234/v1  " } });
    await fireEvent.blur(url);
    await fireEvent.input(model, { target: { value: "gpt-4o-mini" } });
    await fireEvent.blur(model);
    expect(loadCopilotBaseUrl()).toBe("http://localhost:1234/v1"); // trimmed
    expect(loadCopilotModel()).toBe("gpt-4o-mini");
  });

  it("saving a key calls the keychain command, clears the field, and NEVER echoes the value back", async () => {
    render(CopilotSettings);
    const keyField = screen.getByTestId("copilot-key") as HTMLInputElement;
    await fireEvent.input(keyField, { target: { value: "sk-super-secret" } });
    await fireEvent.click(screen.getByTestId("copilot-key-save"));

    const setCalls = calls.filter((c) => c.cmd === "copilot_set_key");
    expect(setCalls).toHaveLength(1);
    expect(setCalls[0].args.key).toBe("sk-super-secret");

    await waitFor(() => expect(keyField.value).toBe(""));
    expect(document.body.innerHTML).not.toContain("sk-super-secret");
    await waitFor(() => expect(screen.getByTestId("copilot-key-saved")).toBeTruthy());
  });

  it("Test connection reports success", async () => {
    saveCopilotBaseUrl("http://localhost:1234/v1");
    saveCopilotModel("m");
    render(CopilotSettings);
    await fireEvent.click(screen.getByTestId("copilot-test"));
    await waitFor(() => {
      const msg = screen.getByTestId("copilot-test-msg");
      expect(msg.textContent).toContain("Connected");
    });
  });

  it("Test connection surfaces a clear error (never throws) on failure", async () => {
    saveCopilotBaseUrl("http://bad-host/v1");
    saveCopilotModel("m");
    render(CopilotSettings);
    await fireEvent.click(screen.getByTestId("copilot-test"));
    await waitFor(() => {
      const msg = screen.getByTestId("copilot-test-msg");
      expect(msg.textContent).toContain("could not reach the model endpoint");
    });
  });
});
