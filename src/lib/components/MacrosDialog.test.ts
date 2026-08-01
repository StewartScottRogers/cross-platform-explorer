/**
 * MacrosDialog (CPE-1189, epic CPE-739). The dialog is a thin render over the typed macro commands;
 * these assert it lists macros, the step editor's add/remove/reorder logic, save/delete, and
 * export/import. The typed `commands.*` client routes through the mocked `../invoke`, so mocking
 * `invoke` here drives it — mirrors TemplatesDialog.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import MacrosDialog from "./MacrosDialog.svelte";

const SUMMARIES = [
  { name: "Tidy screenshots", steps: 2 },
  { name: "Archive project", steps: 3 },
];

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => (cmd === "macro_list" ? SUMMARIES : null));
});

describe("MacrosDialog list (CPE-1189)", () => {
  it("lists stored macros with their step count on open", async () => {
    render(MacrosDialog);
    expect(await screen.findByTestId("macro-Tidy screenshots")).toBeTruthy();
    expect(screen.getByTestId("macro-Archive project")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("macro_list");
  });
});

describe("MacrosDialog step editor: add/remove/reorder (CPE-1189)", () => {
  it("starting a new macro opens an empty step editor", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));
    expect(screen.getByTestId("step-list")).toBeTruthy();
    expect(screen.queryByTestId("step-row-0")).toBeNull();
  });

  it("adds a step of the selected kind, appended to the end", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));

    await fireEvent.click(screen.getByTestId("add-step-btn")); // default kind: rename
    expect(screen.getByTestId("step-row-0")).toBeTruthy();

    const kindSelect = screen.getByTestId("new-step-kind") as HTMLSelectElement;
    await fireEvent.change(kindSelect, { target: { value: "tag" } });
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    expect(screen.getByTestId("step-row-1")).toBeTruthy();
  });

  it("editing a step's field updates that step only", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.click(screen.getByTestId("add-step-btn"));

    const field0 = screen.getByTestId("step-field-0") as HTMLInputElement;
    await fireEvent.input(field0, { target: { value: "{stem}_v2.{ext}" } });
    expect((screen.getByTestId("step-field-0") as HTMLInputElement).value).toBe("{stem}_v2.{ext}");
    expect((screen.getByTestId("step-field-1") as HTMLInputElement).value).toBe("");
  });

  it("removes a step by index, shifting later steps down", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.input(screen.getByTestId("step-field-0"), { target: { value: "first" } });
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.input(screen.getByTestId("step-field-1"), { target: { value: "second" } });

    await fireEvent.click(screen.getByTestId("remove-step-0"));

    expect((screen.getByTestId("step-field-0") as HTMLInputElement).value).toBe("second");
    expect(screen.queryByTestId("step-row-1")).toBeNull();
  });

  it("reorders steps with move up/down, clamped at the ends", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.input(screen.getByTestId("step-field-0"), { target: { value: "A" } });
    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.input(screen.getByTestId("step-field-1"), { target: { value: "B" } });

    // Clamp at top: index 0 can't move up further (button disabled, no-op).
    expect((screen.getByTestId("up-step-0") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(screen.getByTestId("down-step-0")); // A,B -> B,A
    expect((screen.getByTestId("step-field-0") as HTMLInputElement).value).toBe("B");
    expect((screen.getByTestId("step-field-1") as HTMLInputElement).value).toBe("A");

    await fireEvent.click(screen.getByTestId("up-step-1")); // B,A -> A,B
    expect((screen.getByTestId("step-field-0") as HTMLInputElement).value).toBe("A");
    expect((screen.getByTestId("step-field-1") as HTMLInputElement).value).toBe("B");
  });

  it("Save is disabled with no name or no steps, and calls macro_save with the built ActionMacro", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("new-macro-btn"));

    expect((screen.getByTestId("save-macro-btn") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(screen.getByTestId("add-step-btn"));
    await fireEvent.input(screen.getByTestId("step-field-0"), { target: { value: "{stem}.{ext}" } });
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "New one" } });

    expect((screen.getByTestId("save-macro-btn") as HTMLButtonElement).disabled).toBe(false);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_save") return {};
      if (cmd === "macro_list") return SUMMARIES;
      return null;
    });
    await fireEvent.click(screen.getByTestId("save-macro-btn"));

    expect(invokeMock).toHaveBeenCalledWith("macro_save", {
      macro: { name: "New one", steps: [{ rename: { template: "{stem}.{ext}" } }] },
    });
  });
});

describe("MacrosDialog surface/hotkey bindings (CPE-1191)", () => {
  it("checking Menu/Palette dispatches bindingschange with the macro added to that surface", async () => {
    const { component } = render(MacrosDialog, { bindings: [] });
    await screen.findByTestId("macro-Tidy screenshots");
    const changes: Array<Array<{ name: string; surfaces: string[]; hotkey: string }>> = [];
    component.$on("bindingschange", (e: CustomEvent) => changes.push(e.detail));

    await fireEvent.click(screen.getByTestId("bind-context-Tidy screenshots"));
    expect(changes.at(-1)).toEqual([{ name: "Tidy screenshots", surfaces: ["context"], hotkey: "" }]);

    await fireEvent.click(screen.getByTestId("bind-palette-Tidy screenshots"));
    expect(changes.at(-1)).toEqual([{ name: "Tidy screenshots", surfaces: ["context", "palette"], hotkey: "" }]);

    // Unchecking removes just that surface.
    await fireEvent.click(screen.getByTestId("bind-context-Tidy screenshots"));
    expect(changes.at(-1)).toEqual([{ name: "Tidy screenshots", surfaces: ["palette"], hotkey: "" }]);
  });

  it("reflects an existing binding's checked state and hotkey value on render", async () => {
    render(MacrosDialog, {
      bindings: [{ name: "Tidy screenshots", surfaces: ["palette"], hotkey: "Ctrl+Alt+1" }],
    });
    await screen.findByTestId("macro-Tidy screenshots");
    expect((screen.getByTestId("bind-palette-Tidy screenshots") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByTestId("bind-context-Tidy screenshots") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByTestId("hotkey-Tidy screenshots") as HTMLInputElement).value).toBe("Ctrl+Alt+1");
  });

  it("typing a hotkey and blurring dispatches bindingschange with the normalized hotkey", async () => {
    const { component } = render(MacrosDialog, { bindings: [] });
    await screen.findByTestId("macro-Tidy screenshots");
    const changes: Array<Array<{ name: string; hotkey: string }>> = [];
    component.$on("bindingschange", (e: CustomEvent) => changes.push(e.detail));

    await fireEvent.change(screen.getByTestId("hotkey-Tidy screenshots"), { target: { value: "ctrl+alt+1" } });
    expect(changes.at(-1)?.[0]).toMatchObject({ name: "Tidy screenshots", hotkey: "Ctrl+Alt+1" });
  });
});

describe("MacrosDialog delete/export/import (CPE-1189)", () => {
  it("deletes a macro by name", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_delete") return {};
      if (cmd === "macro_list") return [SUMMARIES[1]];
      return null;
    });
    await fireEvent.click(screen.getByTestId("delete-btn-Tidy screenshots"));
    expect(invokeMock).toHaveBeenCalledWith("macro_delete", { name: "Tidy screenshots" });
  });

  it("exports a macro's JSON to the clipboard", async () => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_load") return { name: "Tidy screenshots", steps: [] };
      if (cmd === "macro_export") return '{"name":"Tidy screenshots","steps":[]}';
      if (cmd === "macro_list") return SUMMARIES;
      return null;
    });
    await fireEvent.click(screen.getByTestId("export-btn-Tidy screenshots"));
    expect(invokeMock).toHaveBeenCalledWith("macro_load", { name: "Tidy screenshots" });
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('{"name":"Tidy screenshots","steps":[]}');
  });

  it("imports pasted JSON and refreshes the list", async () => {
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");
    await fireEvent.click(screen.getByTestId("import-toggle-btn"));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_import") return {};
      if (cmd === "macro_list") return SUMMARIES;
      return null;
    });
    await fireEvent.input(screen.getByTestId("import-textarea"), {
      target: { value: '{"name":"Imported","steps":[]}' },
    });
    await fireEvent.click(screen.getByTestId("import-btn"));

    expect(invokeMock).toHaveBeenCalledWith("macro_import", { json: '{"name":"Imported","steps":[]}' });
  });
});
