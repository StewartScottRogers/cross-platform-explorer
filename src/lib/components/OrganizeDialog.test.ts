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
 * CPE-1965 — DERIVED, NOT CLAIMED (see CLAUDE.md "Derive provenance, don't claim it").
 *
 * `gui-smoke/specs/organize.smoke.ts` carries a comment explaining why it must wait for the default
 * rule's preview to land before clicking a rule pill. That explanation asserts three facts about THIS
 * component, and a comment asserting facts about another file is untested by construction. So the
 * three are re-read out of `OrganizeDialog.svelte` here on every run instead:
 *
 *   1. the backdrop centres the dialog vertically, so a height change moves the `.rules` row UP;
 *   2. `.preview` is SHORTER while the plan is in flight (`min-height`) than once it renders
 *      (`max-height`), so there IS a height change ~120ms after mount;
 *   3. `.dialog` swallows stray clicks (`on:click|stopPropagation`), so a mis-landed click is silent
 *      rather than closing the dialog — which is why the failure surfaced 10s later as a missing
 *      `group-PNG` instead of at the click.
 *
 * Together those are the mechanism. Change any of them and this reds, so the next reader is told the
 * spec's wait may no longer be load-bearing rather than inheriting a stale story.
 *
 * WHAT THIS DOES NOT PROVE, stated plainly: it does not reproduce the swallowed click. jsdom has no
 * layout, so the ~98px shift cannot be measured here, and the real reproduction rate was 3 of 69
 * shard-4 CI jobs (4.3%) — no single local or CI run settles it either way. The empirical half of the
 * red-proof is the enumerated CI record in CPE-1965, not this file.
 */
describe("CPE-1965 — the reflow the gui-smoke spec waits out (derived from the component)", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "OrganizeDialog.svelte"), "utf8");

  /** The one CSS declaration block for `.name`. Fails rather than guessing if it is not unique. */
  function ruleBlock(name: string): string {
    const matches = [...SRC.matchAll(new RegExp(`(?:^|\\n)\\s*\\.${name}\\s*\\{([^}]*)\\}`, "g"))];
    expect(matches.length, `expected exactly one \`.${name}\` CSS block in OrganizeDialog.svelte`).to.equal(1);
    return matches[0][1];
  }

  it("centres the dialog vertically, so the rules row moves when the dialog's height changes", () => {
    expect(ruleBlock("backdrop")).toMatch(/place-items:\s*center/);
  });

  it("gives .preview a different height while loading than once the plan renders", () => {
    const preview = ruleBlock("preview");
    const min = /min-height:\s*([^;]+);/.exec(preview)?.[1]?.trim();
    const max = /max-height:\s*([^;]+);/.exec(preview)?.[1]?.trim();
    expect(min, "expected .preview to declare a min-height (its in-flight height)").toBeTruthy();
    expect(max, "expected .preview to declare a max-height (its settled height)").toBeTruthy();
    expect(
      min,
      "expected .preview's loading height to differ from its settled height — if these are now equal " +
        "the dialog no longer reflows and organize.smoke.ts's CPE-1965 wait is belt-and-braces",
    ).not.toEqual(max);
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
