/**
 * MacrosDialog (CPE-1189, epic CPE-739). The dialog is a thin render over the typed macro commands;
 * these assert it lists macros, the step editor's add/remove/reorder logic, save/delete, and
 * export/import. The typed `commands.*` client routes through the mocked `../invoke`, so mocking
 * `invoke` here drives it — mirrors TemplatesDialog.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { styleBlock, declaration, contentIndependentHeightReason } from "../svelteCss";

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

/**
 * CPE-1968 — the swallowed-click shape, guarded here BEFORE it ever bit.
 *
 * `OrganizeDialog.svelte` shipped a header control above a body that grew when an async load landed,
 * on a vertically centred backdrop: the growth re-centred the dialog, the control slid up ~98px under
 * the pointer, and the resulting click was eaten by `.dialog`'s `on:click|stopPropagation` in total
 * silence. This dialog has the identical shape — `+ New macro` in the `<header>`, `.list` below it,
 * `onMount(refresh)` -> `commands.macroList()` filling that list ~a frame later.
 *
 * It never failed in CI, and the reason is worth writing down because it is the reason a guard is
 * needed rather than a reason one is not: `gui-smoke/specs/macro-in-menu.smoke.ts` clicks
 * `[data-testid="new-macro-btn"]` against an EMPTY catalog, so the load resolves to `[]` and the box
 * does not change height. That is the harness's case. Any user who has saved a macro gets the growth
 * every single time the dialog opens, which is the ordinary case, not the exotic one.
 *
 * So this asserts the same invariant CPE-1968 put on `.preview`, from the same derivation
 * (`src/lib/svelteCss.ts`, shared rather than copied): `.list`'s height must not depend on its
 * CONTENT. Viewport-dependent is fine — the viewport does not change while `macroList` is in flight.
 *
 * RED-PROOF, run and recorded here rather than only in the PR body (CPE-1933 rule 3): restoring
 * `.list { max-height: 30vh; … }` reds 2 of 16 in this file — "`.list` declares a max-height again
 * with no matching height … expected '30vh' to be undefined" and the `flex: 0 0 auto` assertion.
 * Reverted; 16/16 green. The flex leg was red-proofed SEPARATELY, because removing both at once only
 * proves the pair: with the `height` in place and `flex: 0 0 auto` alone deleted, exactly 1 of 16
 * reds — the flex assertion. So it is not decorative. Without it `.dialog`'s flex column shrinks the
 * fixed height back toward content and undoes the fix while the `height` declaration still reads
 * correct, which is the version of this bug that would be hardest to see.
 *
 * NOT ASSERTED, and deliberately: the `{#if editingName !== null}` editor also changes the dialog's
 * height, and `startEdit` awaits `commands.macroLoad` before rendering it. That growth follows the
 * user's own click rather than arriving unbidden after mount, so it is not this shape.
 */
describe("CPE-1968 — the macro list's height does not depend on the loaded catalog", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "MacrosDialog.svelte"), "utf8");
  /** `src-tauri/src/lib.rs`'s `.inner_size(1000.0, 700.0)`; only the vh terms below read it. */
  const VIEWPORT_H = 700;

  it("gives .list a content-independent height, so loading the catalog cannot move the header", () => {
    const list = styleBlock(SRC, "list");

    expect(
      declaration(list, "max-height"),
      "`.list` declares a max-height again with no matching height. That is the CPE-1968 shape: the " +
        "box is ~42px while `macroList()` is in flight and up to the cap once it resolves, so the " +
        "centred dialog slides `+ New macro` up out from under the pointer and `.dialog`'s " +
        "on:click|stopPropagation eats the click in silence.",
    ).toBeUndefined();

    const reason = contentIndependentHeightReason(list, VIEWPORT_H);
    expect(reason, `\`.list\` ${reason}. See CPE-1968 and OrganizeDialog.svelte's \`.preview\`.`).toBeNull();
  });

  it("keeps that height fixed under flex, which would otherwise shrink it back to its content", () => {
    // `.dialog` is a flex column with a `max-height`, so a flex item's definite height is only
    // honoured while `flex-shrink` is 0 — without this the fix above is undone by the layout.
    expect(declaration(styleBlock(SRC, "dialog"), "display")).toMatch(/flex/);
    expect(
      declaration(styleBlock(SRC, "list"), "flex"),
      "`.list` needs `flex: 0 0 auto` — inside `.dialog`'s flex column a shrinkable item falls back " +
        "toward its content height, which reintroduces the CPE-1968 growth the fixed height removes",
    ).toMatch(/^0\s+0\b/);
  });

  it("renders both an empty and a populated catalog into that same box", async () => {
    // The runtime half: the catalog only ever changes what is INSIDE `.list`, never what is around
    // it, so the fixed height above is sufficient — nothing above the header's controls varies.
    const outsideList = (): string => {
      const clone = (document.querySelector(".dialog") as HTMLElement).cloneNode(true) as HTMLElement;
      clone.querySelector('[data-testid="macro-list"]')!.innerHTML = "";
      return clone.innerHTML;
    };

    invokeMock.mockImplementation(async (cmd: string) => (cmd === "macro_list" ? [] : null));
    const empty = render(MacrosDialog);
    await screen.findByTestId("macro-list");
    const withNothing = outsideList();
    empty.unmount();

    invokeMock.mockImplementation(async (cmd: string) => (cmd === "macro_list" ? SUMMARIES : null));
    render(MacrosDialog);
    await screen.findByTestId("macro-Tidy screenshots");

    expect(
      outsideList().replace(/>\d+ macros?</, ">N macros<"),
      "loading a non-empty catalog changed the dialog outside `.list` — with the box's height now " +
        "fixed, that is the only remaining way the header could move (CPE-1968)",
    ).toEqual(withNothing.replace(/>\d+ macros?</, ">N macros<"));
  });
});
