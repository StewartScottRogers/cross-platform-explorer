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
// EXIT CODE is the point: non-zero the moment any ENFORCED site is under its bar, naming the site,
// the two colours, the ground it was measured against, and the bar it missed.

import { sweep, round2, CHROMA_MIN, ANIM_SAMPLES } from "./engine.mjs";

const argv = process.argv.slice(2);
const flag = (n) => argv.includes(n);
const opt = (n) => { const i = argv.indexOf(n); return i >= 0 ? argv[i + 1] : undefined; };

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
    console.log("WCAG implementation validated against known anchors BEFORE measuring anything:");
    for (const [k, v] of Object.entries(res.anchors)) console.log(`  ${k.padEnd(42)} ${v}`);
    console.log("");

    for (const scheme of ["light", "dark"]) {
      const d = res.schemes[scheme];
      console.log(`${scheme}: engine-resolved Canvas=${d.canvas} CanvasText=${d.canvasText} Field=${d.field} ButtonFace=${d.buttonFace}`);
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
    for (const scheme of ["light", "dark"]) {
      const d = res.schemes[scheme];
      console.log(`  ${scheme}: ${d.forced.length} forced pseudo-state readings, ${d.animations} CSS animations x ${ANIM_SAMPLES} frames`);
      if (d.pixels) {
        const bad = d.pixels.filter((p) => p.delta > 1);
        console.log(`  ${scheme}: pixel cross-check — ${d.pixels.length} grounds screenshot-verified, ${bad.length} disagreeing by more than 1/255`);
        for (const p of bad.slice(0, 8)) console.log(`      predicted ${p.predicted} painted ${p.painted} (delta ${p.delta})  ${p.path}`);
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
    } else {
      console.log("PASS — every enforced site clears its bar in both schemes.");
    }
    return failures.length || unmatched.length ? 1 : 0;
  });
}

main().then(
  (code) => process.exit(code),
  (err) => {
    console.error(err?.stack || String(err));
    process.exit(2);
  },
);
