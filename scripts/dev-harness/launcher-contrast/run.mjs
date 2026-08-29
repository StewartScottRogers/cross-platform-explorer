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
//   --json             dump the whole measurement as JSON on stdout, with a `verdict` object, and
//                      exit on the SAME verdict the report would (round 2's --json returned 0
//                      unconditionally: it ran the entire sweep and evaluated nothing)
//
// EXIT CODE is the point. 1 the moment any ENFORCED site is under its bar — naming the site, the two
// colours, the ground it was measured against, and the bar it missed — and equally on any of:
//   * a styled class no fixture mounts (the fixture-completeness enumeration, CPE-1932);
//   * a `--verify-pixels` ground the screenshot painted differently from the prediction;
//   * a `--verify-pixels` pass that verified ZERO grounds, because "0 verified, 0 disagreeing" reads
//     as success and is really the leg not running (this repo's "did not run" != "found nothing");
//   * ANY of the other legs measuring nothing — no base readings, no forced pseudo-states, no
//     animation frames actually stepped, a state rule the engine refused to select, or (CPE-1977) no
//     INLINE site at all, meaning the fixtures stopped mounting the launcher's JS-painted palettes.
//     See `legsThatDidNotRun` for the sabotages that used to print PASS and exit 0.
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
 * Grounds that CANNOT sample as one flat colour, declared one at a time with the reason.
 *
 * The flatness condition is fatal (see `analyse`), so this list is the only way past it, and it is
 * deliberately a list of selectors rather than a tolerance: a global "allow 3 stray pixels" would have
 * covered round 5's 13-of-45 defect too. Each entry has to name a painter that is inside the element's
 * border box, is not the ground, and is not something this harness can suppress.
 *
 * Adding an entry is a real decision — the site stops being evidence that the compositing model is
 * right, and keeps only the weaker majority check. Prefer fixing the sampler.
 *
 * **This is a RATCHET, and it is named so the class guard can see it (CPE-1934, round 7).** It was
 * `NOT_FLAT_BY_DESIGN`, which contains none of `ratchetBaselines.test.ts`'s vocabulary words, so
 * appending a second entry reddened nothing — an unratcheted allowlist inside a PR whose subject is
 * unratcheted allowlists. It is now registered as `launcher-contrast-not-flat-exemptions` in
 * `scripts/ratchet-baselines.mjs` and carries a row in `docs/design/RATCHETS.md`, so growing it needs
 * a declared, ticketed raise.
 *
 * It earns that more than most: an exempted site keeps ONLY the majority check, and the margin
 * between the majority bar and the known glyph defect is exactly one sample — the bar needs 23 of 45
 * and the sabotage in `engine.mjs`'s red-proof B1 measured the weakest agreement at 51%, i.e. 23.
 */
const NOT_FLAT_BY_DESIGN_EXEMPTIONS = [
  {
    match: /\bselect#/,
    why:
      "a native <select> paints a UA dropdown arrow inside its own border box. It is foreground " +
      "content exactly like a glyph, but it is drawn by the widget rather than as text, so " +
      "`-webkit-text-fill-color: transparent` cannot hide it and no inset can exclude it without " +
      "excluding most of the control. Measured: 44 of 45 samples are still the ground.",
  },
];

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
  // THE INLINE EXEMPTION IS GONE (CPE-1977). It used to read `if (s.inlineSelf || s.inlineGround)
  // return false`, on two grounds: a JS-assigned colour is not in the stylesheet this harness guards,
  // and the session-identity palette is shared with the main app so retuning it is an app-wide
  // decision rather than a launcher CSS fix. The second reason expired when CPE-1977 made that
  // decision and pinned both copies to each other; the first never held up — the exemption was about
  // where a colour is AUTHORED, and every bar in here is about what a user can SEE.
  //
  // What it cost while it stood: the sweep reported PASS with `#2aa1a1` carrying white text at 3.13:1
  // and sitting on a hovered light tab at 2.42:1, one hash bucket in eight away from any session.
  // "Measured, not enforced" printed the numbers honestly, and nothing read them for eleven days.
  //
  // The argument for keeping it was fragility: an inline site's ground is composited from
  // engine-resolved system colours that move between Chrome builds. That is true — and it is equally
  // true of the 28 stylesheet readings already in THIN MARGINS at +0.05. Exempting one class of site
  // from a fragility the rest of the sweep already lives with is not caution, it is a blind spot with
  // a rationale attached.
  //
  // RED-PROOFED: with `#2aa1a1` put back into SESSION_CHIP_COLORS this exits 1 on
  // `#ffffff on #2aa1a1 — 3.13:1, below the 4.5:1 bar` in both schemes and both tab states. That is
  // the exact reading the old exemption printed under "measured, not enforced" while exiting 0.
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

/**
 * DID EACH LEG ACTUALLY RUN? — a per-scheme floor on WORK DONE, for every leg this harness exists to
 * add.
 *
 * Round 2 enforced this for exactly one of the four legs (the pixel cross-check), and the other three
 * could measure NOTHING and still print `PASS`, exit 0:
 *
 *   - STATES: `const stateRules = []` gave 844 raw readings instead of 1306, 244 enforced instead of
 *     384, a log line reading "0 forced pseudo-state readings", and `PASS`. It did not even need an
 *     edit — the rule loop's `catch { continue; }` was bare, so one CDP change would have skipped
 *     every rule silently.
 *   - TIME: `if (false && animMeta.targets.length)` took zero frames, exited 0, and the report STILL
 *     printed "3 CSS animations x 21 frames", because that count came from `animMeta.count` — the
 *     page's metadata — rather than from readings taken. A report that claims a leg ran is worse than
 *     one that says nothing.
 *   - COMPUTED-STYLE (base): `const all = []` gave "0 raw readings -> 0 distinct sites, 0 enforced"
 *     followed by "PASS — every enforced site clears its bar in both schemes." under plain
 *     `npm run harness:launcher-contrast`. `--verify-pixels` caught it, but only incidentally (the
 *     pixel leg is fed from `all`), and the no-flag invocation is the documented local one.
 *
 * ── What the checks below actually are (corrected in round 4, polarity named in round 5) ─────────
 *
 * There are **SIX** checks per scheme, not three, and they do not all come from the same place. Say
 * this precisely, because "the floors are taken out of `all`" was the round-3 wording and it was
 * wrong by half:
 *
 *   - THREE are `all.filter(...)` in `engine.mjs` — `baseReadings`, `stateReadings`, `animReadings`.
 *     `all` is the one array the report is built from, so a sabotage that empties it reds all three
 *     at once and cannot be survived by a leg's own bookkeeping. These are the load-bearing ones.
 *   - THREE read LEG-LOCAL state — `forced.length` (pseudo-states actually forced), `stateSkips`
 *     (a skip counter, which is correctly local: it counts rules that produced no reading, so it
 *     could not be derived from readings), and `animFrames` (frames actually stepped).
 *
 * FIVE of the six are floors; **`stateSkips` is a CEILING** and reds when it is ABOVE zero, because
 * the thing it counts is rules that produced no reading. Calling all six "floors" is loose — the
 * surrounding text says what each one counts, but the polarity is worth naming rather than inferring.
 *
 * The property still holds, and it is the property rather than the provenance that matters: **every
 * leg has at least one `all`-derived floor**, so no leg can measure nothing and pass. `animFrames`
 * is leg-local but is incremented only AFTER a frame's probe has run and pushed into `all`, so it
 * counts work rather than intent — which is exactly the distinction `animations` failed. That page
 * metadata count is still printed, but beside `animFrames` rather than in place of it, and it is
 * never floored.
 */
function legsThatDidNotRun(res) {
  const out = [];
  for (const scheme of ["light", "dark"]) {
    const d = res.schemes[scheme];
    if (!(d.baseReadings > 0)) {
      out.push(`${scheme}: the COMPUTED-STYLE leg took ${d.baseReadings} base readings — it measured nothing at all`);
    }
    if (!(d.forced.length > 0)) {
      out.push(
        `${scheme}: the STATES leg forced 0 pseudo-states, from ${d.stateRuleCount} ` +
          ":hover/:focus/:active rule(s) found in the stylesheet",
      );
    }
    if (!(d.stateReadings > 0)) {
      out.push(`${scheme}: the STATES leg took 0 readings while a pseudo-state was forced`);
    }
    if (d.stateSkips.length) {
      out.push(
        `${scheme}: the STATES leg SKIPPED ${d.stateSkips.length} rule(s) it could not select — every ` +
          "one is a state nothing measured:\n        " + d.stateSkips.join("\n        "),
      );
    }
    if (!(d.animFrames > 0)) {
      out.push(
        `${scheme}: the TIME leg stepped 0 animation frames (the page reports ${d.animations} ` +
          `animation object(s) on ${d.animTargets} element(s) — that is intent, not work)`,
      );
    }
    if (!(d.animReadings > 0)) {
      out.push(`${scheme}: the TIME leg took 0 readings across the ${d.animFrames} frame(s) it stepped`);
    }
    // The INLINE leg (CPE-1977). The launcher paints two JS tables inline — SESSION_CHIP_COLORS and
    // STATE_META — and both are now enforced rather than exempted. An inline population of ZERO is
    // not "the palette is fine", it is the fixtures having stopped expanding `__PALETTE_CHIPS__` /
    // `__STATE_DOTS__`, and it prints exactly like a clean sweep. STATE_META spent CPE-1966 in
    // precisely that state: mounted at its CSS default, measured, dropped, reported nowhere.
    const inline = (d.sites ?? []).filter((s) => s.inlineSelf || s.inlineGround);
    if (!(inline.length > 0)) {
      out.push(
        `${scheme}: the INLINE leg measured 0 site(s) painted from the launcher's own JS tables ` +
          "(SESSION_CHIP_COLORS / STATE_META) — the fixtures are no longer mounting them",
      );
    }
  }
  return out;
}

/**
 * Everything the exit code depends on, computed ONCE so `--json` renders the same verdict the report
 * does. Round 2's `--json` returned 0 unconditionally: it dumped the measurement and evaluated
 * nothing, so `--json` was a way to run the whole sweep and never be told it failed.
 */
function analyse(res) {
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

  // Fixture completeness (CPE-1932): a styled class that matches nothing is a rule this sweep never
  // measured, and it must be declared rather than quietly skipped.
  const declared = new Set(res.unreachable.map(([c]) => c));
  const unmatched = res.classNames.filter((c) => !res.schemes.light.matched[c] && !declared.has(c));

  // The pixel leg's THREE failure modes. `pixelBad` is a real disagreement between the two paths;
  // `pixelEmpty` is a leg that ran and measured nothing, which prints as "0 verified, 0 disagreeing"
  // and reads exactly like success.
  //
  // `pixelBad` is round 6's, and it REPLACES two weaker things: the old "the mode equals the
  // prediction" and round 5's "the sample must be unanimous". It states the claim the leg exists to
  // check, directly — **most of the element's interior is painted the colour the model predicted** —
  // and nothing else.
  //
  // Why the mode alone was not enough: it can win a plurality. Round 5 measured a 25x14 site whose
  // sample held 28 distinct colours and whose mode won with 13 of 45, and the verdict was decided by
  // which antialiased glyph pixel happened to come first. Same commit, 0 disagreements on Windows and
  // 3 on ubuntu-latest. Suppressing glyph fill for the screenshot fixed the cause of that one.
  //
  // Why unanimity was the wrong replacement, and this is round 6's lesson: **it measures the page, not
  // the model.** It reddened CI on `.tab-close` at 35/45 and 40/45 while every one of the 120 grounds
  // agreed with the prediction to within 1/255 — the model was right and the check said otherwise. A
  // real page has interior pixels that are not ground and never will be: a `<select>`'s UA-painted
  // arrow, an antialiased corner where an element overflows its parent. Demanding they vanish is
  // demanding the page be simpler than it is.
  //
  // A strict majority is not a tuned epsilon; it is what "this is the background" MEANS. And it is not
  // a loosening in the direction that matters: a wrong ground prediction agrees with ~nothing, so it
  // reds harder here than it ever did against the mode. Measured margins on this page below.
  let pixelBad = 0;
  const pixelWeak = [];
  const pixelEmpty = [];
  const pixelUnreadable = [];
  const pixelOffscreen = [];
  const pixelNotFlat = [];
  const pixelUnrendered = [];
  for (const scheme of ["light", "dark"]) {
    const d = res.schemes[scheme];
    if (!d.pixels) continue;
    if (d.pixels.length === 0) pixelEmpty.push(scheme);
    // DETERMINACY, kept fatal. With glyph fill suppressed and the safe box derived, a ground reads as
    // ONE colour across all 45 samples — measured 114/118 on this page. The four that do not are
    // declared below by selector, with the reason; anything else that goes non-flat reds until someone
    // looks at it and either fixes the model or declares it here. That is the difference between a
    // named exemption and a global tolerance.
    for (const p of d.pixels) {
      if (p.share === p.total) continue;
      if (NOT_FLAT_BY_DESIGN_EXEMPTIONS.some((x) => x.match.test(p.path))) continue;
      pixelNotFlat.push(
        `${scheme}: ${p.path} sampled ${p.distinct} colour(s) in ${p.total} interior points ` +
          `(mode ${p.painted} x${p.share}; border box ${p.rect.w}x${p.rect.h}, safe box ` +
          `${p.rect.sw}x${p.rect.sh}) — the ground is not flat and no declared exemption covers it`,
      );
    }
    for (const p of d.pixels) {
      if (p.agreeing * 2 > p.total) continue;
      pixelBad++;
      pixelWeak.push(
        `${scheme}: ${p.path} — the model predicted ${p.predicted}, and only ${p.agreeing} of ` +
          `${p.total} interior samples are within 1/255 of it (mode ${p.painted} x${p.share}, ` +
          `${p.distinct} distinct colour(s); border box ${p.rect.w}x${p.rect.h}, safe box ` +
          `${p.rect.sw}x${p.rect.sh})`,
      );
    }
    // A site the sampler could not read at all. Reported rather than dropped from the denominator:
    // a silently skipped site prints exactly like a site that passed, which is round 5's defect in a
    // different costume. These were being dropped before round 6 — all `counts.size === 0`, silently
    // `continue`d.
    //
    // Round 7 is why that sentence no longer ends "so '59 verified' was really '59 verified and 4 not
    // mentioned'." That figure was measured at ONE of the two silent drops. The line immediately
    // above the one round 6 fixed was still a bare `continue` — the `w < 10 || h < 10` guard in
    // `engine.mjs` — and instrumented it drops **161 sites per scheme** against those 4. Both are now
    // collected, and the not-verified population is printed as its own counted buckets instead of
    // being summarised in a sentence.
    for (const u of d.pixelUnsamplable ?? []) {
      (u.offscreen ? pixelOffscreen : pixelUnreadable).push(`${scheme}: ${u.path} — ${u.reason}`);
    }
    // NOT FATAL, and the reason is the leg's stated scope rather than a judgement call: the pixel
    // cross-check screenshots ONE state (base), so every site that only exists inside a panel this
    // state does not show is a zero-size box here. That is out of scope, not unverified-and-ignored —
    // but the two print identically when one of them prints nothing at all, which is the whole point
    // of the bucket. Printed grouped by selector, since this bucket is much larger than the others.
    for (const u of d.pixelUnrendered ?? []) pixelUnrendered.push({ scheme, ...u });
  }

  const legsDown = legsThatDidNotRun(res);
  const clean = !failures.length && !unmatched.length && !pixelBad && !pixelEmpty.length
    && !pixelUnreadable.length && !pixelNotFlat.length && !legsDown.length;
  return {
    measured, sites, checked, failures, unmatched,
    pixelBad, pixelWeak, pixelEmpty, pixelUnreadable, pixelOffscreen, pixelNotFlat, pixelUnrendered,
    legsDown, clean,
  };
}

function main() {
  return sweep({ verifyPixels: flag("--verify-pixels") }).then((res) => {
    const a = analyse(res);

    if (flag("--json")) {
      // The verdict travels WITH the data, and the exit code is the same one the report would give.
      process.stdout.write(JSON.stringify({
        ...res,
        verdict: {
          clean: a.clean,
          rawReadings: a.measured,
          distinctSites: a.sites.length,
          enforced: a.checked.length,
          failures: a.failures.length,
          unmatchedClasses: a.unmatched,
          pixelDisagreements: a.pixelBad,
          pixelLegEmpty: a.pixelEmpty,
          pixelGroundsUnderMajority: a.pixelWeak,
          pixelGroundsUnreadable: a.pixelUnreadable,
          pixelGroundsOffscreen: a.pixelOffscreen,
          pixelGroundsNotFlat: a.pixelNotFlat,
          // Not verified because the site does not render in the ONE state this leg screenshots.
          // Non-fatal and in the verdict anyway: what was not verified is several counted buckets,
          // not one number and not silence.
          pixelGroundsNotRendered: a.pixelUnrendered.map((u) => `${u.scheme}: ${u.path} — ${u.reason}`),
          legsThatDidNotRun: a.legsDown,
        },
      }, null, 2));
      return a.clean ? 0 : 1;
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

    const { measured, sites, checked, failures, unmatched, pixelBad, pixelWeak, pixelEmpty, pixelUnreadable, pixelOffscreen, pixelNotFlat, pixelUnrendered, legsDown } = a;

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

    // Per-leg WORK DONE, per scheme. Every count here comes from readings that exist: `animFrames`
    // and `animReadings` are incremented inside the frame loop, never read off `animMeta.count`,
    // which is the metadata that let round 2 print "3 CSS animations x 21 frames" for a leg that
    // took no frames at all. The page's own count is still shown, in brackets, so the two can be
    // compared rather than confused. `legsThatDidNotRun()` floors all of them below.
    for (const scheme of ["light", "dark"]) {
      const d = res.schemes[scheme];
      console.log(
        `  ${scheme}: ${d.baseReadings} base readings; ${d.forced.length} forced pseudo-states over ` +
          `${d.stateRuleCount} rule(s) -> ${d.stateReadings} readings` +
          (d.stateSkips.length ? ` (${d.stateSkips.length} SKIPPED)` : ""),
      );
      console.log(
        `  ${scheme}: ${d.animFrames} animation frames stepped on ${d.animTargets} element(s) -> ` +
          `${d.animReadings} readings  [page reports ${d.animations} animation object(s); ANIM_SAMPLES=${ANIM_SAMPLES}]`,
      );
      if (d.pixels) {
        const bad = d.pixels.filter((p) => p.agreeing * 2 <= p.total);
        // The MARGIN, printed every run rather than only on a failure. A ground where the prediction
        // covers 100% of the interior and one where it scrapes 51% print the same verdict otherwise,
        // and the distance between them is the only warning anyone gets before a red.
        const worst = d.pixels.reduce((m, p) => Math.min(m, p.agreeing / p.total), 1);
        const flat = d.pixels.filter((p) => p.share === p.total).length;
        const off = (d.pixelUnsamplable ?? []).filter((u) => u.offscreen).length;
        // The DENOMINATOR. "N verified" on its own is only honest if nothing else happened, and three
        // other things can: a site off the viewport, a site whose safe box collapsed, and — by far the
        // largest — a site that does not render at all in the one state this leg screenshots. The
        // first and third are counted on this line; the second is FATAL and gets its own block below,
        // so it can never be read as a footnote. Round 7 added the third; before it, that population
        // was not printed anywhere.
        const notRendered = (d.pixelUnrendered ?? []).length;
        console.log(`  ${scheme}: pixel cross-check — ${d.pixels.length} grounds screenshot-verified, ${bad.length} where the prediction is not most of the interior` +
          (off ? `, ${off} UNVERIFIED (off the captured viewport)` : "") +
          (notRendered ? `, ${notRendered} NOT RENDERED in the screenshotted state` : ""));
        console.log(`      weakest agreement ${(worst * 100).toFixed(0)}% of samples within 1/255 of the prediction (bar: >50%); ${flat}/${d.pixels.length} grounds sampled a single flat colour`);
        for (const p of bad.slice(0, 8)) console.log(`      predicted ${p.predicted} painted ${p.painted} (delta ${p.delta})  ${p.path}  [agreeing ${p.agreeing}/${p.total}, ${p.distinct} distinct]`);
        if (bad.length > 8) console.log(`      ... and ${bad.length - 8} more`);
      }
    }
    console.log(`  chromatic threshold for non-text roles: max-min channel >= ${CHROMA_MIN}`);

    // INLINE-ASSIGNED sites, printed every run. This block used to be "MEASURED, NOT ENFORCED" and
    // listed the readings the exemption let through; CPE-1977 removed the exemption, so what has to
    // be printed now is the opposite fact — that these sites exist AND are being enforced. A section
    // that only appears when something is wrong cannot distinguish "nothing is wrong" from "nothing
    // was measured", which is how STATE_META stayed invisible; `legsThatDidNotRun` reds on an empty
    // population, and this prints the count either way.
    const inline = sites.filter((s) => s.inlineSelf || s.inlineGround);
    const inlineEnforced = inline.filter(enforced);
    const inlineWorst = [...inlineEnforced].sort((a, b) => (worstOf(a).r - a.bar) - (worstOf(b).r - b.bar));
    console.log("");
    console.log(`  INLINE-ASSIGNED — ${inline.length} site(s) painted by the launcher's own JS`);
    console.log(`  (SESSION_CHIP_COLORS via sessionColor(), STATE_META via renderState()); ${inlineEnforced.length} enforced,`);
    console.log(`  ${inlineEnforced.filter((s) => worstOf(s).r < s.bar).length} under bar. Tightest first:`);
    const seen = new Set();
    for (const s of inlineWorst) {
      const sig = `${s.scheme}|${s.role}|${s.painted}|${worstOf(s).against}`;
      if (seen.has(sig)) continue;
      seen.add(sig);
      if (seen.size > 8) break;
      console.log(`      ${round2(worstOf(s).r).toFixed(2).padStart(5)}:1 (bar ${s.bar})  ${s.scheme}  ${s.role}  ${s.painted} on ${worstOf(s).against}  ${s.path}`);
    }

    // The rest of the 786. Round 1's report accounted for the 384 enforced and gave the 9 inline
    // ones their own section, and was silent about the remainder — which is most of them. Silence
    // reads as "there was nothing there"; these are a JUDGEMENT (SC 1.4.11 excludes decorative
    // separators) and the count belongs in the log where the judgement can be checked, with `--all`
    // as the way to see every one.
    const dropped = sites.filter((s) => !enforced(s));
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
    console.log("  4. Inline-assigned colours ARE enforced now (CPE-1977 dropped the exemption), but only");
    console.log("     for the JS tables a fixture mounts. A THIRD table painted inline from JS would be");
    console.log("     invisible here exactly as STATE_META was, and nothing in this harness can derive");
    console.log("     that a table exists — `stateDotColours()` had to be written by hand and pointed at");
    console.log("     it. The counted INLINE-ASSIGNED population above is what makes the absence loud.");
    console.log("  5. The main app's copy of the chip palette (src/lib/sessionChip.ts) is NOT swept here:");
    console.log("     this harness loads launcher.html and nothing else. The two are pinned equal by");
    console.log("     src/lib/sessionChip.test.ts, so the VALUES cannot drift — but the app's own grounds");
    console.log("     (--surface / --hover across four themes, under .agent-chip and .menu-chip) are");
    console.log("     measured by neither this harness nor that test. No number is quoted here for them");
    console.log("     because nothing in the tree measures one; that is the gap, not a footnote to it.");
    console.log("");

    // Fixture completeness (CPE-1932): a styled class that matches nothing is a rule this sweep never
    // measured, and it must be declared rather than quietly skipped. (Computed in `analyse`.)
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
        `PIXEL CROSS-CHECK FAILED — for ${pixelBad} ground(s) the colour the computed-style path predicted is\n` +
          "NOT most of what the screenshot actually painted inside the element. Either the compositing\n" +
          "model is wrong or the screenshot is of a different page than the one measured; every ratio in\n" +
          "this report rests on the model being right.",
      );
      for (const l of pixelWeak.slice(0, 8)) console.log(`      ${l}`);
      if (pixelWeak.length > 8) console.log(`      ... and ${pixelWeak.length - 8} more`);
    }
    if (pixelNotFlat.length) {
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK IS NOT WELL-DETERMINED — ${pixelNotFlat.length} ground(s) did not sample a single\n` +
          "flat colour, and no entry in NOT_FLAT_BY_DESIGN_EXEMPTIONS covers them. Something is painted inside those\n" +
          "elements that the compositing model does not know about, or the safe sample box is reaching\n" +
          "geometry it should be excluding. Fix the model or the sampler; declaring a new exemption costs\n" +
          "that site's evidence, so it is the last resort rather than the first.",
      );
      for (const l of pixelNotFlat.slice(0, 8)) console.log(`      ${l}`);
      if (pixelNotFlat.length > 8) console.log(`      ... and ${pixelNotFlat.length - 8} more`);
    }
    if (pixelOffscreen.length) {
      // NOT fatal, and printed anyway. A screenshot of a viewport cannot verify what is outside the
      // viewport; that is a limit of the method, like "grounds only, role=text, state=base". What
      // WOULD be dishonest is leaving these out of the count, which is what happened before round 6:
      // every one of them sampled zero in-bounds pixels and was silently `continue`d.
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK LEFT ${pixelOffscreen.length} GROUND(S) UNVERIFIED — they lie outside the captured\n` +
          "viewport, so no screenshot can speak to them. Reported rather than dropped: this is coverage\n" +
          "the leg does not have, not coverage it has and passed.",
      );
      for (const l of pixelOffscreen.slice(0, 8)) console.log(`      ${l}`);
      if (pixelOffscreen.length > 8) console.log(`      ... and ${pixelOffscreen.length - 8} more`);
    }
    if (pixelUnreadable.length) {
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK COULD NOT READ ${pixelUnreadable.length} GROUND(S) — the safe sample box collapsed.\n` +
          "These are NOT counted as verified and NOT quietly dropped: a site removed from the denominator\n" +
          "prints exactly like a site that passed. Either the element is too small to read a ground out of,\n" +
          "or it overflows an ancestor so far that nothing inside it is reliably that ancestor's paint.",
      );
      for (const l of pixelUnreadable.slice(0, 8)) console.log(`      ${l}`);
      if (pixelUnreadable.length > 8) console.log(`      ... and ${pixelUnreadable.length - 8} more`);
    }
    if (pixelUnrendered.length) {
      // NOT fatal, and the largest of the three not-verified buckets by a wide margin. It exists
      // because the alternative — an unconditional `continue` in the sampler — made these sites print
      // exactly like sites that passed, which is the same defect round 6 fixed one line further down
      // and measured at 4. This one is measured at 161 per scheme.
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK DID NOT REACH ${pixelUnrendered.length} SITE(S) — they render at a size no ground can\n` +
          "be read out of (mostly 0x0) in the ONE state this leg screenshots. That is the leg's stated scope,\n" +
          "not a disagreement — but scope has to be COUNTED, or \"N verified\" reads as \"N is all there was\".\n" +
          "Grouped by selector; the count after each is light+dark combined.",
      );
      const grouped = new Map();
      for (const u of pixelUnrendered) {
        const size = /^border box ([0-9.]+x[0-9.]+)/.exec(u.reason);
        // Named `bucketKey`, not `key`: `key` is this file's site-identity helper and shadowing it
        // inside a reporting block is the kind of thing that reads fine and behaves surprisingly.
        const bucketKey = `${u.path} @ ${size ? size[1] : "not in the document"}`;
        grouped.set(bucketKey, (grouped.get(bucketKey) ?? 0) + 1);
      }
      const ranked = [...grouped].sort((a2, b2) => b2[1] - a2[1]);
      for (const [k, n] of ranked.slice(0, 12)) console.log(`      x${String(n).padStart(3)}  ${k}`);
      if (ranked.length > 12) console.log(`      ... and ${ranked.length - 12} more distinct selector/size pairs`);
    }
    if (pixelEmpty.length) {
      console.log("");
      console.log(
        `PIXEL CROSS-CHECK DID NOT RUN — the ${pixelEmpty.join(" and ")} screenshot pass verified ZERO grounds.\n` +
          '"0 verified, 0 disagreeing" is not a clean bill; it is the leg failing to measure anything, and\n' +
          "this repo treats \"did not run\" as a failure rather than as \"found nothing\".",
      );
    }

    // The other three legs' floors. Same rule as the pixel one above, applied to the legs this PR
    // exists to add: a leg that measured nothing prints numbers that read like a clean bill.
    if (legsDown.length) {
      console.log("");
      console.log(
        `LEG(S) DID NOT RUN — ${legsDown.length} floor(s) on WORK ACTUALLY DONE were not met. A sweep that\n` +
          "measured nothing prints the same PASS as one that measured everything, so this is a failure\n" +
          'rather than a quiet zero (this repo\'s "did not run" != "found nothing"):',
      );
      for (const l of legsDown) console.log(`      ${l}`);
    }

    if (a.clean) console.log("PASS — every enforced site clears its bar in both schemes.");
    return a.clean ? 0 : 1;
  });
}

main().then(
  (code) => process.exit(code),
  (err) => {
    console.error(err?.stack || String(err));
    process.exit(2);
  },
);
