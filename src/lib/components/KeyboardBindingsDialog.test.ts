/**
 * Component test for the keyboard shortcuts viewer + rebind surface (CPE-1548 read-only base +
 * CPE-1549's press-to-set capture, live conflict warning, and reset-to-default). The read-only
 * rendering/filter/close-on-Escape describe blocks below are CPE-1548's, updated only where
 * CPE-1549 changed the row markup (the read-only `kbd` became an interactive
 * `HotkeyCaptureInput`, so "Unbound" is no longer literal — see that block's note). The new
 * describe blocks cover CPE-1549: a successful rebind persists via `saveKeymap`, a colliding
 * rebind shows a warning naming the other action and "Cancel"/"Rebind anyway" resolve it exactly
 * one of two ways (never a silent double-binding), a per-row reset, "Reset all to defaults", and
 * that a capture Escape-cancel doesn't also close the dialog.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import KeyboardBindingsDialog from "./KeyboardBindingsDialog.svelte";
import { ACTIONS, chordFor, defaultKeymap, setChord, exportKeymap, type Keymap } from "../keymap";

vi.mock("../settings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../settings")>();
  return { ...actual, saveKeymap: vi.fn() };
});
import { saveKeymap } from "../settings";

beforeEach(() => {
  vi.mocked(saveKeymap).mockClear();
});

describe("KeyboardBindingsDialog renders every action + its chord (CPE-1548)", () => {
  it("lists every ACTIONS entry, grouped by category", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });

    const groups = screen.getByTestId("keyboard-bindings-groups");
    for (const action of ACTIONS) {
      expect(within(groups).getByText(action.description)).toBeTruthy();
    }
    // Spot-check a couple of group headings appear.
    expect(within(groups).getByText("Navigation")).toBeTruthy();
    expect(within(groups).getByText("Tabs")).toBeTruthy();
  });

  it("shows the effective (overridden) chord, not the built-in default, when the keymap has an override", async () => {
    const keymap = setChord(defaultKeymap(), "newTab", "Ctrl+Alt+N");
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("New tab").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+Alt+N")).toBeTruthy();
  });

  it("renders a friendly display form for a substituted key (arrow glyph)", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("Back").closest(".row") as HTMLElement;
    expect(within(row).getByText("Alt+←")).toBeTruthy();
  });

  it("shows a 'Click to set…' capture prompt (not the passive 'Unbound' label) for an action whose chord is empty", async () => {
    // CPE-1549 turned the read-only `kbd` into an actionable HotkeyCaptureInput — an empty chord
    // now reads as an invitation to set one, not a static "Unbound" status the CPE-1548 version
    // showed. formatChord's "Unbound" text still exists for other read-only surfaces; this row no
    // longer uses it.
    const keymap: Keymap = { ...defaultKeymap(), refresh: "" };
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("Refresh").closest(".row") as HTMLElement;
    expect(within(row).getByText("Click to set…")).toBeTruthy();
    expect(within(row).queryByText("Unbound")).toBeNull();
  });
});

describe("KeyboardBindingsDialog filter (CPE-1548)", () => {
  it("narrows the visible rows by action description, case-insensitively", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "rename" } });

    expect(screen.getByText("Rename")).toBeTruthy();
    expect(screen.queryByText("Copy")).toBeNull();
    expect(screen.queryByText("New tab")).toBeNull();
  });

  it("narrows the visible rows by group name", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "tabs" } });

    expect(screen.getByText("New tab")).toBeTruthy();
    expect(screen.getByText("Close tab")).toBeTruthy();
    expect(screen.queryByText("Rename")).toBeNull();
  });

  it("shows an empty state when nothing matches", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "zzzznomatch" } });

    expect(screen.getByText(/No shortcuts match/)).toBeTruthy();
  });
});

describe("KeyboardBindingsDialog close behavior (CPE-1548)", () => {
  it("dispatches close on Escape", async () => {
    const { component } = render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(true);
  });

  it("dispatches close on backdrop click but not on a click inside the dialog", async () => {
    const { component, container } = render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    let closed = false;
    component.$on("close", () => (closed = true));

    const dialog = container.querySelector(".dialog") as HTMLElement;
    await fireEvent.click(dialog);
    expect(closed).toBe(false);

    const backdrop = container.querySelector(".backdrop") as HTMLElement;
    await fireEvent.click(backdrop);
    expect(closed).toBe(true);
  });
});

describe("KeyboardBindingsDialog press-to-set rebind (CPE-1549)", () => {
  it("commits a non-colliding capture immediately and persists via saveKeymap", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });

    await fireEvent.click(screen.getByTestId("hotkey-capture-newTab"));
    await fireEvent.keyDown(window, { key: "n", ctrlKey: true, altKey: true, shiftKey: true });

    const row = screen.getByText("New tab").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+Alt+Shift+N")).toBeTruthy();
    expect(saveKeymap).toHaveBeenCalledTimes(1);
    const saved = vi.mocked(saveKeymap).mock.calls[0][0];
    expect(chordFor(saved, "newTab")).toBe("Ctrl+Alt+Shift+N");
  });

  it("does not commit a chord hotkeyFromEvent rejects (bare letter, no Ctrl/Alt) — no saveKeymap call", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });

    await fireEvent.click(screen.getByTestId("hotkey-capture-newTab"));
    await fireEvent.keyDown(window, { key: "n" });

    expect(saveKeymap).not.toHaveBeenCalled();
    expect(screen.getByText("Needs Ctrl or Alt…")).toBeTruthy();
  });
});

describe("KeyboardBindingsDialog live conflict warning (CPE-1549)", () => {
  function renderWithConflictSetup() {
    // "Copy" defaults to Ctrl+C. Rebinding "Cut" (default Ctrl+X) to Ctrl+C collides with it.
    const keymap = defaultKeymap();
    return render(KeyboardBindingsDialog, { keymap });
  }

  it("shows an inline warning naming the colliding action instead of silently applying the rebind", async () => {
    renderWithConflictSetup();

    await fireEvent.click(screen.getByTestId("hotkey-capture-cut"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });

    expect(screen.getByTestId("keyboard-binding-conflict-cut")).toBeTruthy();
    const conflict = screen.getByTestId("keyboard-binding-conflict-cut");
    expect(within(conflict).getByText(/Copy/)).toBeTruthy();
    // Not persisted yet — still pending user confirmation.
    expect(saveKeymap).not.toHaveBeenCalled();
  });

  it("'Cancel' leaves both bindings unchanged and dismisses the warning", async () => {
    renderWithConflictSetup();

    await fireEvent.click(screen.getByTestId("hotkey-capture-cut"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await fireEvent.click(screen.getByTestId("keyboard-binding-conflict-cancel-cut"));

    expect(screen.queryByTestId("keyboard-binding-conflict-cut")).toBeNull();
    expect(saveKeymap).not.toHaveBeenCalled();
    const cutRow = screen.getByText("Cut").closest(".row") as HTMLElement;
    const copyRow = screen.getByText("Copy").closest(".row") as HTMLElement;
    expect(within(cutRow).getByText("Ctrl+X")).toBeTruthy(); // still the original default
    expect(within(copyRow).getByText("Ctrl+C")).toBeTruthy(); // untouched
  });

  it("'Rebind anyway' applies the new chord and unbinds the other action — never a silent double-binding", async () => {
    renderWithConflictSetup();

    await fireEvent.click(screen.getByTestId("hotkey-capture-cut"));
    await fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    await fireEvent.click(screen.getByTestId("keyboard-binding-conflict-rebind-cut"));

    expect(screen.queryByTestId("keyboard-binding-conflict-cut")).toBeNull();
    expect(saveKeymap).toHaveBeenCalledTimes(1);
    const saved = vi.mocked(saveKeymap).mock.calls[0][0];
    expect(chordFor(saved, "cut")).toBe("Ctrl+C");
    expect(chordFor(saved, "copy")).toBe(""); // the loser is unbound, not silently sharing the chord

    const cutRow = screen.getByText("Cut").closest(".row") as HTMLElement;
    const copyRow = screen.getByText("Copy").closest(".row") as HTMLElement;
    expect(within(cutRow).getByText("Ctrl+C")).toBeTruthy();
    expect(within(copyRow).getByText("Click to set…")).toBeTruthy();
  });
});

describe("KeyboardBindingsDialog reset (CPE-1549)", () => {
  it("per-row Reset restores an override back to the built-in default and persists it", async () => {
    const keymap = setChord(defaultKeymap(), "newTab", "Ctrl+Alt+N");
    render(KeyboardBindingsDialog, { keymap });

    await fireEvent.click(screen.getByTestId("keyboard-binding-reset-newTab"));

    const row = screen.getByText("New tab").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+T")).toBeTruthy(); // built-in default
    expect(saveKeymap).toHaveBeenCalledTimes(1);
    const saved = vi.mocked(saveKeymap).mock.calls[0][0];
    expect(chordFor(saved, "newTab")).toBe("Ctrl+T");
  });

  it("'Reset all to defaults' restores every action and persists the fresh default map", async () => {
    let keymap = setChord(defaultKeymap(), "newTab", "Ctrl+Alt+N");
    keymap = setChord(keymap, "closeTab", "Ctrl+Alt+W");
    render(KeyboardBindingsDialog, { keymap });

    await fireEvent.click(screen.getByTestId("keyboard-bindings-reset-all"));

    const newTabRow = screen.getByText("New tab").closest(".row") as HTMLElement;
    const closeTabRow = screen.getByText("Close tab").closest(".row") as HTMLElement;
    expect(within(newTabRow).getByText("Ctrl+T")).toBeTruthy();
    expect(within(closeTabRow).getByText("Ctrl+W")).toBeTruthy();
    expect(saveKeymap).toHaveBeenCalledTimes(1);
    const saved = vi.mocked(saveKeymap).mock.calls[0][0];
    expect(saved).toEqual(defaultKeymap());
  });
});

describe("KeyboardBindingsDialog capture Escape doesn't also close the dialog (CPE-1549)", () => {
  it("Escape while armed cancels the capture only — the dialog stays open", async () => {
    const { component } = render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("hotkey-capture-newTab"));
    expect(screen.getByText("Press a key…")).toBeTruthy();

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(closed).toBe(false);
    expect(screen.queryByText("Press a key…")).toBeNull();
    const row = screen.getByText("New tab").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+T")).toBeTruthy(); // unchanged

    // A second, unarmed Escape still closes the dialog normally.
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(true);
  });
});

describe("KeyboardBindingsDialog Import / Export (CPE-1550)", () => {
  beforeEach(() => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
  });

  it("the Import / Export section is collapsed until toggled", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    expect(screen.queryByTestId("keymap-export-textarea")).toBeNull();

    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));
    expect(screen.getByTestId("keymap-export-textarea")).toBeTruthy();
  });

  it("the export textarea shows the current keymap's exportKeymap() JSON", async () => {
    const keymap = setChord(defaultKeymap(), "newTab", "Ctrl+Alt+N");
    render(KeyboardBindingsDialog, { keymap });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    const textarea = screen.getByTestId("keymap-export-textarea") as HTMLTextAreaElement;
    expect(textarea.value).toBe(exportKeymap(keymap));
  });

  it("'Copy to clipboard' writes the exported JSON via navigator.clipboard.writeText", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    await fireEvent.click(screen.getByTestId("keymap-export-copy-btn"));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(exportKeymap(keymap));
    expect(screen.getByTestId("keymap-io-note").textContent).toMatch(/Copied/);
  });

  it("importing valid pasted JSON applies it via saveKeymap and shows an applied-count summary", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    const payload = JSON.stringify({ version: 1, bindings: { copy: "Ctrl+Alt+C" } });
    await fireEvent.input(screen.getByTestId("keymap-import-textarea"), { target: { value: payload } });
    await fireEvent.click(screen.getByTestId("keymap-import-btn"));

    expect(saveKeymap).toHaveBeenCalledTimes(1);
    const saved = vi.mocked(saveKeymap).mock.calls[0][0];
    expect(chordFor(saved, "copy")).toBe("Ctrl+Alt+C");
    expect(screen.getByTestId("keymap-io-note").textContent).toMatch(/Applied 1/);

    const row = screen.getByText("Copy").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+Alt+C")).toBeTruthy();
  });

  it("importing JSON with an unrecognized action id applies the rest and reports the skip count", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    const payload = JSON.stringify({ bindings: { copy: "Ctrl+Alt+C", notARealAction: "Ctrl+Alt+Z" } });
    await fireEvent.input(screen.getByTestId("keymap-import-textarea"), { target: { value: payload } });
    await fireEvent.click(screen.getByTestId("keymap-import-btn"));

    expect(saveKeymap).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("keymap-io-note").textContent).toMatch(/Applied 1, skipped 1/);
  });

  it("importing malformed JSON shows a validation error and never calls saveKeymap", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    await fireEvent.input(screen.getByTestId("keymap-import-textarea"), {
      target: { value: "not valid json {{{" },
    });
    await fireEvent.click(screen.getByTestId("keymap-import-btn"));

    expect(saveKeymap).not.toHaveBeenCalled();
    expect(screen.getByTestId("keymap-io-error")).toBeTruthy();
  });

  it("the Import button is disabled with empty/whitespace-only pasted text", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.click(screen.getByTestId("keymap-io-toggle"));

    expect((screen.getByTestId("keymap-import-btn") as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.input(screen.getByTestId("keymap-import-textarea"), { target: { value: "   " } });
    expect((screen.getByTestId("keymap-import-btn") as HTMLButtonElement).disabled).toBe(true);
  });
});
