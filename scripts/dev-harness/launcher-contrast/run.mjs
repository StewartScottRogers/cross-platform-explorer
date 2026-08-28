// CPE-1966 — CLI entry point + CI job body for the AI Console launcher's real-browser contrast sweep.
// See engine.mjs's header for what this covers that the static vitest guard structurally cannot, and
// why the browser is the only thing that can answer those four questions.
//
// Run:  node scripts/dev-harness/launcher-contrast/run.mjs
//   or: npm run harness:launcher-contrast
//
// Flags:
//   --all              print every measured site, not only the failures
//   --site <substr>    print every measured site whose path/prop/state contains <substr>
//   --verify-pixels    additionally screenshot each scheme and compare the painted ground against
//                      the computed-style prediction (the independent second path)
//   --json             dump the whole measurement as JSON on stdout
//
// EXIT CODE is the point. 1 the moment any ENFORCED site is under its bar — naming the site, the two
// colours, the ground it was measured against, and the bar it missed — and equally on any of:
//   * a styled class no fixture mounts (the fixture-completeness enumeration, CPE-1932);
//   * a `--verify-pixels` ground the screenshot painted differently from the prediction;
//   * a `--verify-pixels` pass that verified ZERO grounds, because "0 verified, 0 disagreeing" reads
//     as success and is really the leg not running (this repo's "did not run" != "found nothing").
// 2 is the harness itself refusing to answer: a failed WCAG anchor, a fixture whose provenance claim
// is no longer in the launcher's source, a Chrome that would not start.

import { sweep, round2, CHROMA_MIN, ANIM_SAMPLES } from "./engine.mjs";

const argv = process.argv.slice(2);
const flag = (n) => argv.includes(n);
const opt = (n) => { const i = argv.indexOf(n); return i >= 0 ? argv[i + 1] : undefined; };

/**
 * How close to its bar an ENFORCED site may sit before the report calls it out by name.
 *
 * Not cosmetic. Every ratio here is measured against colours the ENGINE resolved — `Canvas`,
 * `CanvasText`, `Field`, `ButtonFace` — and those move between Chrome builds and platforms: this
 * build resolves dark `ButtonFace` to rgb(107,107,107), the build CPE-1966 was filed against
 * resolved it near rgb(120,120,120), and that alone moved one site from 4.24 to 5.07. A site
 * clearing 4.5 by +0.02 is therefore not passing, it is *undecided*, and the person who meets it as
 * a red on a runner this job has never run on will go hunting for a regression that is not there.
 * So thin margins are printed with the resolved system colours beside them, every run, pass or fail.
 */
const THIN_MARGIN = 0.25;

/**
 * Which measured sites the harness ENFORCES, and why each rule is where it is.
 *
 * TEXT is enforced unconditionally, at SC 1.4.3's 4.5:1 (3:1 for large text, derived from the
 * computed font-size/weight, never guessed upward).
 *
 * NON-TEXT (SC 1.4.11, 3:1) is enforced for two disjoint reasons, and the split matters:
 *   - any FOCUS state's border/outline/shadow, chromatic or not, because a focus indicator is
 *     REQUIRED to be perceivable and this page sets `outline: none` so its border is the only one
 *     it has (CPE-1966 site 1);
 *   - any CHROMATIC border/fill/shadow in any state, because an author who reached for a colour is
 *     conveying something with it. The `>= CHROMA_MIN` channel spread is what keeps the neutral
 *     hairlines and hover washes (`var(--line)` = rgba(128,128,128,0.26), `rgba(128,128,128,0.14)`)
 *     out: those are decorative separators, which SC 1.4.11 explicitly does not cover, and enforcing
 *     them would drown the real findings in noise rather than add coverage.
 */
function enforced(s) {
  // NOT ENFORCED, and this is the harness's own honest limit rather than an oversight: a colour the
  // launcher's JS assigns INLINE (`chip.style.background = sessionColor(id)`) is not authored in the
  // stylesheet this harness guards. It comes from an identity palette shared with the main app
  // (`src/lib/sessionChip.ts`), so changing it is an app-wide visual decision, not a launcher CSS fix.
  // Those sites are still MEASURED and printed under "measured, not enforced" below — see the report
  // and CPE-1977 for the palette's own numbers.
  if (s.inlineSelf || s.inlineGround) return false;
  if (s.role === "text") return true;
  const isFocus = /:focus/.test(s.state || "");
  if ((s.role === "border" || s.role === "outline") && isFocus) return true;
  return !!s.chromatic;
}

/** A border abuts two colours; it must clear its bar against BOTH (the worst one is the finding). */
function worstOf(s) {
  if (s.role === "border" && typeof s.ratioOutside === "number") {
    if (typeof s.ratio !== "number") return { r: s.ratioOutside, against: s.againstOutside, side: "the exterior it sits on" };
    return s.ratio <= s.ratioOutside
      ? { r: s.ratio, against: s.against, side: "the interior it encloses" }
      : { r: s.ratioOutside, against: s.againstOutside, side: "the exterior it sits on" };
  }
  return { r: s.ratio, against: s.against, side: s.role === "fill" ? "what it sits on" : "its ground" };
}

function key(s) {
  return [s.scheme, s.path, s.role, s.prop, s.animated ? "animation" : s.state].join(" | ");
}

/** Findings are grouped by the RULE that produced them, not the element: `button.primary` appears
 *  five times in this page and one accent value is one defect, not five. */
function signature(s) {
  const w = worstOf(s);
  return [s.scheme, s.role, s.prop, s.declared, s.painted, w.against, s.bar, s.pseudo ?? (s.animated ? "animation" : "base")].join(" | ");
}

function main() {
  return sweep({ verifyPixels: flag("--verify-pixels") }).then((res) => {
    if (flag("--json")) {
      process.stdout.write(JSON.stringify(res, null, 2));
      return 0;
    }

    console.log("AI Console launcher — contrast sweep (CPE-1966)\n");
    // ONE implementation, validated here and re-validated inside the page against the same source
    // string the probe runs — see COLOR_MATH_SOURCE in engine.mjs for the round-1 defect this closed.
    console.log("WCAG implementation validated against known anchors BEFORE measuring anything,");
    console.log("then re-run BY THE ENGINE on the same source the probe uses (one copy, two executions):");
    for (const [k, v] of Object.entries(res.anchors)) console.log(`  ${k.padEnd(40)} ${v}`);
    console.log("  (the last two are a stated worked example, not any engine's system colours;");
    console.log("   the engine-resolved version of the same comparison is per-scheme below)");
    console.log("");

    // The system colours EVERY number below is measured against. Printed as a baseline, not as
    // trivia: they are the one input this harness does not control, they differ between Chrome
    // builds and platforms, and this job has never run on a GitHub runner. A future disagreement is
    // then diagnosable from the log rather than mysterious.
    for (const scheme of ["light", "dark"]) {
      const d = res.schemes[scheme];
      console.log(`${scheme}: engine-resolved Canvas=${d.canvas} CanvasText=${d.canvasText} Field=${d.field} ButtonFace=${d.buttonFace}`);
      if (d.composite) {
        console.log(
          `  ${scheme} compositing on those resolved colours (${d.composite.stack}): ` +
            `${d.composite.textOnly} text-only / ${d.composite.bothDimmed} both dimmed`,
        );
      }
    }
    console.log("");

    // Collapse repeats (fixtures mount two tabs, animations produce ANIM_SAMPLES frames per element)
    // down to the WORST reading for each site, which is the one a user can actually encounter.
    const worst = new Map();
    let measured = 0;
    for (const scheme of ["light", "dark"]) {
      for (const s of res.schemes[scheme].sites) {
        measured++;
        const k = key(s);
        const w = worstOf(s).r;
        const prev = worst.get(k);
        if (!prev || w < worstOf(prev).r) worst.set(k, s);
      }
    }

    const sites = [...worst.values()];
    const checked = sites.filter(enforced);
    const failures = checked.filter((s) => worstOf(s).r < s.bar);

    const wanted = opt("--site");
    const listed = flag("--all") ? sites : wanted ? sites.filter((s) => key(s).toLowerCase().includes(wanted.toLowerCase())) : [];
    if (listed.length) {
      console.log(`── ${listed.length} site readings ─────────────────────────────────────────────`);
      for (const s of listed.sort((a, b) => worstOf(a).r - worstOf(b).r)) {
        const w = worstOf(s);
        console.log(
          `  ${round2(w.r).toFixed(2).padStart(6)}:1  bar ${s.bar}  ${enforced(s) ? "ENFORCED" : "measured"}  ` +
            `${s.scheme}  ${s.role}  ${s.painted} on ${w.against}  ${s.path}  [${s.state}]`,
        );
      }
      console.log("");
    }

    console.log(`── coverage ─────────────────────────────────────────────────────────────`);
    console.log(`  ${measured} raw readings -> ${sites.length} distinct sites, ${checked.length} enforced`);

    // The pixel leg's verdict, and the two ways it can fail. `bad.length` is a real disagreement
    // between the two paths; `pixels.length === 0` is a leg that ran and measured nothing, which
    // prints as "0 verified, 0 disagreeing" and reads exactly like success. Round 1 exited 0 on
    // both — forcing every prediction to #ff00ff gave 59/59 disagreeing, PASS, exit 0, in the shape
    // the blocking CI job runs — so both are counted here and both fail the run below.
    let pixelBad = 0;
    let pixelEmpty = [];
    for (const scheme of ["light", "dark"]) {
      const d = res.schemes[scheme];
      console.log(`  ${scheme}: ${d.forced.length} forced pseudo-state readings, ${d.animations} CSS animations x ${ANIM_SAMPLES} frames`);
      if (d.pixels) {
        const bad = d.pixels.filter((p) => p.delta > 1);
        pixelBad += bad.length;
        if (d.pixels.length === 0) pixelEmpty.push(scheme);
        console.log(`  ${scheme}: pixel cross-check — ${d.pixels.length} grounds screenshot-verified, ${bad.length} disagreeing by more than 1/255`);
        for (const p of bad.slice(0, 8)) console.log(`      predicted ${p.predicted} painted ${p.painted} (delta ${p.delta})  ${p.path}`);
        if (bad.length > 8) console.log(`      ... and ${bad.length - 8} more`);
      }
    }
    console.log(`  chromatic threshold for non-text roles: max-min channel >= ${CHROMA_MIN}`);

    // Measured but not enforced, printed every run so the number is never mistaken for zero.
    const inlineUnder = sites.filter((s) => (s.inlineSelf || s.inlineGround) && worstOf(s).r < s.bar);
    if (inlineUnder.length) {
      console.log("");
      console.log(`  MEASURED, NOT ENFORCED — ${inlineUnder.length} reading(s) under bar whose colour is assigned inline`);
      console.log("  by the launcher's JS from the shared session-identity palette (see enforced() for why):");
      const seen = new Set();
      for (const s of inlineUnder.sort((a, b) => worstOf(a).r - worstOf(b).r)) {
        const sig = `${s.scheme}|${s.role}|${s.painted}|${worstOf(s).against}`;
        if (seen.has(sig)) continue;
        seen.add(sig);
        console.log(`      ${round2(worstOf(s).r).toFixed(2).padStart(5)}:1 (bar ${s.bar})  ${s.scheme}  ${s.role}  ${s.painted} on ${worstOf(s).against}  ${s.path}`);
      }
    }

    // The rest of the 786. Round 1's report accounted for the 384 enforced and gave the 9 inline
    // ones their own section, and was silent about the remainder — which is most of them. Silence
    // reads as "there was nothing there"; these are a JUDGEMENT (SC 1.4.11 excludes decorative
    // separators) and the count belongs in the log where the judgement can be checked, with `--all`
    // as the way to see every one.
    const dropped = sites.filter((s) => !enforced(s) && !(s.inlineSelf || s.inlineGround));
    const droppedUnder = dropped.filter((s) => worstOf(s).r < s.bar);
    console.log("");
    console.log(`  NOT ENFORCED — ${dropped.length} non-text site(s) whose colour is under the ${CHROMA_MIN}-channel`);
    console.log(`  chromatic threshold: neutral hairlines and hover washes (var(--line) = rgba(128,128,128,0.26)`);
    console.log(`  and friends), which SC 1.4.11 excludes as decorative. ${droppedUnder.length} of them are under the bar`);
    console.log(`  they would face if enforced — pass --all to list every one with its numbers.`);

    // Thin margins (round-2 review item 4). An enforced site clearing its bar by less than
    // THIN_MARGIN is not a pass so much as an undecided, because the ground under it is a system
    // colour this harness does not control. Printed pass or fail, with the resolved colours above.
    const thin = checked.filter((s) => worstOf(s).r >= s.bar && worstOf(s).r - s.bar < THIN_MARGIN);
    console.log("");
    if (thin.length) {
      // Grouped by RULE, exactly as failures are: one accent value on five buttons is one thin
      // margin, not five, and a list that repeats it is a list nobody reads.
      const byRule = new Map();
      for (const s of thin) {
        const g = byRule.get(signature(s)) ?? { sample: s, paths: new Set() };
        g.paths.add(s.path);
        if (worstOf(s).r < worstOf(g.sample).r) g.sample = s;
        byRule.set(signature(s), g);
      }
      console.log(`  THIN MARGINS — ${thin.length} enforced site reading(s) in ${byRule.size} rule(s) clear their bar by`);
      console.log(`  less than ${THIN_MARGIN}. Every ratio here is measured against engine-resolved system colours`);
      console.log(`  (printed above), which move between Chrome builds and platforms; a rule this close is one`);
      console.log(`  build away from a red that looks like a regression and is not one.`);
      for (const g of [...byRule.values()].sort((a, b) => worstOf(a.sample).r - a.sample.bar - (worstOf(b.sample).r - b.sample.bar))) {
        const s = g.sample;
        const w = worstOf(s);
        const more = g.paths.size > 1 ? ` (+${g.paths.size - 1} more element(s), same rule)` : "";
        console.log(
          `      +${(w.r - s.bar).toFixed(2)}  ${round2(w.r).toFixed(2)}/bar ${s.bar}  ${s.scheme}  ` +
            `[${s.pseudo ?? (s.animated ? "animation" : "base")}]  ` +
            `${s.declared} painted ${s.painted} on ${w.against} :: ${s.path}${more}`,
        );
      }
    } else {
      console.log(`  THIN MARGINS — none: every enforced site clears its bar by at least ${THIN_MARGIN}.`);
    }

    // The limits, printed with the numbers rather than filed somewhere they can drift apart from
    // them. Kept in sync with the fuller versions in engine.mjs's header.
    console.log("");
    console.log(`── what this sweep does NOT see ─────────────────────────────────────────`);
    console.log("  1. The ratio ARITHMETIC is cross-checked by nothing external. --verify-pixels compares");
    console.log("     GROUNDS only, only for role=text, only in state=base; the maths itself is anchored");
    console.log("     against five known values and executed from ONE source string, never recomputed.");
    console.log("  2. Non-ancestor painters are invisible: groundOf composites the ancestor chain, so an");
    console.log("     element that OVERLAPS a site without containing it (the boot overlay is the worked");
    console.log("     example, which is why --verify-pixels has to hide it) contributes no ground at all.");
    console.log("  3. Non-chromatic non-text sites are dropped, un-enforced — the counts are printed above.");
    console.log("  4. Inline-assigned colours are measured but NOT enforced (the session-identity palette");
    console.log("     shared with the main app; CPE-1977 owns its numbers).");
    console.log("  5. Colours assigned inline from JS tables no fixture mounts are not measured AT ALL.");
    console.log("     STATE_META paints .state-dot #d08a1a / #3a72b5 / #3a9d4a in renderState(); the");
    console.log("     fixtures mount .state-dot at its CSS default #7a7a7a, which is non-chromatic and so");
    console.log("     dropped — those three appear nowhere here, not even under MEASURED, NOT ENFORCED.");
    console.log("     By hand: #d08a1a is 2.38:1 on a light tab, #3a9d4a is 2.86:1. Not a 1.4.11 failure");
    console.log("     (each dot has a title= and .pane-state spells the state out), but unmeasured, and in");
    console.log("     CPE-1977's scope with the chip palette.");
    console.log("");

    // Fixture completeness (CPE-1932): a styled class that matches nothing is a rule this sweep never
    // measured, and it must be declared rather than quietly skipped.
    const declared = new Set(res.unreachable.map(([c]) => c));
    const unmatched = res.classNames.filter((c) => !res.schemes.light.matched[c] && !declared.has(c));
    if (unmatched.length) {
      console.log("UNMEASURED — the stylesheet declares these classes but nothing on the page has them.");
      console.log("Add a fixture in fixtures.mjs, or a reason in its UNREACHABLE list:");
      for (const c of unmatched) console.log(`  .${c}`);
      console.log("");
    }

    if (failures.length) {
      const grouped = new Map();
      for (const s of failures) {
        const g = grouped.get(signature(s)) ?? { sample: s, paths: new Set() };
        g.paths.add(s.path);
        if (worstOf(s).r < worstOf(g.sample).r) g.sample = s;
        grouped.set(signature(s), g);
      }
      console.log(`FAIL — ${failures.length} enforced site reading(s) under bar, ${grouped.size} distinct rule(s):\n`);
      for (const g of [...grouped.values()].sort((a, b) => worstOf(a.sample).r - worstOf(b.sample).r)) {
        const s = g.sample;
        const w = worstOf(s);
        const more = g.paths.size > 1 ? ` (+${g.paths.size - 1} more element(s) with the same rule)` : "";
        console.log(
          `  ${s.scheme}  ${s.path}${more}\n` +
            `      ${s.prop}: ${s.declared} -> painted ${s.painted} on ${w.against} (${w.side})\n` +
            `      ${round2(w.r)}:1, below the ${s.bar}:1 bar` +
            (s.role === "text" ? `  (font-size ${s.size}px / weight ${s.weight})` : "") +
            `\n      state: ${s.state}\n`,
        );
      }
    }

    // The pixel leg is the PR's strongest claim — two independent paths agreeing — and the blocking
    // CI job is the one that passes --verify-pixels, so it has to be able to fail the run. Round 1
    // exited on `failures || unmatched` alone: total disagreement exited 0. Both halves count now.
    if (pixelBad) {
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK FAILED — ${pixelBad} ground(s) painted a colour the computed-style path did not\n` +
          "predict (listed above). Either the compositing model is wrong or the screenshot is of a\n" +
          "different page than the one measured; every ratio in this report rests on the model being right.",
      );
    }
    if (pixelEmpty.length) {
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK DID NOT RUN — the ${pixelEmpty.join(" and ")} screenshot pass verified ZERO grounds.\n` +
          '"0 verified, 0 disagreeing" is not a clean bill; it is the leg failing to measure anything, and\n' +
          "this repo treats \"did not run\" as a failure rather than as \"found nothing\".",
      );
    }

    const clean = !failures.length && !unmatched.length && !pixelBad && !pixelEmpty.length;
    if (clean) console.log("PASS — every enforced site clears its bar in both schemes.");
    return clean ? 0 : 1;
  });
}

main().then(
  (code) => process.exit(code),
  (err) => {
    console.error(err?.stack || String(err));
    process.exit(2);
  },
);
