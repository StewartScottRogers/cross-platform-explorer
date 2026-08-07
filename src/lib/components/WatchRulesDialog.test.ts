/**
 * Component render tests for the watched-folder rules editor (CPE-795, epic CPE-734, ticket CPE-1400).
 * `WatchRulesDialog` has no `invoke` calls of its own — it's a thin CRUD form over the pure, already
 * unit-tested `watchRules` store (`addRule`/`removeRule`/`setRuleEnabled`/`moveRule`/`planForEntry`, see
 * `../watchRules.test.ts`) plus a reactive dry-run preview. So this spec exercises the WIRING: does each
 * of the 5 condition-kind builders (ext/glob/size/older-newer/isDir) turn form input into the right
 * `Condition` object, does the NaN/empty guard actually block a bad rule from being added, does the
 * dry-run preview track `list`/`sampleName` reactively, do watch-folder add/remove/toggle call
 * `watchConfig` with the right payload, and do `save`/`cancel`/`undo` fire with the right data. No
 * `@tauri-apps/api/core` mock needed — this component performs no I/O itself.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import WatchRulesDialog from "./WatchRulesDialog.svelte";
import type { WatchRule } from "../watchRules";
import type { Condition } from "../colorRules";
import type { WatchFire } from "../folderWatch";

// ── fixtures ──────────────────────────────────────────────────────────────────────────────────

const rule = (name: string, when: Condition, over: Partial<WatchRule> = {}): WatchRule => ({
  id: `r_${name}`,
  name,
  when,
  actions: [{ kind: "move", dest: "/dest" }],
  enabled: true,
  ...over,
});

const fire = (id: string, over: Partial<WatchFire> = {}): WatchFire => ({
  id,
  rule: "Archive PDFs",
  source: "/dl/a.pdf",
  finalPath: "/archive/a.pdf",
  copies: [],
  summary: `Archive PDFs: a.pdf → /archive/a.pdf`,
  ...over,
});

// ── shared helpers ───────────────────────────────────────────────────────────────────────────

const ruleNameInput = () => screen.getByLabelText("Rule name") as HTMLInputElement;
const kindSelect = () => screen.getByLabelText("Condition kind") as HTMLSelectElement;
const actKindSelect = () => screen.getByLabelText("Action kind") as HTMLSelectElement;
const actValueInput = () => screen.getByLabelText("Action value") as HTMLInputElement;
const addActionBtn = () => screen.getByTestId("add-action-btn") as HTMLButtonElement;
const addRuleBtn = () => screen.getByTestId("add-rule-btn") as HTMLButtonElement;
const doneBtn = () => screen.getByTestId("done-btn") as HTMLButtonElement;

/** Fill the action row and click "+ action" so `pending` has one action ready for Add rule. */
async function addPendingAction(kind: "move" | "copy" | "tag" | "rename" = "move", value = "/dest") {
  await fireEvent.change(actKindSelect(), { target: { value: kind } });
  await fireEvent.input(actValueInput(), { target: { value } });
  await fireEvent.click(addActionBtn());
}

describe("WatchRulesDialog — Add-rule enablement (CPE-1400)", () => {
  it("Add rule stays disabled with no name and no pending action", () => {
    render(WatchRulesDialog, { rules: [] });
    expect(addRuleBtn().disabled).toBe(true);
  });

  it("Add rule stays disabled with a pending action but a blank name", async () => {
    render(WatchRulesDialog, { rules: [] });
    await addPendingAction();
    expect(ruleNameInput().value).toBe("");
    expect(addRuleBtn().disabled).toBe(true);
  });

  it("Add rule stays disabled with a name but no pending action", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.input(ruleNameInput(), { target: { value: "Archive PDFs" } });
    expect(addRuleBtn().disabled).toBe(true);
  });

  it("Add rule enables once both a name and a pending action exist", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.input(ruleNameInput(), { target: { value: "Archive PDFs" } });
    await addPendingAction();
    expect(addRuleBtn().disabled).toBe(false);
  });

  it("a name typed with only whitespace does not enable Add rule (trimmed)", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.input(ruleNameInput(), { target: { value: "   " } });
    await addPendingAction();
    expect(addRuleBtn().disabled).toBe(true);
  });
});

describe("WatchRulesDialog — the Add-rule button does not validate the condition (CPE-1400 finding)", () => {
  // The `disabled` binding on data-testid="add-rule-btn" is `!ruleName.trim() || pending.length === 0`
  // only — it does NOT re-check `buildCondition()`. So a name + a pending action is enough to enable
  // the button even when the currently-selected condition kind is empty/NaN and would build to `null`.
  // Clicking it in that state is a harmless no-op (addTheRule bails on `!cond`), but the button itself
  // gives no visual signal that the click will do nothing. Documented here rather than "fixed" per the
  // ticket's do-not-fix instruction.
  it("clicking Add rule silently no-ops when the ext condition is empty, though the button is enabled", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.input(ruleNameInput(), { target: { value: "Archive PDFs" } });
    await addPendingAction();
    // kind defaults to "ext"; the Extensions field is left blank -> buildCondition() returns null.
    expect(addRuleBtn().disabled).toBe(false);

    await fireEvent.click(addRuleBtn());

    expect(screen.queryByTestId("watch-rule-row")).toBeNull();
    expect(screen.getByText("No rules yet.")).toBeTruthy();
  });

  it("clicking Add rule silently no-ops when the size condition's min/max are both blank", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Big files" } });
    await addPendingAction();

    await fireEvent.click(addRuleBtn());

    expect(screen.queryByTestId("watch-rule-row")).toBeNull();
  });

  it("clicking Add rule silently no-ops when a size bound is non-numeric (NaN guard)", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "not-a-number" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Big files" } });
    await addPendingAction();

    await fireEvent.click(addRuleBtn());

    expect(screen.queryByTestId("watch-rule-row")).toBeNull();
  });

  it("clicking Add rule silently no-ops for olderThan when days is non-numeric or non-positive (NaN/empty guard)", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "olderThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "abc" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Old files" } });
    await addPendingAction();
    await fireEvent.click(addRuleBtn());
    expect(screen.queryByTestId("watch-rule-row")).toBeNull();

    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "0" } }); // d > 0 required
    await fireEvent.click(addRuleBtn());
    expect(screen.queryByTestId("watch-rule-row")).toBeNull();
  });
});

describe("WatchRulesDialog — the 5 condition-kind builders (CPE-1400)", () => {
  it("ext: comma-separated extensions become a trimmed exts array", async () => {
    render(WatchRulesDialog, { rules: [] });
    // kind defaults to "ext".
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "pdf, zip ,  txt" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "PDFs" } });
    await addPendingAction("move", "/archive");
    await fireEvent.click(addRuleBtn());

    // Asserted via the rendered summary (condSummary reads `c.exts.join("/.")`), and independently via
    // the exact Condition object in the combined save-payload test below. Scope to the rules list: the
    // sample filename defaults to "invoice.pdf", which this rule also matches, so its name shows a
    // SECOND time in the dry-run preview below — querying unscoped would find two "PDFs" matches.
    const list = screen.getByTestId("watch-rules-list");
    expect(within(list).getByText("PDFs")).toBeTruthy();
    expect(within(list).getByText("when .pdf/.zip/.txt")).toBeTruthy();
  });

  it("glob: the pattern is passed through verbatim", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "glob" } });
    await fireEvent.input(screen.getByLabelText("Glob"), { target: { value: "*.tmp" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Temp files" } });
    await addPendingAction("tag", "cleanup");
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when *.tmp")).toBeTruthy();
  });

  it("size: numeric min/max become a bounded size condition", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "1000" } });
    await fireEvent.input(screen.getByLabelText("Max bytes"), { target: { value: "5000" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Mid-size files" } });
    await addPendingAction("rename", "{stem}-x.{ext}");
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when size 1000–5000b")).toBeTruthy();
  });

  it("size: an unset bound renders as unbounded (∞ / 0)", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "1000" } });
    // Max left blank.
    await fireEvent.input(ruleNameInput(), { target: { value: "Big files" } });
    await addPendingAction();
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when size 1000–∞b")).toBeTruthy();
  });

  it("olderThan: a positive day count builds the days condition", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "olderThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "30" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Old files" } });
    await addPendingAction("copy", "/backup");
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when >30d old")).toBeTruthy();
  });

  it("newerThan: a positive day count builds the days condition", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "newerThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "3" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "New files" } });
    await addPendingAction();
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when <3d old")).toBeTruthy();
  });

  it("isDir: the folder checkbox becomes a boolean isDir condition", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.change(kindSelect(), { target: { value: "isDir" } });
    await fireEvent.click(screen.getByLabelText("folder"));
    await fireEvent.input(ruleNameInput(), { target: { value: "Folders" } });
    await addPendingAction("tag", "folder");
    await fireEvent.click(addRuleBtn());

    expect(screen.getByText("when folders")).toBeTruthy();
  });

  it("saving after building each of the 5 kinds dispatches `save` with all 5 exact Condition objects", async () => {
    const { component } = render(WatchRulesDialog, { rules: [] });

    // ext
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "pdf,zip" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "R-ext" } });
    await addPendingAction("move", "/a");
    await fireEvent.click(addRuleBtn());

    // glob
    await fireEvent.change(kindSelect(), { target: { value: "glob" } });
    await fireEvent.input(screen.getByLabelText("Glob"), { target: { value: "*.tmp" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "R-glob" } });
    await addPendingAction("tag", "t");
    await fireEvent.click(addRuleBtn());

    // size
    await fireEvent.change(kindSelect(), { target: { value: "size" } });
    await fireEvent.input(screen.getByLabelText("Min bytes"), { target: { value: "10" } });
    await fireEvent.input(screen.getByLabelText("Max bytes"), { target: { value: "20" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "R-size" } });
    await addPendingAction("copy", "/b");
    await fireEvent.click(addRuleBtn());

    // olderThan
    await fireEvent.change(kindSelect(), { target: { value: "olderThan" } });
    await fireEvent.input(screen.getByLabelText("Days"), { target: { value: "7" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "R-older" } });
    await addPendingAction("rename", "{stem}.{ext}");
    await fireEvent.click(addRuleBtn());

    // isDir
    await fireEvent.change(kindSelect(), { target: { value: "isDir" } });
    await fireEvent.click(screen.getByLabelText("folder"));
    await fireEvent.input(ruleNameInput(), { target: { value: "R-isdir" } });
    await addPendingAction("tag", "d");
    await fireEvent.click(addRuleBtn());

    const saved = await new Promise<WatchRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });

    expect(saved).toHaveLength(5);
    expect(saved.map((r) => r.when)).toEqual([
      { kind: "ext", exts: ["pdf", "zip"] },
      { kind: "glob", pattern: "*.tmp" },
      { kind: "size", min: 10, max: 20 },
      { kind: "olderThan", days: 7 },
      { kind: "isDir", value: true },
    ]);
    expect(saved.map((r) => r.name)).toEqual(["R-ext", "R-glob", "R-size", "R-older", "R-isdir"]);
    expect(saved.every((r) => r.enabled === true)).toBe(true);
  });
});

describe("WatchRulesDialog — dry-run preview (CPE-1400)", () => {
  it("shows 'no rule matches' when the rule list is empty", () => {
    render(WatchRulesDialog, { rules: [] });
    expect(screen.getByTestId("dry-run-result").textContent).toContain("no rule matches");
  });

  it("defaults the sample filename to invoice.pdf and previews the first matching rule's resolved actions", async () => {
    const rules = [rule("Archive PDFs", { kind: "ext", exts: ["pdf"] }, { actions: [{ kind: "rename", template: "{stem}-x.{ext}" }] })];
    render(WatchRulesDialog, { rules });

    expect((screen.getByLabelText("Sample filename") as HTMLInputElement).value).toBe("invoice.pdf");
    await waitFor(() => {
      const text = screen.getByTestId("dry-run-result").textContent ?? "";
      expect(text).toContain("Archive PDFs");
      expect(text).toContain("invoice-x.pdf");
    });
  });

  it("re-evaluates reactively as the sample filename is edited", async () => {
    const rules = [rule("Only DOCX", { kind: "ext", exts: ["docx"] })];
    render(WatchRulesDialog, { rules });
    await waitFor(() => expect(screen.getByTestId("dry-run-result").textContent).toContain("no rule matches"));

    await fireEvent.input(screen.getByLabelText("Sample filename"), { target: { value: "report.docx" } });
    await waitFor(() => expect(screen.getByTestId("dry-run-result").textContent).toContain("Only DOCX"));

    await fireEvent.input(screen.getByLabelText("Sample filename"), { target: { value: "report.pdf" } });
    await waitFor(() => expect(screen.getByTestId("dry-run-result").textContent).toContain("no rule matches"));
  });

  it("re-evaluates reactively as a new rule is added through the builder (not just from the `rules` prop)", async () => {
    render(WatchRulesDialog, { rules: [] });
    await fireEvent.input(screen.getByLabelText("Sample filename"), { target: { value: "photo.jpg" } });
    await waitFor(() => expect(screen.getByTestId("dry-run-result").textContent).toContain("no rule matches"));

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "jpg" } });
    await fireEvent.input(ruleNameInput(), { target: { value: "Photos" } });
    await addPendingAction("tag", "photo");
    await fireEvent.click(addRuleBtn());

    await waitFor(() => {
      const text = screen.getByTestId("dry-run-result").textContent ?? "";
      expect(text).toContain("Photos");
      expect(text).toContain("photo"); // resolved tag action
    });
  });

  it("skips a disabled rule in the preview, matching the next enabled one", async () => {
    const rules = [
      rule("Disabled PDFs", { kind: "ext", exts: ["pdf"] }, { enabled: false, actions: [{ kind: "tag", tag: "disabled-hit" }] }),
      rule("Enabled PDFs", { kind: "ext", exts: ["pdf"] }, { actions: [{ kind: "tag", tag: "enabled-hit" }] }),
    ];
    render(WatchRulesDialog, { rules });
    await waitFor(() => {
      const text = screen.getByTestId("dry-run-result").textContent ?? "";
      expect(text).toContain("Enabled PDFs");
      expect(text).not.toContain("Disabled PDFs");
    });
  });
});

describe("WatchRulesDialog — watch-folder add/remove/toggle (CPE-1400)", () => {
  it("Add folder dispatches watchConfig with the folder appended, preserving `live`", async () => {
    const { component } = render(WatchRulesDialog, {
      rules: [],
      watchAvailable: true,
      watchedFolders: ["C:/watch1"],
      watchLive: true,
    });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    await fireEvent.input(screen.getByLabelText("Watch folder"), { target: { value: "C:/watch2" } });
    await fireEvent.click(screen.getByTestId("add-folder-btn"));

    expect(watchConfig).toHaveBeenCalledTimes(1);
    expect(watchConfig.mock.calls[0][0].detail).toEqual({ folders: ["C:/watch1", "C:/watch2"], live: true });
    // The input clears and the new folder renders.
    expect((screen.getByLabelText("Watch folder") as HTMLInputElement).value).toBe("");
    expect(screen.getAllByTestId("watched-folder")).toHaveLength(2);
  });

  it("Add folder via Enter key works the same as clicking the button", async () => {
    const { component } = render(WatchRulesDialog, { rules: [], watchAvailable: true });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    const input = screen.getByLabelText("Watch folder");
    await fireEvent.input(input, { target: { value: "C:/new" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(watchConfig).toHaveBeenCalledWith(expect.objectContaining({ detail: { folders: ["C:/new"], live: false } }));
  });

  it("does not add a duplicate folder and does not dispatch watchConfig for the no-op", async () => {
    const { component } = render(WatchRulesDialog, {
      rules: [],
      watchAvailable: true,
      watchedFolders: ["C:/watch1"],
    });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    await fireEvent.input(screen.getByLabelText("Watch folder"), { target: { value: "C:/watch1" } });
    await fireEvent.click(screen.getByTestId("add-folder-btn"));

    expect(watchConfig).not.toHaveBeenCalled();
    expect(screen.getAllByTestId("watched-folder")).toHaveLength(1);
  });

  it("does not add a blank/whitespace-only folder", async () => {
    const { component } = render(WatchRulesDialog, { rules: [], watchAvailable: true });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    await fireEvent.input(screen.getByLabelText("Watch folder"), { target: { value: "   " } });
    await fireEvent.click(screen.getByTestId("add-folder-btn"));

    expect(watchConfig).not.toHaveBeenCalled();
    expect(screen.queryByTestId("watched-folder")).toBeNull();
  });

  it("Unwatch removes the folder and dispatches watchConfig without it", async () => {
    const { component } = render(WatchRulesDialog, {
      rules: [],
      watchAvailable: true,
      watchedFolders: ["C:/watch1", "C:/watch2"],
      watchLive: false,
    });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    const rows = screen.getAllByTestId("watched-folder");
    expect(rows).toHaveLength(2);
    await fireEvent.click(within(rows[0]).getByLabelText("Unwatch"));

    expect(watchConfig).toHaveBeenCalledTimes(1);
    expect(watchConfig.mock.calls[0][0].detail).toEqual({ folders: ["C:/watch2"], live: false });
    expect(screen.getAllByTestId("watched-folder")).toHaveLength(1);
  });

  it("toggling the live-watch checkbox flips `live` and dispatches watchConfig", async () => {
    const { component } = render(WatchRulesDialog, {
      rules: [],
      watchAvailable: true,
      watchedFolders: ["C:/watch1"],
      watchLive: false,
    });
    const watchConfig = vi.fn();
    component.$on("watchConfig", watchConfig);

    await fireEvent.click(screen.getByTestId("watch-live-toggle"));

    expect(watchConfig).toHaveBeenCalledTimes(1);
    expect(watchConfig.mock.calls[0][0].detail).toEqual({ folders: ["C:/watch1"], live: true });
  });

  it("hides the live-watch controls and shows the sidecar-required message when watchAvailable is false", () => {
    render(WatchRulesDialog, { rules: [], watchAvailable: false });

    expect(screen.getByText("needs the sidecar build (the shipping app)")).toBeTruthy();
    expect(screen.queryByTestId("watch-live-toggle")).toBeNull();
    expect(screen.queryByTestId("add-folder-btn")).toBeNull();
    expect(screen.queryByLabelText("Watch folder")).toBeNull();
  });
});

describe("WatchRulesDialog — watch activity log + undo (CPE-1400)", () => {
  it("renders each fire's summary and lets Undo dispatch the exact WatchFire object", async () => {
    const fires = [fire("f1", { summary: "Archive PDFs: a.pdf → /archive/a.pdf" }), fire("f2", { summary: "Tag temp: b.tmp → tagged" })];
    const { component } = render(WatchRulesDialog, { rules: [], watchAvailable: true, watchLog: fires });
    const undo = vi.fn();
    component.$on("undo", undo);

    const lines = screen.getAllByTestId("watch-log-line");
    expect(lines).toHaveLength(2);
    expect(screen.getByText(/Archive PDFs: a\.pdf/)).toBeTruthy();
    expect(screen.getByText(/Tag temp: b\.tmp/)).toBeTruthy();

    await fireEvent.click(within(lines[1]).getByTestId("undo-btn"));

    expect(undo).toHaveBeenCalledTimes(1);
    expect(undo.mock.calls[0][0].detail).toEqual(fires[1]);
  });

  it("shows at most the first 6 log lines even when more fires are supplied", () => {
    const fires = Array.from({ length: 9 }, (_, i) => fire(`f${i}`, { summary: `fire ${i}` }));
    render(WatchRulesDialog, { rules: [], watchAvailable: true, watchLog: fires });
    expect(screen.getAllByTestId("watch-log-line")).toHaveLength(6);
  });

  it("renders no log section when watchLog is empty", () => {
    render(WatchRulesDialog, { rules: [], watchAvailable: true, watchLog: [] });
    expect(screen.queryByTestId("watch-log-line")).toBeNull();
  });
});

describe("WatchRulesDialog — rule row controls: enable/disable, reorder, delete (CPE-1400)", () => {
  function threeRules(): WatchRule[] {
    return [
      rule("First", { kind: "ext", exts: ["a"] }),
      rule("Second", { kind: "ext", exts: ["b"] }),
      rule("Third", { kind: "ext", exts: ["c"] }),
    ];
  }

  it("Move up/down are boundary-disabled on the first/last row", () => {
    render(WatchRulesDialog, { rules: threeRules() });
    const rows = screen.getAllByTestId("watch-rule-row");
    expect((within(rows[0]).getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(true);
    expect((within(rows[0]).getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(false);
    expect((within(rows[2]).getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(true);
    expect((within(rows[2]).getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(false);
  });

  it("unchecking Enable rule and saving dispatches `save` with that rule's enabled:false", async () => {
    const { component } = render(WatchRulesDialog, { rules: threeRules() });
    const rows = screen.getAllByTestId("watch-rule-row");
    await fireEvent.click(within(rows[0]).getByLabelText("Enable rule"));

    const saved = await new Promise<WatchRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });

    expect(saved.find((r) => r.name === "First")?.enabled).toBe(false);
    expect(saved.find((r) => r.name === "Second")?.enabled).toBe(true);
  });

  it("Move down swaps the rule with its successor and saving reflects the new order", async () => {
    const { component } = render(WatchRulesDialog, { rules: threeRules() });
    const rows = screen.getAllByTestId("watch-rule-row");
    await fireEvent.click(within(rows[0]).getByLabelText("Move down"));

    const saved = await new Promise<WatchRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });

    expect(saved.map((r) => r.name)).toEqual(["Second", "First", "Third"]);
  });

  it("Delete rule removes it from the list and saving reflects the removal", async () => {
    const { component } = render(WatchRulesDialog, { rules: threeRules() });
    const rows = screen.getAllByTestId("watch-rule-row");
    await fireEvent.click(within(rows[1]).getByLabelText("Delete rule"));

    expect(screen.getAllByTestId("watch-rule-row")).toHaveLength(2);
    const saved = await new Promise<WatchRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });
    expect(saved.map((r) => r.name)).toEqual(["First", "Third"]);
  });

  it("does not mutate the `rules` prop array in place (edits operate on a local copy)", async () => {
    const originalRules = threeRules();
    const snapshot = originalRules.map((r) => ({ ...r }));
    render(WatchRulesDialog, { rules: originalRules });
    const rows = screen.getAllByTestId("watch-rule-row");
    await fireEvent.click(within(rows[0]).getByLabelText("Delete rule"));

    expect(originalRules).toEqual(snapshot);
  });
});

describe("WatchRulesDialog — save / cancel (CPE-1400)", () => {
  it("Done dispatches `save` with the current local list, even with no edits", async () => {
    const rules = threeRulesFixture();
    const { component } = render(WatchRulesDialog, { rules });
    const saved = await new Promise<WatchRule[]>((resolve) => {
      component.$on("save", (e) => resolve(e.detail));
      fireEvent.click(doneBtn());
    });
    expect(saved).toEqual(rules);
  });

  it("Cancel button dispatches `cancel`", async () => {
    const { component } = render(WatchRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(screen.getByText("Cancel"));
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it("clicking the backdrop dispatches `cancel`", async () => {
    const { component, container } = render(WatchRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(container.querySelector(".backdrop") as HTMLElement);
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it("clicking inside the dialog body does not dispatch `cancel` (stopPropagation)", async () => {
    const { component } = render(WatchRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.click(screen.getByText("Watched-folder rules"));
    expect(cancel).not.toHaveBeenCalled();
  });

  it("pressing Escape dispatches `cancel`", async () => {
    const { component } = render(WatchRulesDialog, { rules: [] });
    const cancel = vi.fn();
    component.$on("cancel", cancel);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(cancel).toHaveBeenCalledTimes(1);
  });
});

function threeRulesFixture(): WatchRule[] {
  return [
    rule("First", { kind: "ext", exts: ["a"] }),
    rule("Second", { kind: "ext", exts: ["b"] }),
    rule("Third", { kind: "ext", exts: ["c"] }),
  ];
}
