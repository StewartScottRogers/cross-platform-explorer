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
 * THE SPLIT AS IT STANDS, stated so nobody has to infer it (and corrected in review round 2, where
 * round 1's "only three were fixed" was wrong in both halves): of the 22 boxes, **5 already had a
 * content-independent height** — `CompareDialog#tree`, `IntegrityDialog#report`,
 * `TemplatesDialog#list`, and CPE-1968's own two, `OrganizeDialog#preview` and `MacrosDialog#list` —
 * **10 are fixed by CPE-1983**, and **7 are allowlisted** below with reasons. CPE-1968 fixed two, not
 * three; the other three were already `vh`-stable and nobody had counted them.
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
 *   - **a dialog whose ROOT is its only scroll box.** The `role="dialog"` exclusion below says the
 *     root's `max-height` "caps growth rather than causing it", and that is only true ABOVE the cap:
 *     BELOW it the root sizes to its content and re-centres exactly like a body does. Live instances,
 *     named rather than left as a category — `ArchiveSafetyDialog.svelte` (`onMount(run)` swaps a
 *     loading line for a full report, with the `×` button above it) and `JoinPartsDialog.svelte`; both
 *     are centred and neither has any inner scroll box for this guard to look at. They are NOT fixed
 *     here because a definite height on a dialog ROOT is the wrong fix — it would make every state of
 *     that dialog as tall as its tallest. Removing the exclusion would surface them as false
 *     positives, not as fixes.
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
 *   - with `stripComments: false` this sweep's population drops sharply — most of it — and the losses
 *     include BOTH already-fixed boxes AND still-content-driven ones.
 *     The mechanism is that every fix in this class ships with a comment explaining it, and a comment
 *     immediately above a rule is swallowed into that rule's SELECTOR — `.list` stops parsing as a
 *     single class and drops out of the enumeration altogether.
 *   - **NO FIGURE APPEARS ABOVE, AND IT TOOK FOUR ATTEMPTS TO STOP WRITING ONE.** The history is kept
 *     because the shape of the mistake is the useful part. Every figure below is anchored to the
 *     commit it was measured at, and NONE of them describes the tree you are reading:
 *       * round 1 — "22 -> 8, and the losses are exactly the ones that have been FIXED". The 8 was
 *         right; "exactly the fixed ones" was not, as two unfixed boxes were lost too.
 *       * round 2 — corrected that to "one is not", which was ALSO wrong, and REGRESSED the
 *         population figure from 8 to 9 inside the correction. Measured at `776db8c1`: 8, losing 14,
 *         two of them content-driven.
 *       * round 3 — deleted the split, kept a hedged "at the time of writing 22 -> 8", and that was
 *         already wrong when it was committed. Measured at `56ac6bfa` by the reviewer's probe: 7,
 *         losing 15, THREE content-driven. The extra loss was `CopilotDialog#op-results`, whose only
 *         change in that round was REWORDED PROSE INSIDE A BLOCK COMMENT — no CSS, no assertion.
 *     Rounds 2 and 3 each demonstrated the lesson from inside the sentence stating it: which boxes
 *     vanish moves with the COMMENTS in the tree, so any commit touching any comment in a fixed
 *     component falsifies the count — including a commit whose sole purpose is to correct it.
 *     CLAUDE.md says it exactly: "the commit that writes such a claim is often the commit that
 *     falsifies it."
 *     A pinned COUNT is the same object as a pinned LIST, and the fix for both is the same: the two
 *     legs below DERIVE the properties — that at least one already-fixed box is lost, and that at
 *     least one still-content-driven box is lost — and name no box and no number. They absorbed all
 *     three drifts without an `expect(...)` moving. Whatever those legs report when they fail is the
 *     current truth; this prose is not, and has stopped trying to be.
 *   - **Round 1 also said "a smaller population is all green … a silent pass, not a loud throw",
 *     which claims this file's own floor does not work. It does.** With the stripper disabled the
 *     `>=15` floor reds, along with the other legs that name specific boxes. The accurate
 *     statement is one level down: the SUBSTANTIVE leg — "no scroll box can grow under the pointer" —
 *     passes VACUOUSLY over the shrunken population, and it is this PR's own non-vacuity floors that
 *     turn that vacuum into a red. Which is the argument for having them, not against.
 *
 * All of that is asserted below rather than left as prose.
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
 * THE COMMON THREAD FOR SIX OF THE SEVEN, and why they are not simply the ones nobody got to: each
 * is a SECONDARY panel that is absent until the user asks for it, and whose contents are already
 * whole when it mounts. Pinning it to a definite height does not remove a reflow — the panel's
 * ARRIVAL is the reflow, and no height can fix that — it only reserves dead space in the state the
 * dialog spends most of its life in. **Gating alone is not the discriminator**: `SyncDialog`'s
 * `.log` is `{#if}`-gated too and IS fixed here, because it arrives AND THEN GROWS, line by line,
 * for the whole run. The split is what happens after arrival, not whether there is one. That family
 * is CPE-1990.
 *
 * THE SEVENTH IS A DIFFERENT ANIMAL and says so out loud rather than sheltering under that sentence
 * — see `ColumnPickerDialog` below.
 *
 * WARNING ABOUT THE KEY, which round 1 of this file got caught by. `<file>#<class>` is a CSS key,
 * and a CSS class is not an element identity: **one row can name several structurally different
 * elements**, with different gating and different roles, and a justification true of one of them
 * reads as covering all of them. The scan now records `elements` per box and the leg
 * "an allowlist row that covers more than one element says so" asserts that any multi-element row is
 * declared here — so this paragraph is derived rather than remembered.
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
  // THE ONE THAT IS NOT A SECONDARY PANEL, stated honestly rather than filed under the paragraph
  // above (review round 2 caught round 1 doing exactly that). `.list` here names TWO elements: the
  // conditional `active-list`, and `available-list` at ColumnPickerDialog.svelte:94, which is
  // unconditional, is the dialog's main body, and changes height every time a column moves between
  // the two sections — with the row the user just clicked directly above it. That IS the CPE-1983
  // shape.
  //
  // It is deferred rather than fixed because a one-line pin is the wrong fix and the arithmetic says
  // so from the declarations alone: both boxes share this one rule, so pinning it puts 2 x 220px =
  // 440px of list inside a dialog whose own cap is `max-height: 85vh` — 595px at the 700px harness
  // window — leaving 155px for a header row, two section headings, two 14px section margins, an
  // action row and 40px of padding. The dialog would start scrolling itself, which trades one reflow
  // for a worse one. Sizing two boxes against a shared budget is a design decision, not a CSS
  // one-liner, and it needs the same before/after harness pass the CheckpointDialog fix got.
  "src/lib/components/ColumnPickerDialog.svelte#list",
];

/**
 * Allowlist rows whose class is applied to more than one element, with what the extra elements are.
 *
 * Derived-not-recalled companion to the key warning above: the guard measures which rows are
 * multi-element and fails if one is not declared here, so a second `ColumnPickerDialog` cannot hide
 * behind a single-element-shaped justification.
 *
 * NOT a ratchet, and named to avoid looking like one: it permits nothing and cannot grow on its own.
 * Every key must already be a `DIALOG_BODY_REFLOW_ALLOWLIST` row (asserted below), and that list is
 * the gated count. This is a description of rows that exist, not a licence for more of them.
 */
const MULTI_ELEMENT_ROWS: Record<string, string> = {
  "src/lib/components/ColumnPickerDialog.svelte#list":
    "TWO elements with DIFFERENT gating and different roles: `active-list` (conditional) and " +
    "`available-list` (unconditional, the dialog's main body). The one row's reason is true of only " +
    "one of them — see the row's own comment, which says so.",
  "src/lib/components/MacroRunConfirm.svelte#collision-list":
    "TWO elements, both `<ul>`s and both the same role — the blocked-collisions list and the " +
    "confirmable-collisions list, each inside its own `{#if …length}` collision panel. The row's " +
    "reason covers both without amendment.",
  "src/lib/components/RunCommandConfirm.svelte#res-out":
    "THREE elements, all the same role — the error line and the stdout/stderr `<pre>`s, all inside " +
    "the `{#if result}` results panel. The row's reason covers all three without amendment.",
};

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

/**
 * The class names an opening tag applies, as exact tokens.
 *
 * FOUND IN REVIEW ROUND 2, and it is worth stating because the wrong version looks right. Round 1
 * asked `class="[^"]*\blist\b[^"]*"`. In a regex a HYPHEN IS A WORD BOUNDARY, so `\blist\b` matches
 * inside `class="drift-list"`, and `\blog\b` inside `class="log-line"`. Measured against the
 * tokenised matcher: the same 22 boxes either way, but FIVE reported as multi-element where there are
 * THREE — the two phantoms being exactly `CheckpointDialog#list` (via `drift-list`) and
 * `SyncDialog#log` (via `log-line`).
 *
 * Both examples above are real, which round 2's first draft of this paragraph could not say: it
 * offered a third, `\bres-out\b` inside `res-outcome`, and no such class exists anywhere in `src/` —
 * an invented detail, inside the comment explaining a defect caused by over-trusting a matcher, which
 * is precisely the shape CPE-1933 exists for. Two measured examples are enough.
 *
 * It was also a latent FALSE EXCLUSION, which is the direction that matters: the `role="dialog"` skip
 * asks whether any tag carrying the class is the dialog root, so a body class that happened to be a
 * hyphen-substring of the root's would have removed a real box from the population silently. Nothing
 * in the tree does that today; the point is that the old matcher could not have told us.
 *
 * Tokenising is the fix rather than a tighter regex: split the attribute on whitespace and compare
 * whole tokens. Svelte interpolations (`class="op-kind kind-{opKind(op)}"`) survive as their own
 * tokens and simply never match a plain class name, which is correct.
 */
function classTokens(tag: string): string[] {
  const attr = /\sclass="([^"]*)"/.exec(tag);
  return attr ? attr[1].split(/\s+/).filter(Boolean) : [];
}

interface ScrollBox {
  /** `<file>#<class>` — the allowlist key and the failure message's subject. */
  id: string;
  file: string;
  cls: string;
  block: string;
  /**
   * How many elements in the markup carry this class.
   *
   * Recorded because the `#class` key is a CSS key, and CSS classes are not element identities: a
   * single row can stand for several structurally different boxes. Round 1 of this file allowlisted
   * `ColumnPickerDialog#list` on a justification true of only one of the TWO elements it names, and
   * nothing in the guard could show that. Now a multi-element box is a fact the scan reports.
   */
  elements: number;
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
    const forClass = (cls: string) => tags.filter((t) => classTokens(t).includes(cls));

    for (const rule of rules) {
      const cls = /^\.([A-Za-z][A-Za-z0-9_-]*)$/.exec(rule.selector)?.[1];
      if (!cls) continue;
      const used = forClass(cls);
      if (used.length === 0) continue; // a rule for a class the markup never applies
      // The centred box itself, not a body inside it. Its `max-height` caps growth ABOVE the cap —
      // but below the cap a root grows with its content and re-centres exactly like a body, so this
      // exclusion does hide a real family. It is a deliberate blind spot, not a claim of safety, and
      // it is named with live instances in this file's header.
      if (used.some((t) => /role="dialog"/.test(t))) continue;
      if (rule.atRule) {
        if (boxes.some((b) => b.id === `${file}#${cls}`) && !declaration(rule.block, "height")) {
          atRuleOverrides.push({ id: `${file}#${cls}`, atRule: rule.atRule, block: rule.block });
        }
        continue;
      }
      if (!isScrollBox(rule.block)) continue;
      boxes.push({ id: `${file}#${cls}`, file, cls, block: rule.block, elements: used.length });
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
      `only ${SCAN.boxes.length} scroll boxes found inside centred dialogs. CPE-1983 measured 22; a ` +
        "collapse means the rule enumerator or the markup scan broke, and every assertion below would " +
        "then be vacuously green.",
    ).toBeGreaterThanOrEqual(15);
  });

  it("the comment stripper is load-bearing HERE, unlike in styleBlock — measured, not inherited", () => {
    // CPE-1933 rule 3, and the ticket's explicit instruction not to inherit PR #1099's "buys nothing"
    // finding. The direction is the point: the stripper does not stop over-reporting, it stops
    // SILENT UNDER-reporting, which is the failure that passes.
    const unstripped = new Set(scan(false).boxes.map((b) => b.id));
    const lost = SCAN.boxes.filter((b) => !unstripped.has(b.id));
    expect(
      lost.length,
      "disabling the comment stripper did not change this scan's population. If that is genuinely " +
        "true now, the paragraph in this file's header claiming otherwise is stale and must be " +
        "re-measured — a note about a safeguard is a claim like any other.",
    ).toBeGreaterThan(0);

    // WHY it matters, derived rather than pinned. Round 1 listed three box ids here, which is a
    // claim about today's comments: WHICH rules a comment happens to sit above moves every time
    // anyone edits one — round 2's own diff moved one, by adding a comment to `CheckpointDialog`.
    // So assert the two PROPERTIES that make this dangerous instead, and name nothing.
    //
    // Property 1: the losses include boxes that are ALREADY FIXED — the enumeration stops watching
    // the instances it was built for, and cannot then notice a revert.
    expect(
      lost.filter((b) => contentIndependentHeightReason(b.block, VIEWPORT_H) === null).length,
      "an unstripped scan lost no already-fixed box, so it could not stop watching for a revert",
    ).toBeGreaterThan(0);

    // Property 2, and the worse half: the losses include boxes that are STILL CONTENT-DRIVEN. A live
    // offender leaving the population is a guard quietly narrowing its own scope, which is worse than
    // losing a box that is already correct. Round 1's prose said the losses were "exactly the ones
    // that have been FIXED"; this is the leg that would have caught that.
    expect(
      lost.filter((b) => contentIndependentHeightReason(b.block, VIEWPORT_H) !== null).map((b) => b.id).length,
      "an unstripped scan lost no still-content-driven box. That is a WEAKER failure than the header " +
        "describes, so if it is genuinely true now the header paragraph is stale.",
    ).toBeGreaterThan(0);
  });

  it("with the stripper off, this file's own floor is what reds — not a silent pass", () => {
    // Round 1's prose said "a smaller population is all green … a silent pass, not a loud throw",
    // which claims this file's floor does not work. It does, and saying otherwise undersells the very
    // safeguard the PR added. What passes VACUOUSLY is the substantive height leg; the floor is what
    // turns that vacuum into a red. Both halves are asserted here so neither can be over-stated again.
    const unstripped = scan(false);

    // The floor: the same >=15 the enumeration leg above asserts, evaluated on the shrunken scan.
    expect(
      unstripped.boxes.length,
      "the unstripped population is still above this file's floor, so the floor would NOT red and " +
        "the header's account of what protects this scan is wrong",
    ).toBeLessThan(15);

    // The vacuum: every surviving box still satisfies the substantive invariant, so that leg alone
    // would report success over a population missing most of its subjects.
    const stillOffending = unstripped.boxes
      .filter((b) => !DIALOG_BODY_REFLOW_ALLOWLIST.includes(b.id))
      .filter((b) => contentIndependentHeightReason(b.block, VIEWPORT_H) !== null);
    expect(
      stillOffending.map((b) => b.id),
      "the substantive leg would have caught the shrunken scan on its own, so the floor is not what " +
        "is doing the work here and the header should say so",
    ).toEqual([]);
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

  it("an allowlist row that covers more than one element says so", () => {
    // Round 1 allowlisted `ColumnPickerDialog#list` on a justification true of one of the TWO
    // elements that class names — and no leg could show it, because the key is a CSS key and the
    // guard only ever looked at CSS. This is the leg that makes it visible: a row standing for
    // several structurally different elements must be declared in MULTI_ELEMENT_ROWS, so its
    // justification has to be written knowing that.
    const undeclared = SCAN.boxes
      .filter((b) => allowed.has(b.id) && b.elements > 1 && !(b.id in MULTI_ELEMENT_ROWS))
      .map((b) => `${b.id} (${b.elements} elements)`);
    expect(
      undeclared,
      "this allowlist row's class is applied to more than one element, so its reason may be true of " +
        "only one of them. Declare it in MULTI_ELEMENT_ROWS with what the extra elements are — or fix " +
        "the box, if the reason turns out not to cover them all.",
    ).toEqual([]);
  });

  it("every MULTI_ELEMENT_ROWS key is a real, still-multi-element allowlist row", () => {
    // The other direction, so the table cannot outlive what it describes.
    const byId = new Map(SCAN.boxes.map((b) => [b.id, b]));
    const wrong = Object.keys(MULTI_ELEMENT_ROWS).filter(
      (id) => !allowed.has(id) || (byId.get(id)?.elements ?? 0) <= 1,
    );
    expect(wrong, "these MULTI_ELEMENT_ROWS keys are stale — the row is gone, or now names one element").toEqual([]);
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
  // second rule was skipped — and it lost `CopilotDialog`'s `.op-list`/`.op-results` and
  // `MacroRunConfirm`'s `.ops`, three of the instances the ticket names. That consequence is the
  // durable fact and it is pinned directly below; the raw hit counts that draft printed are recorded
  // as history in `svelteCss.ts`'s `styleRules` header, not repeated here as if reproducible.
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
