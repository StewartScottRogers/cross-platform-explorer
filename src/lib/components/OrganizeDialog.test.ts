/**
 * OrganizeDialog (CPE-1142, epic CPE-979 "rules-based" slice). The preview/approve UI over
 * `organize_plan`/`organize_apply`. These assert: a preview loads for the default rule on mount,
 * switching the rule debounces then reloads the preview, an empty folder shows the empty state, Apply
 * calls `organize_apply` and surfaces the checkpoint + Undo (never before Apply is clicked), and a
 * failed preview surfaces an error instead of a stale/blank list. The typed `commands.*` client routes
 * through the mocked `../invoke`, so mocking `invoke` here drives it (mirrors `CheckpointDialog.test.ts`).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { styleBlock, declaration, lengthToPx, contentIndependentHeightReason } from "../svelteCss";
import { stripRustComments } from "../rustSource";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import OrganizeDialog from "./OrganizeDialog.svelte";

const PLAN_BY_KIND = [
  { name: "photo.png", target_subdir: "Images" },
  { name: "report.pdf", target_subdir: "Documents" },
  { name: "cover.png", target_subdir: "Images" },
];

beforeEach(() => {
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "organize_plan") return PLAN_BY_KIND;
    return null;
  });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("OrganizeDialog (CPE-1142)", () => {
  it("previews the default rule (by_kind) on mount, grouped by destination subfolder", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);

    expect(invokeMock).toHaveBeenCalledWith("organize_plan", { dir: "/work/proj", rule: "by_kind" });
    expect(await screen.findByTestId("summary")).toBeTruthy();
    expect(screen.getByTestId("group-Images")).toBeTruthy();
    expect(screen.getByTestId("group-Documents")).toBeTruthy();
  });

  it("switching the rule debounces, then reloads the preview for the new rule", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    invokeMock.mockClear();

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_plan") return [{ name: "a.png", target_subdir: "PNG" }];
      return null;
    });

    await fireEvent.click(screen.getByTestId("rule-by_extension"));
    expect(invokeMock).not.toHaveBeenCalled(); // still inside the debounce window

    await vi.advanceTimersByTimeAsync(150);
    expect(invokeMock).toHaveBeenCalledWith("organize_plan", { dir: "/work/proj", rule: "by_extension" });
    expect(await screen.findByTestId("group-PNG")).toBeTruthy();
  });

  it("shows the empty state when the folder has no files to organize", async () => {
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "organize_plan" ? [] : null));
    render(OrganizeDialog, { path: "/work/empty" });
    await vi.advanceTimersByTimeAsync(150);

    expect(await screen.findByTestId("empty-state")).toBeTruthy();
    expect((screen.getByTestId("apply-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("surfaces a preview error instead of showing a stale or blank list", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_plan") throw new Error("permission denied");
      return null;
    });
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);

    expect(await screen.findByTestId("error")).toBeTruthy();
  });

  it("never calls organize_apply before the user clicks Apply", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    await screen.findByTestId("group-Images");

    expect(invokeMock).not.toHaveBeenCalledWith("organize_apply", expect.anything());
  });

  it("Apply calls organize_apply, checkpoints, and surfaces the result + an Undo action", async () => {
    const { component } = render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    await screen.findByTestId("group-Images");

    const applied = vi.fn();
    const undo = vi.fn();
    component.$on("applied", applied);
    component.$on("undo", undo);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_apply") {
        return {
          checkpoint: {
            checkpoint: { manifest_id: "m-1", label: "Before auto-organize", ts: 1000 },
            new_blobs: 3,
            reused_blobs: 0,
            added_bytes: 100,
            skipped: [],
          },
          results: [
            { path: "/work/proj/Images/photo.png", ok: true, error: "" },
            { path: "/work/proj/Documents/report.pdf", ok: true, error: "" },
            { path: "/work/proj/Images/cover.png", ok: true, error: "" },
          ],
        };
      }
      return null;
    });

    await fireEvent.click(screen.getByTestId("apply-btn"));

    expect(invokeMock).toHaveBeenCalledWith("organize_apply", { dir: "/work/proj", rule: "by_kind" });
    expect(await screen.findByTestId("outcome-panel")).toBeTruthy();
    expect(applied).toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("undo-btn"));
    expect(undo).toHaveBeenCalled();
  });
});

/**
 * CPE-1965, INVERTED BY CPE-1968 — DERIVED, NOT CLAIMED (see CLAUDE.md "Derive provenance, don't
 * claim it").
 *
 * `gui-smoke/specs/organize.smoke.ts` carries a comment explaining why it waits for the default
 * rule's preview to land before clicking a rule pill. That explanation asserts facts about THIS
 * component, and a comment asserting facts about another file is untested by construction, so they
 * are re-read out of `OrganizeDialog.svelte` here on every run instead. Two of the three still hold
 * and are still pinned:
 *
 *   1. the backdrop centres the dialog vertically, so a height change WOULD move the `.rules` row UP;
 *   3. `.dialog` swallows stray clicks (`on:click|stopPropagation`), so a mis-landed click is silent
 *      rather than closing the dialog — which is why the failure surfaced 10s later as a missing
 *      `group-PNG` instead of at the click.
 *
 * WHY THE SECOND ONE IS NOW ASSERTED THE OTHER WAY UP, and it is not a regression. It used to read
 * "gives .preview a different height while loading than once the plan renders" — the height change
 * WAS the defect, and CPE-1965 pinned it because the gui-smoke spec's wait existed to sit it out.
 * CPE-1968 removed the height change at the source (`.preview` now has one plan-independent height;
 * the full reasoning is in the component, at the `.preview` rule). CPE-1965's ticket said in advance
 * that whichever app fix landed, this assertion would red, and that the red was the SIGNAL — it means
 * `organize.smoke.ts`'s wait has become belt-and-braces rather than load-bearing. So it is inverted
 * rather than deleted: the same fact is still derived from the same source, now asserting that the
 * two heights are the SAME. Reintroducing a content-driven height here reds immediately, at the
 * component, instead of resurfacing as a 4.3% CI flake in a spec ten files away.
 *
 * WHAT THIS DOES NOT PROVE, stated plainly: none of these three assertions is a click. jsdom has no
 * layout engine, so the ~98px shift cannot be MEASURED here. The geometric leg is the separate
 * CPE-1968 block below, which models the layout from these same declarations; the empirical leg is
 * the enumerated CI record in CPE-1965.
 */
describe("CPE-1965/CPE-1968 — the reflow the gui-smoke spec waits out (derived from the component)", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "OrganizeDialog.svelte"), "utf8");

  /** gui-smoke's window (`wdio.conf.ts`), and the viewport every px figure in CPE-1965/1968 is quoted at. */
  const VIEWPORT_H = 700;

  it("centres the dialog vertically, so the rules row moves when the dialog's height changes", () => {
    expect(styleBlock(SRC, "backdrop")).toMatch(/place-items:\s*center/);
  });

  it("gives .preview the SAME height while loading as once the plan renders (inverted by CPE-1968)", () => {
    const preview = styleBlock(SRC, "preview");

    // The old shape, named explicitly so its return is a red rather than a quiet pass.
    expect(
      declaration(preview, "min-height"),
      "`.preview` declares a min-height again. That is the CPE-1968 defect returning: paired with a " +
        "max-height it makes the box's height a function of its CONTENT, so it grows when the plan " +
        "lands and the centred dialog slides the rule pills up out from under the pointer.",
    ).toBeUndefined();

    const reason = contentIndependentHeightReason(preview, VIEWPORT_H);
    expect(
      reason,
      "`.preview` must have a height that cannot depend on the plan (viewport-dependent is fine — the " +
        `viewport does not change while a plan is in flight). It ${reason}. See CPE-1968.`,
    ).toBeNull();
  });

  it("swallows a click that lands on the dialog body, so a mis-landed click is silent", () => {
    expect(SRC).toMatch(/<div class="dialog"[^>]*on:click\|stopPropagation/);
  });

  it("still switches the rule when the pill is clicked — the defect is positional, not logical", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);

    await fireEvent.click(screen.getByTestId("rule-by_extension"));
    expect(
      (screen.getByTestId("rule-by_extension") as HTMLElement).className.split(/\s+/),
      "expected the clicked pill to become .active — the class organize.smoke.ts now asserts on",
    ).toContain("active");
  });
});

/**
 * CPE-1965 round 2 — THE FACT THE gui-smoke WAIT DEPENDS ON, pinned rather than assumed.
 *
 * Round 1 of the fix waited for `summary` **or** `empty-state` **or** `error`, on the stated reasoning
 * that "the loading placeholder carries no testid, so one of these three existing IS the dialog having
 * stopped resizing". That sentence was FALSE, and the wait it justified was satisfied at t=0:
 *
 *   - `loading` initialises to `false` and `plan` to `[]`;
 *   - `$: rule, path, scheduleLoad()` only ARMS `setTimeout(loadPlan, 120)` — `loading` does not
 *     become `true` until that timer fires;
 *   - so for the whole pre-load window the markup takes the `{:else if plan.length === 0}` branch and
 *     renders `data-testid="empty-state"` **synchronously at mount**.
 *
 * The wait therefore bought one `findElements` round-trip (~10-15ms) and nothing else — it moved runs
 * that used to land at 100-107ms INTO the 113-119ms hazard band. The flake was re-labelled, not fixed.
 *
 * This block is the guard that makes that undetectable-by-reading mistake detectable by running. It
 * reads the selector literal back out of `gui-smoke/specs/organize.smoke.ts` (anchored at column 0 on a
 * real `const … = "…";` declaration, so a commented-out or quoted copy cannot match — CPE-1933 rule 2)
 * and drives a real render of this component through the debounce, asserting WHEN it starts matching.
 *
 * RED-PROOF, run and recorded here rather than only in the PR body (CPE-1933 rule 3): widening
 * `organize.smoke.ts`'s `CPE1965_SETTLED_PREVIEW` back to the round-1 three-testid selector reds
 * **2 of 12** in this file — "…already matches at MOUNT, so `browser.waitUntil` on it returns on its
 * first poll and gates nothing: expected 1 to be +0" and "expected '[data-testid=\'summary\'], …' not
 * to match /empty-state/". Reverted; 12/12 green.
 */
describe("CPE-1965 — organize.smoke's settle-wait does not match until the plan renders", () => {
  const SPEC = readFileSync(
    join(process.cwd(), "gui-smoke", "specs", "organize.smoke.ts"),
    "utf8",
  );

  /** The value of a column-0 `const NAME = "…";` in `src`, asserted to occur exactly once. */
  function tsStringConst(src: string, name: string, where: string): string {
    const hits = [...src.matchAll(new RegExp(`^const ${name} = "([^"]*)";`, "gm"))];
    expect(hits.length, `${where} must declare \`const ${name} = "…"\` exactly once`).toBe(1);
    return hits[0][1];
  }

  const SETTLE_SELECTOR = tsStringConst(
    SPEC,
    "CPE1965_SETTLED_PREVIEW",
    "gui-smoke/specs/organize.smoke.ts",
  );

  /** What round 1 waited on. Kept as a literal so the defect it encodes stays reproducible here. */
  const ROUND_1_SELECTOR =
    '[data-testid="summary"], [data-testid="empty-state"], [data-testid="error"]';

  it("matches nothing at t=0, nothing at t=119ms, and only starts matching once the plan lands", async () => {
    render(OrganizeDialog, { path: "/work/proj" });

    expect(
      document.querySelectorAll(SETTLE_SELECTOR).length,
      `organize.smoke.ts's settle-wait selector (${SETTLE_SELECTOR}) already matches at MOUNT, so ` +
        "`browser.waitUntil` on it returns on its first poll and gates nothing. That is the round-1 " +
        "defect: the wait must not be satisfied before the 120ms debounce has even fired.",
    ).toBe(0);

    await vi.advanceTimersByTimeAsync(119);
    expect(
      document.querySelectorAll(SETTLE_SELECTOR).length,
      "the settle-wait selector matched before the dialog's own 120ms debounce fired",
    ).toBe(0);

    await vi.advanceTimersByTimeAsync(61); // past the debounce, plus the mocked invoke's microtasks
    expect(
      document.querySelectorAll(SETTLE_SELECTOR).length,
      "the settle-wait selector never matched even after the plan rendered — the spec would now hang " +
        "for its full 15s timeout on every run",
    ).toBeGreaterThan(0);
  });

  it("renders empty-state at t=0 — which is exactly why the round-1 three-testid wait was a no-op", async () => {
    render(OrganizeDialog, { path: "/work/proj" });

    // The pin. `loading` is still false and `plan` is still [] at mount, so the empty branch renders
    // immediately even though a non-empty plan is 120ms away.
    expect(
      screen.queryByTestId("empty-state"),
      "expected empty-state to render AT MOUNT (loading=false, plan=[] before scheduleLoad's timer " +
        "fires). If this ever stops being true, organize.smoke.ts's wait could safely widen again — " +
        "and the CPE-1965 comment explaining why it must not is stale.",
    ).not.toBeNull();
    expect(screen.queryByTestId("summary"), "no plan can have rendered at mount").toBeNull();

    expect(
      document.querySelectorAll(ROUND_1_SELECTOR).length,
      "expected the round-1 selector (summary/empty-state/error) to match at t=0 — this is the " +
        "measurement that showed the wait was satisfied before it waited",
    ).toBeGreaterThan(0);

    // ...and the shipped selector is a strict subset of it that does NOT include the mount-time branch.
    expect(SETTLE_SELECTOR).not.toMatch(/empty-state/);
    expect(SETTLE_SELECTOR).not.toMatch(/error/);
  });
});

/**
 * CPE-1968 — THE RED-PROOF: a click aimed at a rule pill while the first plan is still in flight.
 *
 * THE DEFECT this replaces the eyeball for. `.preview` used to be `min-height: 120px;
 * max-height: 45vh`, i.e. content-driven: 120px while the first `organize_plan` was in flight, up to
 * 45vh once it landed. `.backdrop` centres the dialog vertically, so that growth was split evenly
 * above and below and the `.rules` row moved UP by half of it — ~98px at the harness's 700px window,
 * ~120ms after the dialog appeared. A click aimed at "By extension" then landed inside `.preview`,
 * and `.dialog`'s `on:click|stopPropagation` swallowed it in silence.
 *
 * WHY A MODEL. jsdom has no layout engine: `getBoundingClientRect` returns zeros and
 * `elementFromPoint` is not implemented, so the shift cannot be measured by rendering. What CAN be
 * done honestly is to model the ONE axis the defect lives on — the vertical stack — from the
 * component's own declarations, read out of `OrganizeDialog.svelte` at run time (never recalled;
 * CPE-1933), and then feed the model's answer into a REAL click on a REAL render and assert the
 * RENDERED STATE that results (`.rule.active` plus the reloaded plan) rather than an event count.
 * The model decides WHICH element the click reaches; the component decides what that does.
 *
 * WHAT THE MODEL ASSUMES, so a reader can check it rather than trust it:
 *   - the dialog's direct children stack vertically in markup order with nothing between them. The
 *     order and the absence of an extra `.err` row are asserted below, so inserting a row above the
 *     pills reds here rather than silently invalidating the numbers.
 *   - `.rules` is ONE row (`flex-wrap: wrap` could make it two). Four short pills in a 620px dialog
 *     fit on one line; jsdom cannot measure text, so this is the model's one unverifiable input. It
 *     is also the SAFE direction: a second pill row makes the dialog taller and the shift larger,
 *     never smaller.
 *   - for the OLD content-driven shape, the settled height is taken as the `max-height` cap, i.e. a
 *     plan tall enough to fill it. That is the case CPE-1965 measured (run 33131342785). A shorter
 *     plan shifts the pills LESS, so the model states the defect at its measured worst rather than
 *     pretending to know a text-metrics-dependent content height it cannot compute.
 *
 * WHAT IT THEREFORE DOES NOT PROVE: that no OTHER dialog in this app has the same shape, and that
 * wry's webview lays this out exactly as modelled. It proves that a point aimed at the pills before
 * the plan lands is still on the pills after it lands, under a model derived from the CSS that ships.
 *
 * INDEPENDENTLY CROSS-CHECKED IN A REAL BROWSER, which is the leg a model most needs and least
 * deserves on its own. `scripts/dev-harness/organize-dialog/` mounts this same component in headless
 * Chrome at the same 1000x700 viewport and reports `.rules`' measured screen position; with the
 * pre-CPE-1968 CSS re-applied it measures the pills moving **97.5px**, the same number this model
 * produces, arrived at by a real layout engine rather than by arithmetic over the same declarations.
 * With the shipped CSS it measures 0.0px, and `.rules` sits at 187.0px in all three of loading, a
 * two-file plan and a 26-file plan. Two independent methods agreeing on 97.5 is worth more than
 * either alone — and if they ever disagree, the browser is right and this model needs fixing.
 */
describe("CPE-1968 — a click aimed mid-reflow lands where it was aimed", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "OrganizeDialog.svelte"), "utf8");

  /**
   * The harness window's height, DERIVED from the app's own `.inner_size(w, h)` rather than pasted.
   * `gui-smoke/lib/resetAppState.ts` restores exactly this after any spec that resizes, so it is the
   * height every px figure in CPE-1965 and CPE-1968 was measured at. Rust comments are stripped first
   * so a commented-out or quoted copy cannot answer (CPE-1933 rule 2).
   */
  const VIEWPORT_H = (() => {
    const rust = stripRustComments(readFileSync(join(process.cwd(), "src-tauri", "src", "lib.rs"), "utf8"));
    const hits = [...rust.matchAll(/\.inner_size\(\s*([\d.]+)\s*,\s*([\d.]+)\s*\)/g)];
    expect(hits.length, "expected exactly one `.inner_size(w, h)` call in src-tauri/src/lib.rs").toBe(1);
    return parseFloat(hits[0][2]);
  })();

  /** A declaration resolved to px, or a throw naming what was missing. */
  function px(className: string, prop: string): number {
    const raw = declaration(styleBlock(SRC, className), prop);
    if (raw === undefined) throw new Error(`.${className} declares no \`${prop}\``);
    return lengthToPx(raw, VIEWPORT_H);
  }

  /** `.dialog`'s border width, from its shorthand. */
  function dialogBorderPx(): number {
    const raw = declaration(styleBlock(SRC, "dialog"), "border");
    if (raw === undefined) throw new Error(".dialog declares no `border`");
    return lengthToPx(raw.trim().split(/\s+/)[0], VIEWPORT_H);
  }

  /**
   * `.preview`'s height in a given phase. With a definite `height` both phases agree — that IS the
   * fix. With the old `min-height`/`max-height` pair they do not, which is the defect.
   */
  function previewHeight(phase: "loading" | "settled"): number {
    const block = styleBlock(SRC, "preview");
    const definite = declaration(block, "height");
    if (definite) return lengthToPx(definite, VIEWPORT_H);
    const bound = declaration(block, phase === "loading" ? "min-height" : "max-height");
    if (!bound) throw new Error(`.preview declares neither a height nor a ${phase} bound`);
    return lengthToPx(bound, VIEWPORT_H);
  }

  interface Band {
    name: string;
    top: number;
    bottom: number;
  }

  /** The dialog's vertical bands, centred in the viewport, for a given preview phase. */
  function bands(phase: "loading" | "settled"): Band[] {
    // [name, height, margin-bottom] — `.head-row`'s height is set by the 26px `.docs` button in it,
    // `.rules`' by the 28px pills, `.actions`' by the 30px buttons.
    const rows: [string, number, number][] = [
      ["head-row", px("docs", "height"), px("head-row", "margin-bottom")],
      ["rules", px("rule", "height"), px("rules", "margin-bottom")],
      ["preview", previewHeight(phase), px("preview", "margin-bottom")],
      ["actions", px("btn", "height"), 0],
    ];
    const edge = dialogBorderPx() + px("dialog", "padding");
    const total = 2 * edge + rows.reduce((a, [, h, m]) => a + h + m, 0);
    let y = (VIEWPORT_H - total) / 2 + edge;
    return rows.map(([name, h, m]) => {
      const band = { name, top: y, bottom: y + h };
      y += h + m;
      return band;
    });
  }

  const bandNamed = (list: Band[], name: string) => list.find((b) => b.name === name)!;
  const bandAt = (list: Band[], y: number) =>
    list.find((b) => y >= b.top && y < b.bottom)?.name ?? "(outside the dialog)";

  it("stacks head-row, rules, preview, actions in that order with no other row between them", () => {
    // The model's structural assumption, asserted from the markup so that adding a row above the
    // pills reds HERE rather than silently invalidating every number below.
    const markup = SRC.slice(0, SRC.indexOf("<style>"));
    const order = ["head-row", "rules", "preview", "actions"].map((c) => markup.indexOf(`class="${c}"`));
    expect(order, "expected all four stacked rows in the markup").not.toContain(-1);
    expect(order, "expected head-row, rules, preview, actions in markup order").toEqual(
      [...order].sort((a, b) => a - b),
    );
  });

  it("red-proof: a click aimed at 'By extension' during the reflow window changes the rule", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "organize_plan") return null;
      return (args as { rule: string }).rule === "by_extension"
        ? [{ name: "a.png", target_subdir: "PNG" }]
        : PLAN_BY_KIND;
    });

    render(OrganizeDialog, { path: "/work/proj" });

    // t=100ms: inside the 120ms debounce, so no plan has landed and the preview is at its loading
    // height. This is the instant the user's pointer is over the pill.
    await vi.advanceTimersByTimeAsync(100);
    expect(screen.queryByTestId("summary"), "the plan must not have landed yet at t=100ms").toBeNull();
    expect(screen.queryByTestId("error"), "the model has no `.err` row; this run must not have one").toBeNull();
    const aimed = bandNamed(bands("loading"), "rules");
    const aimY = (aimed.top + aimed.bottom) / 2;

    // ...and now it lands, growing the box (or, after CPE-1968, not).
    await vi.advanceTimersByTimeAsync(80);
    await screen.findByTestId("summary");

    const landedIn = bandAt(bands("settled"), aimY);
    const shift = aimed.top - bandNamed(bands("settled"), "rules").top;

    // Dispatch the click at whatever is NOW under that point — the whole question this test exists
    // to answer. RED-PROOF, run and recorded here per CPE-1933 rule 3: with `.preview` reverted to
    // `min-height: 120px; max-height: 45vh`, `landedIn` resolves to "preview", the click below hits
    // the scroll box, `.dialog`'s `on:click|stopPropagation` eats it, and this reds on the `.active`
    // assertion with the message `…landed in "preview" at t=180ms — the dialog re-centred and the
    // pills moved 97.5px`, which is the ticket's ~98px arrived at independently. 3 of 15 red in this
    // file (this test, the inverted height assertion, and "switching rules does not move the rule
    // pills"). Reverted; 15/15 green with the shipped `height: clamp(200px, 40vh, 340px)`, where
    // `landedIn` resolves to "rules" and `shift = 0`.
    const target =
      landedIn === "rules"
        ? screen.getByTestId("rule-by_extension")
        : landedIn === "preview"
          ? screen.getByTestId("preview")
          : landedIn === "actions"
            ? screen.getByTestId("cancel-btn")
            : (document.querySelector(".backdrop") as HTMLElement);
    await fireEvent.click(target);

    expect(
      (screen.getByTestId("rule-by_extension") as HTMLElement).className.split(/\s+/),
      `a click aimed at the centre of the rule pills at t=100ms landed in "${landedIn}" at t=180ms — ` +
        `the dialog re-centred and the pills moved ${shift}px. That is CPE-1968: the click is ` +
        "swallowed by `.dialog`'s on:click|stopPropagation, with no rule change and no feedback. " +
        "`.preview` must have a height that does not depend on the plan.",
    ).toContain("active");

    // ...and the rule change actually took effect end-to-end, not just cosmetically.
    await vi.advanceTimersByTimeAsync(150);
    expect(invokeMock).toHaveBeenCalledWith("organize_plan", { dir: "/work/proj", rule: "by_extension" });
    expect(await screen.findByTestId("group-PNG"), "expected the by_extension plan to render").toBeTruthy();
  });

  it("switching rules does not move the rule pills", async () => {
    // LEG 1 — geometry. The pills' band is identical whatever the preview is showing, because the
    // box's height does not read the plan at all.
    expect(
      bandNamed(bands("settled"), "rules"),
      "the rule pills sit at a different height once the plan lands than while it is loading",
    ).toEqual(bandNamed(bands("loading"), "rules"));

    // LEG 2 — the DOM, which is what makes leg 1 sufficient. Leg 1 only speaks for `.preview`; it
    // would miss a plan-driven node appearing ABOVE the pills. So: render two plans of different
    // sizes and assert everything in the dialog OUTSIDE `.preview`'s scroll box is byte-identical.
    // Together they say the pills cannot move: nothing above them changed, and the box below them
    // did not resize.
    const outsidePreview = (): string => {
      const clone = (document.querySelector(".dialog") as HTMLElement).cloneNode(true) as HTMLElement;
      clone.querySelector('[data-testid="preview"]')!.innerHTML = "";
      // The one thing that IS supposed to change, normalised away so it cannot mask the rest.
      clone.querySelectorAll(".rule").forEach((el) => el.setAttribute("class", "rule"));
      return clone.innerHTML;
    };

    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "organize_plan") return null;
      return (args as { rule: string }).rule === "by_extension"
        ? [{ name: "a.png", target_subdir: "PNG" }] // one group, one item
        : PLAN_BY_KIND; // two groups, three items
    });

    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(180);
    await screen.findByTestId("summary");
    const before = outsidePreview();

    await fireEvent.click(screen.getByTestId("rule-by_extension"));
    await vi.advanceTimersByTimeAsync(180);
    await screen.findByTestId("group-PNG");

    expect(
      outsidePreview(),
      "switching the rule changed the dialog outside `.preview` — something above the rule pills " +
        "grew or shrank, so the centred dialog re-centres and the pills move under the pointer " +
        "(CPE-1968). Only `.preview`'s CONTENTS may differ between plans.",
    ).toEqual(before);
  });
});
