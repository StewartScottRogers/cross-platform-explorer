/**
 * Component tests for the "AI content search" Settings section (CPE-1273, epic CPE-976) — the configurable
 * real embedder. Mirrors the repo's component-test mocking: mock `@tauri-apps/api/core`'s `invoke`, since
 * the typed `commands.*` client (`../bindings.gen`) flows through it.
 *
 * The load-bearing guarantees under test: the API key is WRITE-ONLY from the UI (saved to the keychain,
 * never echoed back into the field), and "Test connection" reports the detected dimensionality or a clear
 * error — never throwing.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import ContentEmbedderSettings from "./ContentEmbedderSettings.svelte";
import {
  loadContentEmbedderEnabled,
  saveContentEmbedderEnabled,
  loadContentEmbedderBaseUrl,
  saveContentEmbedderBaseUrl,
  loadContentEmbedderModel,
  saveContentEmbedderModel,
} from "../settings";

let hasKey = false;
const calls: { cmd: string; args: any }[] = [];

// The mock: `content_embedder_has_key` reflects `hasKey`; `set_key` records + flips it; `test` returns a
// dim (or throws when the URL contains "bad").
const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
  if (cmd === "content_embedder_has_key") return hasKey;
  if (cmd === "content_embedder_set_key") {
    hasKey = String(args?.key ?? "").trim().length > 0;
    return null;
  }
  if (cmd === "content_embedder_test") {
    if (String(args?.config?.base_url ?? "").includes("bad")) throw "could not reach embeddings endpoint";
    return 1536;
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
  // Reset persisted config so each test starts clean.
  saveContentEmbedderEnabled(false);
  saveContentEmbedderBaseUrl("");
  saveContentEmbedderModel("");
});

describe("ContentEmbedderSettings (CPE-1273)", () => {
  it("toggling on persists the enabled flag", async () => {
    render(ContentEmbedderSettings);
    const toggle = screen.getByTestId("content-embedder-toggle") as HTMLInputElement;
    expect(loadContentEmbedderEnabled()).toBe(false);
    await fireEvent.change(toggle, { target: { checked: true } });
    expect(loadContentEmbedderEnabled()).toBe(true);
  });

  it("endpoint + model persist on blur", async () => {
    render(ContentEmbedderSettings);
    const url = screen.getByTestId("content-embedder-url") as HTMLInputElement;
    const model = screen.getByTestId("content-embedder-model") as HTMLInputElement;
    await fireEvent.input(url, { target: { value: "  http://localhost:1234/v1  " } });
    await fireEvent.blur(url);
    await fireEvent.input(model, { target: { value: "text-embedding-3-small" } });
    await fireEvent.blur(model);
    expect(loadContentEmbedderBaseUrl()).toBe("http://localhost:1234/v1"); // trimmed
    expect(loadContentEmbedderModel()).toBe("text-embedding-3-small");
  });

  it("saving a key calls the keychain command, clears the field, and NEVER echoes the value back", async () => {
    render(ContentEmbedderSettings);
    const keyField = screen.getByTestId("content-embedder-key") as HTMLInputElement;
    await fireEvent.input(keyField, { target: { value: "sk-super-secret" } });
    await fireEvent.click(screen.getByTestId("content-embedder-key-save"));

    // The key was sent to the keychain command exactly once…
    const setCalls = calls.filter((c) => c.cmd === "content_embedder_set_key");
    expect(setCalls).toHaveLength(1);
    expect(setCalls[0].args.key).toBe("sk-super-secret");

    // …and the field is cleared afterwards (write-only — the value is never left in the DOM).
    await waitFor(() => expect(keyField.value).toBe(""));
    expect(document.body.innerHTML).not.toContain("sk-super-secret");
    // The "(saved)" indicator now shows.
    await waitFor(() => expect(screen.getByTestId("content-embedder-key-saved")).toBeTruthy());
  });

  it("Test connection reports the detected dimensionality on success", async () => {
    saveContentEmbedderBaseUrl("http://localhost:1234/v1");
    saveContentEmbedderModel("m");
    render(ContentEmbedderSettings);
    await fireEvent.click(screen.getByTestId("content-embedder-test"));
    await waitFor(() => {
      const msg = screen.getByTestId("content-embedder-test-msg");
      expect(msg.textContent).toContain("1536");
    });
  });

  it("Test connection surfaces a clear error (never throws) on failure", async () => {
    saveContentEmbedderBaseUrl("http://bad-host/v1");
    saveContentEmbedderModel("m");
    render(ContentEmbedderSettings);
    await fireEvent.click(screen.getByTestId("content-embedder-test"));
    await waitFor(() => {
      const msg = screen.getByTestId("content-embedder-test-msg");
      expect(msg.textContent).toContain("could not reach embeddings endpoint");
    });
  });
});
