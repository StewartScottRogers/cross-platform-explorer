/**
 * CPE-1836 — at exactly the 600px floor (`.min_inner_size`, `src-tauri/src/lib.rs`), in the compound
 * scenario (both advisory notes + a busy row — a selection, "Hidden files shown", a long git branch —
 * all on screen at once), `.git`'s pinned `flex: 0 0 auto` children (the counts, dirty dot, buttons —
 * intentionally never shrink, since shrinking a clickable button is worse than truncating a branch name)
 * collectively exceed `.git`'s own shrunk box by ~16-33px. `.git` had no `overflow: hidden` of its own,
 * so that excess painted straight through into `.disk`'s box — "text painted over text", exactly what
 * this file's own ordering model (the comment above `.dim` in `StatusBar.svelte`) exists to prevent.
 *
 * ## What jsdom CAN and CANNOT prove here
 *
 * jsdom applies no layout at all (`getBoundingClientRect` returns zeros, `getComputedStyle` sees
 * nothing from a scoped `<style>` block), so no test in this file can observe WHERE `.git`'s children
 * actually render, or whether the bleed is genuinely fixed. Following `StatusBar.notice.test.ts` /
 * `StatusBar.diskAnchor.test.ts`'s established convention for this exact class of guard, what IS pinned
 * below is the *mechanism*: that `.git` carries its own `overflow: hidden`, so a future edit cannot
 * silently drop the one property this fix relies on.
 *
 * ## The actual verification — real browser, not jsdom
 *
 * Extended `scripts/dev-harness/statusbar-notice` (CPE-1660/1859's harness) with `?busy=1`, which
 * reproduces the ticket's own compound scenario (both notes, a selection, "Hidden files shown", and a
 * long branch with ahead/behind/dirty so Pull/Push/Sync all render), plus a full per-child rect sweep
 * with pairwise-overlap and parent-spill checks (see `inner-main.ts`'s `computeDiag`). Driven by
 * installed Chrome, `--headless=new --virtual-time-budget=15000 --dump-dom`, exactly as CPE-1859 was.
 *
 * **The one measurement that actually distinguishes broken from fixed** is NOT a rect comparison —
 * `overflow: hidden` clips PAINTING, not layout, so a pinned child's own `getBoundingClientRect()` is
 * IDENTICAL whether `.git` clips it or not (measured — `git-btn-pull`'s rect was byte-identical in both
 * builds). What differs is whether the excess is actually PAINTED, which `document.elementFromPoint`
 * (real hit-testing, unlike a rect comparison) can see. Probed at the midpoint of the overhanging
 * region — between `.git`'s own right edge and the overhanging child's right edge — at
 * innerWidth=600px, `notice=long`, `busy=1`:
 *
 * | Build | `gitChildOverhangPx` (geometry, unaffected by the fix) | `gitOverflowPaintProbe.hitIsGitDescendant` |
 * |---|---|---|
 * | Broken (`.git` with NO `overflow: hidden`) | `{"git-btn-pull": 40.2}` | **`true`** — the Pull button's overflow paints straight through the gap toward `.disk` |
 * | Fixed (`.git` WITH `overflow: hidden`) | `{"git-btn-pull": 40.2}` (same geometry) | **`false`** — probe hits bare `.statusbar` background; the overflow is clipped |
 *
 * Reproduced at 684px too: overhang `15.4px`, same `false` result once fixed.
 *
 * **Regression check — the two previously-clean surfaces stay clean.** The CPE-1780 acceptance surface
 * (`busy=0`: just the two notes, no selection/hidden/git) at 600px: `gitChildOverhangPx={}`,
 * `overlapPairs` only the pre-existing, ticket-acknowledged `disk×resize-grip` (a ~2px overlap between
 * `.disk`'s ellipsis box and the resize grip's hit region, predating CPE-1780, noted as likely invisible
 * — the grip is a low-opacity hatch and the text ends in an ellipsis). `overflow: hidden` on `.git` is a
 * pure no-op when nothing overflows it, so this surface cannot regress from this change — confirmed by
 * measurement rather than left to that reasoning alone.
 *
 * ## The judgment call this ticket asked for
 *
 * `.unreadable` shrinking to a 2-character `"Co…"` stub at 600px in the compound scenario: left AS IS.
 * Reordering the shrink priority to protect it would take room from `.filtered-hidden`/`.notice`/
 * `.git`, which are equally fragile in this same compound, extreme, sub-600px-floor-only scenario — the
 * ticket's own "Why it is Low" section already establishes that reaching this state at all requires
 * three independent conditions simultaneously, none of which the app's real usage produces together in
 * practice (`filteredHidden` is remote-only, `unreadableCount` is local-only, per each prop's own doc
 * comment in `StatusBar.svelte`). Further reordering here is disproportionate to a Low/S ticket and risks
 * moving the failure to a different element (the ticket's own recorded history for this row, three times
 * over) rather than removing it.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src", "lib", "components", "StatusBar.svelte");
const source = readFileSync(SRC, "utf8");

function styleBlock(): string {
  const m = source.match(/<style>([\s\S]*)<\/style>/);
  if (!m) throw new Error("StatusBar.svelte: no <style> block found");
  return m[1];
}

function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(?:^|\\s|\\})${escaped}\\s*\\{([^}]*)\\}`);
  const m = css.match(re);
  if (!m) throw new Error(`StatusBar.svelte <style>: no rule found for selector "${selector}"`);
  return m[1];
}

describe("CPE-1836 — .git clips its own overflow instead of bleeding into .disk", () => {
  it("`.git` carries `overflow: hidden` — the whole mechanism in one property", () => {
    const git = ruleBody(styleBlock(), ".git");
    expect(git).toMatch(/overflow:\s*hidden\s*;/);
  });

  it("`.git` still keeps `min-width: 0` and its SHRINKS-FIRST flex weight (the fix does not change its shrink behaviour, only its clipping)", () => {
    const git = ruleBody(styleBlock(), ".git");
    expect(git).toMatch(/min-width:\s*0\s*;/);
    expect(git).toMatch(/flex:\s*0\s*var\(--priority-shrink\)\s*auto\s*;/);
  });

  it("the pinned git-only children (counts/dot/buttons) remain flex: 0 0 auto — the fix clips them, it does not make them shrink", () => {
    const css = styleBlock();
    expect(ruleBody(css, ".git-ct")).toMatch(/flex:\s*0\s*0\s*auto\s*;/);
    expect(ruleBody(css, ".git-dirty")).toMatch(/flex:\s*0\s*0\s*auto\s*;/);
    expect(ruleBody(css, ".git-btn")).toMatch(/flex:\s*0\s*0\s*auto\s*;/);
  });
});
