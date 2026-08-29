/**
 * CPE-1983 — THE REPO-WIDE GUARD for the reflow-under-the-pointer class that CPE-1968 fixed twice.
 *
 * THE DEFECT. Every modal in this app is centred by the same backdrop rule
 * (`position: fixed; inset: 0; display: grid; place-items: center`). Centring means the dialog's box
 * is positioned from its own height, so when a body INSIDE it grows — an async list landing, a
 * preview filling in — the growth is split evenly above and below and **everything above the body
 * slides up by half of it**. A pointer already resting on a control ends up somewhere else. In
 * `OrganizeDialog` that was ~98px, and `.dialog`'s `on:click|stopPropagation` swallowed the stray
 * click in silence (CPE-1968, PR #1099).
 *
 * WHY THIS FILE EXISTS AND THE TWO COMPONENT TESTS DO NOT SUFFICE. PR #1099 fixed `OrganizeDialog`
 * and the one neighbour its ticket NAMED, and pinned each with a per-component assertion. A
 * per-component assertion cannot close a class: it says nothing about the component nobody thought
 * of. Enumerating all 137 `.svelte` files for the same shape returned **at least five more**, one of
 * them (`CheckpointDialog`) with `Revert…` buttons INSIDE the growing box — where a mis-landed click
 * is not swallowed but DESTRUCTIVE. So the list is derived here at run time, never recalled
 * (CPE-1932), and the non-vacuity legs below fail loudly rather than reporting a comfortable count of
 * nothing.
 *
 * WHAT IS ENUMERATED, precisely, because the scope is the claim:
 *   - every `.svelte` file `git ls-files` reports (not a hand-written list, not a glob of the
 *     directories anyone remembered);
 *   - of those, the components declaring a CENTRED FIXED BACKDROP;
 *   - within those, every top-level rule whose selector is a single class, which declares
 *     `overflow: auto|scroll` and some height bound, and whose class is used in the markup on an
 *     element that is NOT the dialog's own root (`role="dialog"`). The root is excluded on purpose:
 *     its `max-height: 90vh` CAPS growth rather than causing it, and pinning it to a definite height
 *     would make every small dialog 90vh tall.
 * Each such box must have a height that cannot depend on its content — CPE-1968's decision, reused
 * rather than re-litigated (the alternatives it weighed and rejected are recorded in its ticket:
 * stop centring, 28 shared backdrops; or freeze the measured height in JS while loading).
 *
 * WHAT IT DOES NOT SEE — "at least these", never a count (CLAUDE.md's round-9 rule):
 *   - a body that grows some OTHER way. This scan only sees the `max-height`/`height` + `overflow`
 *     spelling. A box with no overflow at all that simply gets taller as rows arrive is invisible
 *     here, and is the same defect.
 *   - growth ABOVE the body: an error row, a notice, a warning banner appearing after the load moves
 *     everything below it and re-centres the dialog just as effectively. `OrganizeDialog.test.ts`'s
 *     "switching rules does not move the rule pills" leg is the shape that covers that, and it is
 *     per-component by nature.
 *   - a scroll box declared only inside an `@media` block (the population is top-level rules). The
 *     at-rule leg below reds if an at-rule takes a definite height back off a population box, but it
 *     does not enumerate boxes that exist only there.
 *   - a selector that is not a single class — `.a .b`, `ul.ops`, `[data-role]`.
 *   - **the second property some of these fixes need.** Where the dialog root is a flex column
 *     (`MacroRunConfirm`, `MacrosDialog`), a flex item's default `flex-shrink: 1` lets the free-space
 *     algorithm override a declared `height` once the dialog hits its own cap, so the fix needs
 *     `flex: 0 0 auto` as well. This guard reads the height only. Measured: deleting `flex: 0 0 auto`
 *     from `MacroRunConfirm`'s `.ops` leaves every assertion in THIS file green and reds only the
 *     component's own flex leg. That is why both components keep a per-component flex assertion; the
 *     enumeration does not subsume them.
 *   - anything about wry's webview, which is not a browser this runs in.
 *
 * ANCHORED ON PARSED CSS, NEVER ON COMMENT TEXT (CPE-1933 rule 2). `styleRules` strips comments
 * before scanning. **Re-measured for THIS scan rather than inherited**, and the result inverts the
 * upstream note. PR #1099's Reviewer measured that the stripper buys `styleBlock` nothing today, and
 * that its stated failure mode was backwards — without it a commented-out rule yields two matches and
 * `styleBlock` throws, a loud red. Both of those are facts about a SINGLE-SELECTOR LOOKUP. For an
 * ENUMERATOR neither holds, and the difference is not cosmetic:
 *
 *   - with `stripComments: false` this sweep's population drops from **22 boxes to 8**;
 *   - the 14 it loses are exactly the ones that have been FIXED, CPE-1968's `OrganizeDialog#preview`
 *     and `MacrosDialog#list` among them, because every fix in this class ships with a comment
 *     explaining it and a comment immediately above a rule is swallowed into that rule's SELECTOR —
 *     `.list` stops parsing as a single class and drops out of the enumeration altogether;
 *   - a smaller population is **all green**. So the unstripped failure mode here is a silent pass,
 *     not a loud throw: the enumeration quietly stops covering the very instances it was built for.
 *
 * That is asserted below, by name, not left as prose.
 */
import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { styleRules, declaration, contentIndependentHeightReason, svelteStyle } from "./svelteCss";

/** gui-smoke's window (`src-tauri/src/lib.rs`'s `.inner_size`), the viewport CPE-1968's px are quoted at. */
const VIEWPORT_H = 700;

/**
 * Boxes that keep a content-driven height ON PURPOSE, each with the reason it is not the CPE-1968
 * shape. A RATCHET (CLAUDE.md "a stored allowlist is a count wearing a coat"): registered in
 * `scripts/ratchet-baselines.mjs` as `dialog-body-reflow-allowlist`, so it can only ever shrink
 * without a declared row in `docs/design/RATCHETS.md`.
 *
 * The common thread, and why these are NOT simply the ones nobody got to: each is a SECONDARY panel
 * that is absent until the user asks for it. Pinning it to a definite height does not remove a
 * reflow — the panel's APPEARANCE is the reflow, and a fixed height cannot fix that — it only
 * reserves dead space in the state the dialog spends most of its life in. That family (make an
 * on-demand panel's arrival not move the dialog) is a different fix and belongs in its own ticket; it
 * is named in CPE-1983's report so it can be filed rather than smuggled in here.
 *
 * Format: `<component file>#<class>`, so a stale row reds by name (see the no-stale-rows leg).
 */
const DIALOG_BODY_REFLOW_ALLOWLIST = [
  "src/lib/components/CheckpointDialog.svelte#drift-list",
  "src/lib/components/BatchMediaDialog.svelte#skips-list",
  "src/lib/components/MacroRunConfirm.svelte#collision-list",
  "src/lib/components/CopilotDialog.svelte#op-results",
  "src/lib/components/RunCommandConfirm.svelte#res-out",
  "src/lib/components/UpdateDialog.svelte#notes",
  "src/lib/components/ColumnPickerDialog.svelte#list",
];

const REPO_ROOT = process.cwd();

/** Every tracked `.svelte` path, derived — never a glob of the directories anyone remembered. */
function svelteFiles(): string[] {
  return execFileSync("git", ["ls-files", "--", "*.svelte"], { cwd: REPO_ROOT, encoding: "utf8" })
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
}

/**
 * The markup half of a Svelte component: everything before the `<style>` block.
 *
 * Both halves are read from the same source string, so the CSS and the markup can never be one
 * commit apart.
 */
function markupOf(src: string): string {
  const i = src.indexOf("<style");
  return i === -1 ? src : src.slice(0, i);
}

/**
 * Every opening tag in the markup, as raw text.
 *
 * A regex like `/<[a-z][^>]*>/` is wrong here and quietly so: Svelte attributes carry expressions
 * (`on:click={() => dispatch("cancel")}`) whose `>` ends the match early, so the tag's later
 * attributes — `role="dialog"` among them — fall outside it. This walks `{}` depth and quote state
 * instead, so a tag ends at the first `>` that is genuinely outside both.
 */
function openTags(markup: string): string[] {
  const tags: string[] = [];
  for (let i = 0; i < markup.length; i++) {
    if (markup[i] !== "<" || !/[a-zA-Z]/.test(markup[i + 1] ?? "")) continue;
    let depth = 0;
    let j = i + 1;
    while (j < markup.length) {
      const c = markup[j];
      if (c === '"' || c === "'") {
        j++;
        while (j < markup.length && markup[j] !== c) j++;
      } else if (c === "{") depth++;
      else if (c === "}") depth--;
      else if (c === ">" && depth <= 0) break;
      j++;
    }
    tags.push(markup.slice(i, j + 1));
    i = j;
  }
  return tags;
}

interface ScrollBox {
  /** `<file>#<class>` — the allowlist key and the failure message's subject. */
  id: string;
  file: string;
  cls: string;
  block: string;
}

/** Is this rule body a scroll box — an overflow box with some height bound? */
function isScrollBox(block: string): boolean {
  if (!/(?:^|;|\{)\s*overflow(?:-y)?\s*:\s*(?:auto|scroll)/i.test(block)) return false;
  return declaration(block, "height") !== undefined || declaration(block, "max-height") !== undefined;
}

interface Scan {
  files: string[];
  centred: string[];
  boxes: ScrollBox[];
  /** `<file>#<class>` for a population box whose height is re-declared inside an at-rule. */
  atRuleOverrides: { id: string; atRule: string; block: string }[];
}

function scan(strip = true): Scan {
  const files = svelteFiles();
  const centred: string[] = [];
  const boxes: ScrollBox[] = [];
  const atRuleOverrides: Scan["atRuleOverrides"] = [];

  for (const file of files) {
    const src = readFileSync(join(REPO_ROOT, file), "utf8");
    if (!/<style[^>]*>/.test(src)) continue;
    let rules;
    try {
      rules = styleRules(src, { stripComments: strip });
    } catch {
      continue; // more than one <style> block: not a dialog shape this guard speaks for
    }
    const isCentred = rules.some(
      (r) => /(?:^|;|\{)\s*position\s*:\s*fixed/i.test(r.block) && /(?:place-items|align-items)\s*:\s*center/i.test(r.block),
    );
    if (!isCentred) continue;
    centred.push(file);

    const tags = openTags(markupOf(src));
    const forClass = (cls: string) => tags.filter((t) => new RegExp(`class="[^"]*\\b${cls}\\b[^"]*"`).test(t));

    for (const rule of rules) {
      const cls = /^\.([A-Za-z][A-Za-z0-9_-]*)$/.exec(rule.selector)?.[1];
      if (!cls) continue;
      const used = forClass(cls);
      if (used.length === 0) continue; // a rule for a class the markup never applies
      if (used.some((t) => /role="dialog"/.test(t))) continue; // the centred box itself, not a body in it
      if (rule.atRule) {
        if (boxes.some((b) => b.id === `${file}#${cls}`) && !declaration(rule.block, "height")) {
          atRuleOverrides.push({ id: `${file}#${cls}`, atRule: rule.atRule, block: rule.block });
        }
        continue;
      }
      if (!isScrollBox(rule.block)) continue;
      boxes.push({ id: `${file}#${cls}`, file, cls, block: rule.block });
    }
  }
  return { files, centred, boxes, atRuleOverrides };
}

const SCAN = scan();

describe("CPE-1983 — the enumeration itself (CPE-1932: a guard over 'all the X' must find the X)", () => {
  it("reads a real tree of .svelte files", () => {
    expect(
      SCAN.files.length,
      `git ls-files returned ${SCAN.files.length} .svelte files. This guard's whole value is that it ` +
        "sweeps the repo rather than a remembered list, so a near-empty enumeration is a broken scan, " +
        "not a clean bill of health.",
    ).toBeGreaterThan(100);
  });

  it("finds the shared centred-backdrop shape in many components", () => {
    expect(
      SCAN.centred.length,
      `only ${SCAN.centred.length} components matched the centred fixed backdrop. That shape is shared ` +
        "by every modal in the app; a low count means the CSS scan stopped seeing it.",
    ).toBeGreaterThan(40);
  });

  it("finds the scroll boxes inside them", () => {
    expect(
      SCAN.boxes.length,
      `only ${SCAN.boxes.length} scroll boxes found inside centred dialogs. CPE-1983 measured 20; a ` +
        "collapse means the rule enumerator or the markup scan broke, and every assertion below would " +
        "then be vacuously green.",
    ).toBeGreaterThanOrEqual(15);
  });

  it("the comment stripper is load-bearing HERE, unlike in styleBlock — measured, not inherited", () => {
    // CPE-1933 rule 3, and the ticket's explicit instruction not to inherit PR #1099's "buys nothing"
    // finding. Measured on this commit's own tree: 22 boxes stripped, 8 unstripped. The direction is
    // the point — the stripper does not stop over-reporting, it stops SILENT UNDER-reporting, which
    // is the failure that passes.
    const unstripped = new Set(scan(false).boxes.map((b) => b.id));
    const lost = SCAN.boxes.map((b) => b.id).filter((id) => !unstripped.has(id));
    expect(
      lost.length,
      "disabling the comment stripper did not change this scan's population. If that is genuinely " +
        "true now, the paragraph in this file's header claiming otherwise is stale and must be " +
        "re-measured — a note about a safeguard is a claim like any other.",
    ).toBeGreaterThan(0);

    // ...and this is WHY it matters, rather than just that a number moved. Every fix in this class
    // ships with a comment explaining it, and a comment immediately above a rule is swallowed into
    // that rule's SELECTOR when it is not stripped — so the rule stops looking like `.list` and drops
    // out of the enumeration entirely. The boxes an unstripped scan loses are precisely the ones
    // already fixed, including both of CPE-1968's. A smaller population is all green.
    expect(lost, "the unstripped scan must lose CPE-1968's two fixes — that is the silent pass").toEqual(
      expect.arrayContaining([
        "src/lib/components/OrganizeDialog.svelte#preview",
        "src/lib/components/MacrosDialog.svelte#list",
        "src/lib/components/CheckpointDialog.svelte#list",
      ]),
    );
  });
});

describe("CPE-1983 — every body inside a centred dialog has a content-independent height", () => {
  const allowed = new Set(DIALOG_BODY_REFLOW_ALLOWLIST);

  it("no scroll box outside the allowlist can grow under the pointer", () => {
    const offenders = SCAN.boxes
      .filter((b) => !allowed.has(b.id))
      .map((b) => ({ id: b.id, reason: contentIndependentHeightReason(b.block, VIEWPORT_H) }))
      .filter((o) => o.reason !== null);

    expect(
      offenders,
      "These boxes sit inside a dialog the backdrop centres, so growing after an async load slides " +
        "everything above them UP by half the growth and a pointer already resting on a control ends " +
        "up somewhere else (CPE-1968; in CheckpointDialog the control it can land on is `Revert…`). " +
        "Give the box ONE height that cannot depend on its content — a `vh` or a `clamp()` over one is " +
        "fine, the viewport does not change while a load is in flight — or, if it genuinely is a " +
        "secondary panel that is absent until asked for, add it to DIALOG_BODY_REFLOW_ALLOWLIST with " +
        `its reason.\n${offenders.map((o) => `  ${o.id}: ${o.reason}`).join("\n")}`,
    ).toEqual([]);
  });

  it("an @media block never takes a definite height back off one of them", () => {
    // The population is top-level rules; this is the leg that stops a narrow-window override from
    // quietly restoring the content-driven shape under a green top-level assertion.
    expect(SCAN.atRuleOverrides.map((o) => `${o.id} in ${o.atRule}`)).toEqual([]);
  });

  it("every allowlist row still names a real box (no stale rows)", () => {
    const found = new Set(SCAN.boxes.map((b) => b.id));
    const stale = DIALOG_BODY_REFLOW_ALLOWLIST.filter((id) => !found.has(id));
    expect(
      stale,
      "these allowlist rows match nothing the scan found — either the box was fixed (delete the row: " +
        "lowering a ratchet always sails through) or it was renamed and the row is now lying",
    ).toEqual([]);
  });

  it("no allowlist row is there for a box that already has a stable height", () => {
    const pointless = SCAN.boxes
      .filter((b) => allowed.has(b.id) && contentIndependentHeightReason(b.block, VIEWPORT_H) === null)
      .map((b) => b.id);
    expect(pointless, "these boxes are already fixed — remove their allowlist rows").toEqual([]);
  });
});

describe("CPE-1983 — the components CPE-1968 already fixed are still covered by this sweep", () => {
  // A guard modelled on two instances that cannot SEE those two instances is the shape CLAUDE.md
  // warns about: a backstop that structurally cannot fire reads as one. Both of CPE-1968's fixes are
  // in the population, so a revert of either reds here as well as in its own component test.
  it("sees OrganizeDialog's .preview and MacrosDialog's .list", () => {
    const ids = SCAN.boxes.map((b) => b.id);
    expect(ids).toContain("src/lib/components/OrganizeDialog.svelte#preview");
    expect(ids).toContain("src/lib/components/MacrosDialog.svelte#list");
  });

  it("parses the one thing a naive `<[^>]*>` tag regex gets wrong", () => {
    // The root-exclusion leg depends on seeing `role="dialog"` in a tag whose EARLIER attribute is an
    // arrow function. Pinned as its own case because getting it wrong makes every dialog root look
    // like a body and the guard demands a fixed height on all of them — a loud red, but for the
    // wrong reason, and the temptation would then be to loosen the guard.
    const tags = openTags('<div class="dialog" on:click={() => f(a > b)} role="dialog">');
    expect(tags).toHaveLength(1);
    expect(tags[0]).toContain('role="dialog"');
  });

  it("every centred dialog's own root is excluded, so none of them is in the population", () => {
    const roots = SCAN.boxes.filter((b) => b.cls === "dialog");
    expect(
      roots.map((b) => b.id),
      "a dialog root leaked into the population — its max-height CAPS growth rather than causing it, " +
        "and pinning it would make the dialog that tall in every state",
    ).toEqual([]);
  });
});

describe("CPE-1983 — the CSS rule enumerator itself", () => {
  // The first draft of this sweep used a regex that CONSUMED each rule's closing brace, so every
  // second rule was skipped and it reported 9 boxes over 8 files where the truth is 28 over 21 —
  // missing three of the instances the ticket names. Pinned so that regression cannot come back.
  it("returns CONSECUTIVE rules, not every other one", () => {
    const rules = styleRules("<style>.a { color: red; } .b { color: blue; } .c { color: green; }</style>");
    expect(rules.map((r) => r.selector)).toEqual([".a", ".b", ".c"]);
  });

  it("descends into at-rules and records the prelude", () => {
    const rules = styleRules("<style>.a { color: red; } @media (max-width: 700px) { .a { color: blue; } }</style>");
    expect(rules.map((r) => `${r.atRule}|${r.selector}`)).toEqual(["|.a", "@media (max-width: 700px)|.a"]);
  });

  it("does not let a brace inside a string unbalance the scan", () => {
    const rules = styleRules('<style>.a { content: "}"; } .b { color: red; }</style>');
    expect(rules.map((r) => r.selector)).toEqual([".a", ".b"]);
  });

  it("agrees with styleBlock on a real component, so the two readers cannot drift", () => {
    const src = readFileSync(join(REPO_ROOT, "src/lib/components/CheckpointDialog.svelte"), "utf8");
    const fromRules = styleRules(src).filter((r) => r.selector === ".list");
    expect(fromRules).toHaveLength(1);
    expect(fromRules[0].block).toBe(
      // styleBlock is the single-selector reader the two CPE-1968 component tests use.
      svelteStyle(src).match(/(?:^|\n)\s*\.list\s*\{([^}]*)\}/)![1],
    );
  });
});
