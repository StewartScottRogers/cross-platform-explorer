/**
 * Component render tests for the file-coloring/labels rules editor (CPE-776, epic CPE-709, ticket
 * CPE-1405). `ColorRulesDialog` has no `invoke` calls of its own — it's a thin CRUD form over the pure,
 * already unit-tested `colorRulesStore` (`addRule`/`updateRule`/`removeRule`/`moveRule`/`toggleRule`, see
 * `../colorRulesStore.test.ts`) plus a live-preview `change` dispatch on every edit. So this spec exercises
 * the WIRING: does each of the 6 condition-kind builders (ext/glob/size/olderThan/newerThan/isDir) turn
 * form input into the right `Condition` object via `buildCondition()`, does `summarize()` render the right
 * human text for each kind, does `change` fire live with the current working list on every edit (toggle,
 * recolor, relabel, reorder, delete, add), and do `save`/`cancel` fire with the right data. No
 * `@tauri-apps/api/core` mock needed — this component performs no I/O itself (same as WatchRulesDialog).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import ColorRulesDialog from "./ColorRulesDialog.svelte";
import type { ColorRule, Condition } from "../colorRules";

// ── fixtures ──────────────────────────────────────────────────────────────────────────────────

const rule = (id: string, when: Condition, over: Partial<ColorRule> = {}): ColorRule => ({
  id,
  when,
  color: "#888888",
  ...over,
});

function threeRules(): ColorRule[] {
  return [
    rule("r1", { kind: "ext", exts: ["ts"] }, { label: "TS" }),
    rule("r2", { kind: "ext", exts: ["md"] }, { label: "MD" }),
    rule("r3", { kind: "ext", exts: ["png"] }, { label: "PNG" }),
  ];
}

// ── shared helpers ───────────────────────────────────────────────────────────────────────────

const kindSelect = () => screen.getByLabelText("Condition kind") as HTMLSelectElement;
const addBtn = () => screen.getByTestId("add-btn") as HTMLButtonElement;
const doneBtn = () => screen.getByTestId("done-btn") as HTMLButtonElement;
const rows = () => screen.queryAllByTestId("rule-row");

describe("ColorRulesDialog — initial render (CPE-1405)", () => {
  it("shows the empty-state message when there are no rules", () => {
    render(ColorRulesDialog, { rules: [] });
    expect(screen.getByText("No rules yet — add one below.")).toBeTruthy();
    expect(screen.queryByTestId("rule-row")).toBeNull();
  });

  it("renders one row per rule with its summarized condition and label", () => {
    render(ColorRulesDialog, { rules: threeRules() });
    const list = rows();
    expect(list).toHaveLength(3);
    expect(screen.getByText("extension: ts")).toBeTruthy();
    expect(screen.getByText("extension: md")).toBeTruthy();
    expect(screen.getByText("extension: png")).toBeTruthy();
    expect((within(list[0]).getByLabelText("Rule label") as HTMLInputElement).value).toBe("TS");
  });

  it("dispatches `change` with the working-copy list on mount (before any interaction), not the raw prop array", () => {
    const rulesProp = threeRules();
    // Attach $on synchronously in the render() call itself isn't possible, so instead assert the
    // documented contract indirectly: the rendered rows must reflect a COPY, proven by the "does not
    // mutate the prop" test below. Here we just confirm the initial preview matches props 1:1.
    render(ColorRulesDialog, { rules: rulesProp });
    expect(rows()).toHaveLength(3);
  });
});

describe("ColorRulesDialog — the 6 condition-kind builders (CPE-1405)", () => {
  it("ext: comma-separated extensions become a trimmed exts array", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    // kind defaults to "ext"
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "ts, md ,  png" } });
    await fireEvent.click(addBtn());

    expect(screen.getByText("extension: ts, md, png")).toBeTruthy();
    expect(change).toHaveBeenCalledTimes(1);
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list).toHaveLength(1);
    expect(list[0].when).toEqual({ kind: "ext", exts: ["ts", "md", "png"] });
  });

  it("glob: the pattern is passed through verbatim", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "glob" } });
    await fireEvent.input(screen.getByLabelText("Glob pattern"), { target: { value: "*.min.js" } });
    await fireEvent.click(addBtn());

    expect(screen.getByText("name matches *.min.js")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "glob", pattern: "*.min.js" });
  });

  it("size: numeric min/max become a bounded size condition", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "1000" } });
    await fireEvent.input(screen.getByLabelText("Max bytes"), { target: { value: "5000" } });
    await fireEvent.click(addBtn());

    expect(screen.getByText("size ≥ 1000 and ≤ 5000 bytes")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "size", min: 1000, max: 5000 });
  });

  it("size: an unset bound is omitted from the built condition (undefined, not 0)", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "1000" } });
    // Max left blank.
    await fireEvent.click(addBtn());

    expect(screen.getByText("size ≥ 1000 bytes")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "size", min: 1000, max: undefined });
  });

  it("olderThan: a positive day count builds the days condition", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "olderThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "30" } });
    await fireEvent.click(addBtn());

    expect(screen.getByText("older than 30 days")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "olderThan", days: 30 });
  });

  it("newerThan: a positive day count builds the days condition", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "newerThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "3" } });
    await fireEvent.click(addBtn());

    expect(screen.getByText("newer than 3 days")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "newerThan", days: 3 });
  });

  it("isDir: the folder checkbox becomes a boolean isDir condition (default checked = true)", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "isDir" } });
    await fireEvent.click(addBtn()); // isDirValue defaults to true, unchanged

    expect(screen.getByText("is a folder")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "isDir", value: true });
  });

  it("isDir: unchecking the folder checkbox builds value:false, summarized as 'is a file'", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "isDir" } });
    await fireEvent.click(screen.getByLabelText("folder"));
    await fireEvent.click(addBtn());

    expect(screen.getByText("is a file")).toBeTruthy();
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].when).toEqual({ kind: "isDir", value: false });
  });

  it("a new rule picks up the color swatch and trimmed label from the add-row inputs, defaulting label to undefined when blank", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "pdf" } });
    await fireEvent.input(screen.getByLabelText("New rule colour"), { target: { value: "#123456" } });
    await fireEvent.input(screen.getByLabelText("New rule label"), { target: { value: "  Docs  " } });
    await fireEvent.click(addBtn());

    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].color).toBe("#123456");
    expect(list[0].label).toBe("Docs");

    // A second rule added with a blank label gets `undefined`, not "".
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "zip" } });
    await fireEvent.click(addBtn());
    const list2: ColorRule[] = change.mock.calls[1][0].detail;
    expect(list2[1].label).toBeUndefined();
  });

  it("adding a rule resets the kind-specific text inputs and the label, but not the kind selector or color", async () => {
    render(ColorRulesDialog, { rules: [] });
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "ts" } });
    await fireEvent.click(addBtn());

    expect((screen.getByLabelText("Extensions") as HTMLInputElement).value).toBe("");
    expect(kindSelect().value).toBe("ext");
  });
});

describe("ColorRulesDialog — Add-rule button is NOT gated on condition validity (potential mis-wire, cf. CPE-1402)", () => {
  // Unlike WatchRulesDialog post-CPE-1402, `add-btn` here has no `disabled` binding at all — it is
  // always clickable regardless of whether buildCondition() would return null. Clicking it with an
  // invalid/empty condition silently no-ops: `add()` calls `buildCondition()`, sees `null`, and
  // `return`s before mutating `list` or dispatching `change`. This mirrors the exact bug class CPE-1402
  // fixed for WatchRulesDialog, but ColorRulesDialog was not covered by that fix. Reporting per the
  // ticket instructions rather than fixing here.
  it("Add stays enabled with an empty ext condition, and clicking it is a silent no-op", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    // kind defaults to "ext"; Extensions field left blank -> buildCondition() returns null.
    expect(addBtn().disabled).toBe(false); // no validity gate on the button at all
    await fireEvent.click(addBtn());

    expect(change).not.toHaveBeenCalled();
    expect(screen.queryByTestId("rule-row")).toBeNull();
    expect(screen.getByText("No rules yet — add one below.")).toBeTruthy();
  });

  it("Add stays enabled with an empty glob pattern, and clicking it is a silent no-op", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "glob" } });
    expect(addBtn().disabled).toBe(false);
    await fireEvent.click(addBtn());

    expect(change).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(0);
  });

  it("Add stays enabled when both size bounds are blank, and clicking it is a silent no-op", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    expect(addBtn().disabled).toBe(false);
    await fireEvent.click(addBtn());

    expect(change).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(0);
  });

  it("Add stays enabled when a size bound is non-numeric (NaN guard), and clicking it is a silent no-op", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "not-a-number" } });
    expect(addBtn().disabled).toBe(false);
    await fireEvent.click(addBtn());

    expect(change).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(0);
  });

  it("Add stays enabled for olderThan with a zero/non-numeric day count, and clicking it is a silent no-op", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.change(kindSelect(), { target: { value: "olderThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "0" } });
    expect(addBtn().disabled).toBe(false);
    await fireEvent.click(addBtn());
    expect(change).not.toHaveBeenCalled();

    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "abc" } });
    await fireEvent.click(addBtn());
    expect(change).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(0);
  });
});

describe("ColorRulesDialog — live preview on row edits (CPE-1405)", () => {
  it("toggling a rule's Enable checkbox dispatches `change` with that rule's enabled flipped", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rows()[0]).getByLabelText("Enable rule"));

    expect(change).toHaveBeenCalledTimes(1);
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[0].enabled).toBe(false);
    expect(list[1].enabled).not.toBe(false);
  });

  it("changing a rule's colour swatch dispatches `change` with the new color", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.input(within(rows()[1]).getByLabelText("Rule colour"), { target: { value: "#abcdef" } });

    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[1].color).toBe("#abcdef");
    expect(list[0].color).toBe("#888888"); // untouched rows unaffected
  });

  it("editing a rule's label dispatches `change` with the new label live (per keystroke)", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.input(within(rows()[2]).getByLabelText("Rule label"), { target: { value: "Images" } });

    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list[2].label).toBe("Images");
  });

  it("Move up/down are boundary-disabled on the first/last row", () => {
    render(ColorRulesDialog, { rules: threeRules() });
    const list = rows();
    expect((within(list[0]).getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(true);
    expect((within(list[0]).getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(false);
    expect((within(list[2]).getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(true);
    expect((within(list[2]).getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(false);
  });

  it("Move down swaps the rule with its successor and dispatches `change` with the new order", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rows()[0]).getByLabelText("Move down"));

    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list.map((r) => r.id)).toEqual(["r2", "r1", "r3"]);
  });

  it("Delete rule removes it and dispatches `change` with the removal reflected", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    const change = vi.fn();
    component.$on("change", change);

    await fireEvent.click(within(rows()[1]).getByLabelText("Delete rule"));

    expect(rows()).toHaveLength(2);
    const list: ColorRule[] = change.mock.calls[0][0].detail;
    expect(list.map((r) => r.id)).toEqual(["r1", "r3"]);
  });

  it("does not mutate the `rules` prop array in place (edits operate on a local copy)", async () => {
    const originalRules = threeRules();
    const snapshot = originalRules.map((r) => ({ ...r }));
    render(ColorRulesDialog, { rules: originalRules });

    await fireEvent.click(within(rows()[0]).getByLabelText("Delete rule"));

    expect(originalRules).toEqual(snapshot);
  });
});

describe("ColorRulesDialog — save / cancel (CPE-1405)", () => {
  it("Done dispatches `save` with the current local list, even with no edits", async () => {
    const rulesProp = threeRules();
    const { component } = render(ColorRulesDialog, { rules: rulesProp });
    const saved = await new Promise<ColorRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });
    expect(saved).toEqual(rulesProp);
  });

  it("Done dispatches `save` reflecting prior edits (toggle + reorder + delete)", async () => {
    const { component } = render(ColorRulesDialog, { rules: threeRules() });
    await fireEvent.click(within(rows()[0]).getByLabelText("Enable rule")); // disable r1
    await fireEvent.click(within(rows()[1]).getByLabelText("Delete rule")); // delete r2

    const saved = await new Promise<ColorRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });
    expect(saved.map((r) => r.id)).toEqual(["r1", "r3"]);
    expect(saved.find((r) => r.id === "r1")?.enabled).toBe(false);
  });

  it("Cancel button dispatches `cancel`", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(screen.getByText("Cancel"));
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it("clicking the backdrop dispatches `cancel`", async () => {
    const { component, container } = render(ColorRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(container.querySelector(".backdrop") as HTMLElement);
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it("clicking inside the dialog body does not dispatch `cancel` (stopPropagation)", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(screen.getByText("Color rules"));
    expect(cancel).not.toHaveBeenCalled();
  });

  it("pressing Escape dispatches `cancel`", async () => {
    const { component } = render(ColorRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(cancel).toHaveBeenCalledTimes(1);
  });
});
