/**
 * Component render tests for the user-command manager (CPE-783, epic CPE-711, ticket CPE-1409).
 * `UserCommandsDialog` makes no Tauri calls of its own — it's a thin CRUD form over the pure, already
 * unit-tested `userCommands` store (`addCommand`/`updateCommand`/`removeCommand`/`moveCommand`); every
 * mutation goes through `commit()`, which reassigns the local `commands` copy AND dispatches `change`
 * with the whole new list. No `@tauri-apps/api/core` mock needed — this component performs no I/O.
 * This spec exercises: the add/edit form's Save gating (blank name / blank template, each independently
 * and combined, with whitespace-only treated as blank), `toggleSurface` flipping the right surface flag
 * (including the "all surfaces unchecked" fallback to `["context"]` baked into `saveForm`), `startEdit`
 * pre-filling the form from an existing command, row actions (move/remove) committing the right list, and
 * the `change`/`close` event payloads.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import UserCommandsDialog from "./UserCommandsDialog.svelte";
import type { UserCommand } from "../userCommands";

// ── fixtures ──────────────────────────────────────────────────────────────────────────────────

const cmd = (name: string, template: string, over: Partial<UserCommand> = {}): UserCommand => ({
  id: `c_${name}`,
  name,
  template,
  mode: "each",
  surfaces: ["context"],
  ...over,
});

// ── shared helpers ───────────────────────────────────────────────────────────────────────────

const nameInput = () => screen.getByLabelText("Name") as HTMLInputElement;
const tplInput = () => screen.getByLabelText("Command template") as HTMLInputElement;
const saveBtn = () => screen.getByText("Save") as HTMLButtonElement;
const cancelBtn = () => screen.getByText("Cancel") as HTMLButtonElement;
const rowFor = (name: string) => screen.getByText(name).closest(".row") as HTMLElement;

describe("UserCommandsDialog — empty state", () => {
  it("shows the empty-state message when there are no commands and no editor open", () => {
    render(UserCommandsDialog, { commands: [] });
    expect(screen.getByText(/No commands yet/)).toBeTruthy();
  });

  it("hides the empty-state message while the add editor is open, even with zero commands", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    expect(screen.queryByText(/No commands yet/)).toBeNull();
  });
});

describe("UserCommandsDialog — row rendering", () => {
  it("renders each command's name, template, mode pill, and surface pills", () => {
    const commands = [cmd("Open in VS Code", "code {path}", { mode: "joined", surfaces: ["toolbar", "palette"] })];
    render(UserCommandsDialog, { commands });

    const row = rowFor("Open in VS Code");
    expect(within(row).getByText("code {path}")).toBeTruthy();
    expect(within(row).getByText("joined")).toBeTruthy();
    expect(within(row).getByText("toolbar")).toBeTruthy();
    expect(within(row).getByText("palette")).toBeTruthy();
    expect(within(row).queryByText("context")).toBeNull();
  });

  it("boundary-disables Move up on the first row and Move down on the last row", () => {
    const commands = [cmd("First", "a {path}"), cmd("Second", "b {path}"), cmd("Third", "c {path}")];
    render(UserCommandsDialog, { commands });

    expect((within(rowFor("First")).getByTitle("Move up") as HTMLButtonElement).disabled).toBe(true);
    expect((within(rowFor("First")).getByTitle("Move down") as HTMLButtonElement).disabled).toBe(false);
    expect((within(rowFor("Third")).getByTitle("Move down") as HTMLButtonElement).disabled).toBe(true);
    expect((within(rowFor("Third")).getByTitle("Move up") as HTMLButtonElement).disabled).toBe(false);
  });
});

describe("UserCommandsDialog — move/remove row actions commit + dispatch `change`", () => {
  it("Move down swaps the row with its successor and dispatches the reordered list", async () => {
    const commands = [cmd("First", "a {path}"), cmd("Second", "b {path}"), cmd("Third", "c {path}")];
    const { component } = render(UserCommandsDialog, { commands });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rowFor("First")).getByTitle("Move down"));

    expect(change).toHaveBeenCalledTimes(1);
    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload.map((c) => c.name)).toEqual(["Second", "First", "Third"]);
  });

  it("Remove deletes the row and dispatches the filtered list", async () => {
    const commands = [cmd("First", "a {path}"), cmd("Second", "b {path}")];
    const { component } = render(UserCommandsDialog, { commands });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rowFor("First")).getByTitle("Remove"));

    expect(change).toHaveBeenCalledTimes(1);
    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload.map((c) => c.name)).toEqual(["Second"]);
    expect(screen.queryByText("First")).toBeNull();
  });
});

describe("UserCommandsDialog — add-form Save gating (CPE-1409)", () => {
  it("Save is disabled when both name and template are blank", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    expect(saveBtn().disabled).toBe(true);
  });

  it("Save stays disabled with a name but a blank template", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(nameInput(), { target: { value: "Open in VS Code" } });
    expect(saveBtn().disabled).toBe(true);
  });

  it("Save stays disabled with a template but a blank name", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(tplInput(), { target: { value: "code {path}" } });
    expect(saveBtn().disabled).toBe(true);
  });

  it("Save stays disabled when name and template are whitespace-only (trimmed)", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(nameInput(), { target: { value: "   " } });
    await fireEvent.input(tplInput(), { target: { value: "   " } });
    expect(saveBtn().disabled).toBe(true);
  });

  it("Save enables once both name and template are non-blank", async () => {
    render(UserCommandsDialog, { commands: [] });
    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(nameInput(), { target: { value: "Open in VS Code" } });
    await fireEvent.input(tplInput(), { target: { value: "code {path}" } });
    expect(saveBtn().disabled).toBe(false);
  });
});

describe("UserCommandsDialog — saving the add form commits + dispatches `change` (CPE-1409)", () => {
  it("Save adds a new command with the entered name/template, default mode 'each', default surfaces ['context', 'palette'] (CPE-1577), and closes the editor", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(nameInput(), { target: { value: "Open in VS Code" } });
    await fireEvent.input(tplInput(), { target: { value: "code {path}" } });
    await fireEvent.click(saveBtn());

    expect(change).toHaveBeenCalledTimes(1);
    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload).toHaveLength(1);
    expect(payload[0]).toMatchObject({ name: "Open in VS Code", template: "code {path}", mode: "each", surfaces: ["context", "palette"] });

    // Editor closed (form fields no longer rendered) and the new row shows.
    expect(screen.queryByLabelText("Name")).toBeNull();
    expect(rowFor("Open in VS Code")).toBeTruthy();
  });

  it("saving the 'once (joined)' radio produces mode: 'joined' in the dispatched command", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.input(nameInput(), { target: { value: "Joined cmd" } });
    await fireEvent.input(tplInput(), { target: { value: "cmd {paths}" } });
    await fireEvent.click(screen.getByLabelText("once (joined)"));
    await fireEvent.click(saveBtn());

    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload[0].mode).toBe("joined");
  });
});

describe("UserCommandsDialog — toggleSurface (CPE-1409, defaults updated for CPE-1577)", () => {
  it("checking an additional surface appends it, preserving the defaults 'context' + 'palette'", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    // Default fSurfaces is ["context", "palette"] (CPE-1577: not invisible out of the box) — assert
    // the checkboxes reflect that before touching anything.
    expect((screen.getByLabelText("context") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("toolbar") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("palette") as HTMLInputElement).checked).toBe(true);

    await fireEvent.click(screen.getByLabelText("toolbar"));
    await fireEvent.input(nameInput(), { target: { value: "N" } });
    await fireEvent.input(tplInput(), { target: { value: "T" } });
    await fireEvent.click(saveBtn());

    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload[0].surfaces).toEqual(["context", "palette", "toolbar"]);
  });

  it("unchecking a surface removes it", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.click(screen.getByLabelText("toolbar")); // -> ["context", "palette", "toolbar"]
    await fireEvent.click(screen.getByLabelText("context")); // -> ["palette", "toolbar"]
    await fireEvent.input(nameInput(), { target: { value: "N" } });
    await fireEvent.input(tplInput(), { target: { value: "T" } });
    await fireEvent.click(saveBtn());

    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload[0].surfaces).toEqual(["palette", "toolbar"]);
  });

  it("unchecking every surface falls back to the default ['context', 'palette'] at save time (saveForm's empty-surfaces guard)", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(screen.getByRole("button", { name: "+ New command" }));
    await fireEvent.click(screen.getByLabelText("context")); // -> ["palette"]
    await fireEvent.click(screen.getByLabelText("palette")); // -> [] — every default-checked one now off
    expect((screen.getByLabelText("context") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("toolbar") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("palette") as HTMLInputElement).checked).toBe(false);

    await fireEvent.input(nameInput(), { target: { value: "N" } });
    await fireEvent.input(tplInput(), { target: { value: "T" } });
    // Save stays enabled — the surfaces fallback lives in saveForm, not in the disabled binding, which
    // only gates on name/template.
    expect(saveBtn().disabled).toBe(false);
    await fireEvent.click(saveBtn());

    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload[0].surfaces).toEqual(["context", "palette"]);
  });
});

describe("UserCommandsDialog — startEdit pre-fill (CPE-1409)", () => {
  it("Edit pre-fills name, template, mode radio, and surface checkboxes from the existing command", async () => {
    const existing = cmd("Grep here", "rg {path}", { mode: "joined", surfaces: ["toolbar", "palette"] });
    render(UserCommandsDialog, { commands: [existing] });

    await fireEvent.click(within(rowFor("Grep here")).getByTitle("Edit"));

    expect(nameInput().value).toBe("Grep here");
    expect(tplInput().value).toBe("rg {path}");
    expect((screen.getByLabelText("once (joined)") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("once per item") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("toolbar") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("palette") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("context") as HTMLInputElement).checked).toBe(false);
  });

  it("saving an edit patches the existing command in place (same id, other commands untouched) via updateCommand", async () => {
    const other = cmd("Other", "other {path}");
    const existing = cmd("Grep here", "rg {path}", { mode: "each", surfaces: ["context"] });
    const { component } = render(UserCommandsDialog, { commands: [other, existing] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rowFor("Grep here")).getByTitle("Edit"));
    await fireEvent.input(nameInput(), { target: { value: "Ripgrep here" } });
    await fireEvent.input(tplInput(), { target: { value: "rg -n {path}" } });
    await fireEvent.click(screen.getByLabelText("once (joined)"));
    await fireEvent.click(saveBtn());

    expect(change).toHaveBeenCalledTimes(1);
    const payload: UserCommand[] = change.mock.calls[0][0].detail;
    expect(payload).toHaveLength(2);
    expect(payload.find((c) => c.name === "Other")).toEqual(other);
    const edited = payload.find((c) => c.id === existing.id);
    expect(edited).toMatchObject({ id: existing.id, name: "Ripgrep here", template: "rg -n {path}", mode: "joined", surfaces: ["context"] });
  });

  it("Cancel discards in-progress edits without dispatching `change`", async () => {
    const existing = cmd("Grep here", "rg {path}");
    const { component } = render(UserCommandsDialog, { commands: [existing] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rowFor("Grep here")).getByTitle("Edit"));
    await fireEvent.input(nameInput(), { target: { value: "Something else entirely" } });
    await fireEvent.click(cancelBtn());

    expect(change).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Name")).toBeNull();
    expect(rowFor("Grep here")).toBeTruthy();
    expect(screen.queryByText("Something else entirely")).toBeNull();
  });
});

describe("UserCommandsDialog — close (CPE-1409)", () => {
  it("the close (x) button dispatches `close`", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByTitle("Close"));
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("clicking the backdrop dispatches `close`", async () => {
    const { component, container } = render(UserCommandsDialog, { commands: [] });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(container.querySelector(".backdrop") as HTMLElement);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("clicking inside the dialog body does not dispatch `close` (stopPropagation)", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.click(screen.getByText("User commands"));
    expect(close).not.toHaveBeenCalled();
  });

  it("pressing Escape dispatches `close`", async () => {
    const { component } = render(UserCommandsDialog, { commands: [] });
    const close = vi.fn();
    component.$on("close", close);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(close).toHaveBeenCalledTimes(1);
  });
});
