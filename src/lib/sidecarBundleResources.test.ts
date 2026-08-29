// CPE-1271: guard the SHIPPED sidecar bundle against silently missing runtime resources.
//
// This bug class bit twice in one session:
//   - CPE-1258/1267: pdfium.dll path (thumbnails) — resource_dir/bundle mismatch.
//   - CPE-1270: `icons/icon.png` (drag-out preview) — the base tauri.conf.json's ARRAY-form
//     `bundle.resources` (`["icons/icon.png"]`) was silently REPLACED by the sidecar overlays'
//     OBJECT-form `bundle.resources`, because Tauri's `--config` merge treats an overlay whose
//     value at a key is a plain object as a wholesale REPLACEMENT of a base value that isn't
//     also a plain object (an array included) — it does NOT concatenate/union them.
//
// The app the user actually runs is the release-sidecar.yml build: the base `tauri.conf.json`
// with a chain of `--config` overlays applied on top, in a specific order. Nothing verified that
// the FINAL merged `bundle.resources` still contains every resource the runtime code resolves —
// this test is that guard.
//
// CPE-1900 — that chain used to be a `CONFIG_CHAIN` LITERAL here, with a comment asking the reader
// to "keep this in lockstep" with `release-sidecar.yml`. Nothing read the workflow, so a fifth
// `--config` overlay would have shipped into the config every install runs on while this file stayed
// green and narrower than it looked. It is now derived — see the two-halves note on
// {@link configChainForLeg}, which states which half is derived and which is enumerated.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStrSliceAfter, rustStrConstAfter } from "./rustSource";
import guardCases from "./platformConfigGuard.cases.json";
import {
  BASE_CONFIG,
  TAURI_PROJECT_DIR,
  deriveBuildLegs,
  type BuildLeg,
  type ShipOS,
} from "./tauriConfigChain";

const SRC_TAURI = join(process.cwd(), TAURI_PROJECT_DIR);

/**
 * Every `tauri-action` build in the repo, matrix-expanded, with its `--config` overlay chain read out
 * of the workflow's own `args:` — see `src/lib/tauriConfigChain.ts`. Derived at module load, so a
 * discovery that comes back near-empty (or a workflow this guard cannot understand) fails collection
 * loudly instead of leaving every assertion below sweeping nothing.
 *
 * This covers BOTH release channels, not just the sidecar one. The plain `release.yml` passes no
 * `--config` overlay today, so its chain is the base config alone — but that is now a MEASUREMENT
 * rather than an assumption, and the day someone adds an overlay there the updater pin below extends
 * to it with nobody editing anything.
 */
const BUILD_LEGS: BuildLeg[] = deriveBuildLegs();

/** The channel `/run` installs and every install auto-updates from ([[always-install-sidecar-build]]). */
const SIDECAR_WORKFLOW = ".github/workflows/release-sidecar.yml";

/** The sidecar channel's legs, one per shipped OS. */
const SIDECAR_LEGS: BuildLeg[] = BUILD_LEGS.filter((l) => l.workflow === SIDECAR_WORKFLOW);

function loadConfig(repoRelativePath: string): unknown {
  return JSON.parse(readFileSync(join(process.cwd(), repoRelativePath), "utf8"));
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Tauri's own `--config` merge semantics, reduced to what this guard needs: when the SAME key is
 * a plain object on both sides, merge recursively (key-wise); otherwise the overlay's value wins
 * OUTRIGHT, replacing the base's value with no attempt to combine them. That "outright replace"
 * branch is exactly the CPE-1270 footgun: a base ARRAY meeting an overlay OBJECT at
 * `bundle.resources` doesn't union — the array is gone, and only the overlay's keys survive.
 */
function mergeJson(base: unknown, overlay: unknown): unknown {
  if (overlay === undefined) return base;
  if (isPlainObject(base) && isPlainObject(overlay)) {
    const merged: Record<string, unknown> = { ...base };
    for (const key of Object.keys(overlay)) {
      merged[key] = mergeJson(base[key], overlay[key]);
    }
    return merged;
  }
  return overlay;
}

/**
 * Every config file Tauri merges AUTOMATICALLY for one platform, with no `--config` flag — repo-
 * relative, sorted. See the CPE-1903 block further down for the mechanism
 * (`tauri-utils::config::parse::read_from`) and for why this is a directory LISTING classified by
 * shape rather than a lookup of remembered filenames.
 *
 * Today this returns `[]` for every OS: no such file exists in the tree. That is the point — it is a
 * measurement of the directory on each run, so the day one appears it enters the chain below instead
 * of being invisible to it.
 *
 * Fail-closed twice over. TWO files for the same platform throws, because Tauri's `do_parse` falls
 * through json -> json5 -> toml and picks one, and a guard that merged both would be describing a
 * build nobody runs. A single file that is not strict JSON also throws, because no JSON5/TOML parser
 * is carried here on purpose (same decision as `platformConfigUpdaterRefusal`) and silently skipping
 * a file Tauri merges is precisely the failure this whole file exists to stop.
 *
 * **RED-PROOFED — this half can fire, and it fires somewhere new (2026-08-28, measured locally, every
 * fixture deleted afterwards).** Before CPE-1900 the auto-merged half was covered ONLY by the CPE-1903
 * refusal further down; it never entered a merged config, so the pins could not see it.
 *
 *   - `src-tauri/tauri.windows.conf.json` holding an attacker `plugins.updater` block: **5 failed /
 *     36 passed**, and crucially the merged-config pin is now among them — both *windows* legs
 *     (sidecar and plain) red on pubkey and endpoints, plus the CPE-1903 refusal. Only windows,
 *     which is correct: that file governs no other platform's build.
 *   - `src-tauri/Tauri.linux.toml` (a format this guard deliberately cannot parse): **8 failed / 33
 *     passed**, refused rather than skipped.
 *   - `tauri.macos.conf.json` and `tauri.macos.conf.json5` together: **8 failed / 33 passed**, the
 *     ambiguity refused rather than guessed at.
 */
function autoMergedPlatformConfigs(os: ShipOS): string[] {
  const matches = readdirSync(SRC_TAURI)
    .filter(
      (name) => isAutoMergedPlatformConfigName(name) && asciiLower(name).split(".")[1] === os,
    )
    .sort();
  if (matches.length > 1) {
    throw new Error(
      `${os}: src-tauri/ holds ${matches.length} auto-merged per-platform config files ` +
        `(${matches.join(", ")}). Tauri picks ONE of json/json5/toml, so which of these governs the ` +
        `shipped build is ambiguous and this guard refuses to guess. Delete all but one.`,
    );
  }
  for (const name of matches) {
    try {
      JSON.parse(readFileSync(join(SRC_TAURI, name), "utf8"));
    } catch (err) {
      throw new Error(
        `${os}: ${name} is a config Tauri merges automatically into the shipped build, and it is not ` +
          `strict JSON (${err instanceof Error ? err.message : String(err)}). This guard carries no ` +
          `JSON5/TOML parser on purpose, so it cannot compute the merged config — refused rather ` +
          `than skipped (CPE-1900/CPE-1903).`,
      );
    }
  }
  return matches.map((name) => `${TAURI_PROJECT_DIR}/${name}`);
}

/**
 * THE model of "what config does the shipped app actually run on", for one build leg — repo-relative
 * paths in the exact order Tauri applies them. RFC 7396 merge is order-dependent, so this is a
 * sequence, never a set.
 *
 * **It has two halves, and they are covered by two different mechanisms. Saying which is which is the
 * point of this comment (CPE-1900).**
 *
 *  1. **DERIVED** — the `--config` overlays, read out of `release-sidecar.yml`'s own `args:` by
 *     `src/lib/tauriConfigChain.ts` (structural YAML parse, matrix expanded). Nothing here is copied;
 *     an overlay added to the workflow enters this chain on the same commit.
 *  2. **ENUMERATED** — the auto-merged `tauri.<platform>.conf.*`, which Tauri reads with NO flag at
 *     all. This half CANNOT be derived from the workflow: there is no flag in `args:` to read, by
 *     construction. It is instead a listing of `src-tauri/` classified by shape
 *     ({@link isAutoMergedPlatformConfigName}, whose platform-token list is itself read out of the
 *     Rust guard), which is the CPE-1932 answer when derivation is impossible.
 *
 * Order between the two halves is Tauri's: `read_from` merges the per-platform file onto the base
 * before the CLI applies a single `--config`, so it sits between them.
 *
 * **What NEITHER half covers — AT LEAST these, and the list is open.** A closed count of blind spots
 * is a claim like any other, and this repo has been wrong about one twice (CLAUDE.md, CPE-1933 rule 2,
 * round 9). Known today:
 *
 *   - A config supplied at build time by something that is not a committed file: a runner-side patch
 *     of `tauri.conf.json` (`release.yml` really does patch `bundle.windows` for code signing), or a
 *     `TAURI_CONFIG` environment variable. A property of the runner, not of the tree — no test reading
 *     this checkout can see either.
 *   - A build not driven by a `uses: tauri-apps/tauri-action` step with its overlays in `with.args`.
 *     `src/lib/tauriConfigChain.ts`'s header lists the four shapes measured to yield zero legs
 *     silently (a bare `run: npx tauri build --config …`, a composite action, a reusable-workflow
 *     call, tauri-action's `tauriScript:`) and says why the floors do not catch them.
 */
function configChainForLeg(leg: BuildLeg): string[] {
  return [BASE_CONFIG, ...autoMergedPlatformConfigs(leg.os), ...leg.overlays];
}

/** The FULL merged config for one build leg — the config that actually governs that shipped build. */
function mergedConfig(leg: BuildLeg): Record<string, unknown> {
  const configs = configChainForLeg(leg).map(loadConfig);
  const merged = configs.reduce((acc, cfg) => mergeJson(acc, cfg));
  if (!isPlainObject(merged)) {
    throw new Error(`${leg.where}: merged tauri config chain did not produce a JSON object`);
  }
  return merged;
}

/** Final `bundle.resources` value (array | object | undefined) after applying a leg's full chain. */
function mergedBundleResources(leg: BuildLeg): unknown {
  const merged = mergedConfig(leg);
  return isPlainObject(merged.bundle) ? merged.bundle.resources : undefined;
}

/**
 * Normalizes a merged `bundle.resources` value (array-form or object-form) down to the set of
 * DESTINATION paths that will actually exist inside the shipped bundle's resource root — what
 * `resource_dir()`-based runtime lookups resolve against. Array-form entries (Tauri's shorthand)
 * use the same string as both source and destination.
 */
function resourceDestinations(resources: unknown): Set<string> {
  if (Array.isArray(resources)) return new Set(resources.map(String));
  if (isPlainObject(resources)) return new Set(Object.values(resources).map(String));
  return new Set();
}

interface RequiredResource {
  /** Human label used in failure messages. */
  id: string;
  /** Where this resource is resolved/consumed at runtime — the one place to look when this fails,
   *  and the place a NEW resource dependency should be registered alongside this list. */
  consumer: string;
  /** Its expected destination path (relative to the bundle's resource root) for a given shipped OS. */
  dest: (os: ShipOS) => string;
}

/**
 * THE canonical list of runtime-required bundled resources (CPE-1271). Every resource the running
 * app resolves out of `resource_dir()` (or the frontend's `resolveResource`) must be registered
 * here, with a comment/consumer pointing at the code that resolves it — that's what keeps a future
 * resource dependency from silently missing the shipped bundle the way CPE-1258/1267/1270 did.
 */
const REQUIRED_RESOURCES: RequiredResource[] = [
  {
    id: "drag-out preview icon",
    consumer: "src/lib/dragOut.ts — DEFAULT_DRAG_ICON / resolveDragIcon()",
    dest: () => "icons/icon.png",
  },
  {
    id: "pdfium dynamic library",
    consumer: "crates/server/src/thumb_pdf.rs — resolve_bindings() (dylib next to a bundled native-dep dir)",
    dest: (os) => (os === "windows" ? "pdfium.dll" : os === "macos" ? "libpdfium.dylib" : "libpdfium.so"),
  },
  {
    id: "ffmpeg executable",
    consumer: "crates/server/src/thumb_video.rs — resolve_ffmpeg_bin()",
    dest: (os) => (os === "windows" ? "ffmpeg.exe" : "ffmpeg"),
  },
  {
    id: "ai-console sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_ai_console_bin() / sidecar_dirs() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/ai-console${os === "windows" ? ".exe" : ""}`,
  },
  {
    id: "agent-board sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_agent_board_bin() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/agent-board${os === "windows" ? ".exe" : ""}`,
  },
  {
    id: "repos sidecar binary",
    consumer: "src-tauri/src/lib.rs — resolve_sidecar_bin() (sidecars/<id>[.exe])",
    dest: (os) => `sidecars/repos${os === "windows" ? ".exe" : ""}`,
  },
];

describe("shipped sidecar bundle — every runtime-required resource is bundled (CPE-1271)", () => {
  // CPE-1900: the legs are DERIVED, so this refusal is what stops the derivation from quietly
  // returning fewer builds than ship. A guard that sweeps two OSes reports the same green as one that
  // sweeps three.
  it("the sidecar channel derives exactly one build leg per shipped OS", () => {
    expect(
      SIDECAR_LEGS.map((l) => `${l.os} (${l.runner})`).sort(),
      `${SIDECAR_WORKFLOW} did not derive one tauri-action build leg per shipped OS. Either the ` +
        `workflow's matrix changed, or src/lib/tauriConfigChain.ts stopped recognising its build ` +
        `step — both leave a shipped OS guarded by nothing while this file stays green.`,
    ).toEqual(["linux (ubuntu-latest)", "macos (macos-latest)", "windows (windows-latest)"]);
  });

  SIDECAR_LEGS.forEach((leg) => {
    it(`${leg.os}: merged bundle.resources includes every REQUIRED_RESOURCES entry`, () => {
      const destinations = resourceDestinations(mergedBundleResources(leg));
      const missing = REQUIRED_RESOURCES.map((r) => ({ ...r, dest: r.dest(leg.os) })).filter(
        (r) => !destinations.has(r.dest),
      );
      expect(
        missing,
        missing
          .map(
            (r) =>
              `${leg.os}: the shipped sidecar bundle would NOT include "${r.dest}" — ${r.id}, ` +
              `resolved at runtime by ${r.consumer}. Chain: ${configChainForLeg(leg).join(" -> ")}`,
          )
          .join("\n"),
      ).toEqual([]);
    });
  });

  // Documents + locks in the exact footgun this ticket guards against: an overlay OBJECT at a key
  // whose base value is an ARRAY replaces it outright rather than merging — this is Tauri's real
  // `--config` behavior, not a quirk of this test's merge helper (CPE-1270's root cause).
  it("mergeJson: an overlay OBJECT replaces (not unions with) a base ARRAY at the same key — the CPE-1270 footgun", () => {
    const base = { resources: ["a.png"] };
    const overlay = { resources: { "b.dll": "b.dll" } };
    expect(mergeJson(base, overlay)).toEqual({ resources: { "b.dll": "b.dll" } });
  });
});

/** Every ordering of `items`. Small by construction — the real chains are 4 entries, 24 orderings. */
function permutations<T>(items: T[]): T[][] {
  if (items.length <= 1) return [items];
  return items.flatMap((item, i) =>
    permutations([...items.slice(0, i), ...items.slice(i + 1)]).map((rest) => [item, ...rest]),
  );
}

/**
 * A key-order-INDEPENDENT rendering of a merged config, for asking "is this the same configuration?".
 *
 * A plain `JSON.stringify` is the wrong oracle here and the difference is not academic: `mergeJson`
 * spreads the base and then assigns the overlay's keys, so merging the same files in a different order
 * yields the same configuration with its keys in a different insertion order.
 *
 * Measured while writing this, **per leg, because the gap is not the same on each and quoting one
 * leg's number as the chain's would repeat the very error this function exists to fix** (of 24
 * orderings): `JSON.stringify` calls **21** "different" on all three legs, while the canonical
 * comparison finds **16 / 12 / 12** (windows / linux / macos) — so **5 / 9 / 9** of them differ in key
 * insertion order alone. Any of those raw 21s would have been quoted as evidence that ordering
 * matters more than it does.
 */
function canonicalJson(v: unknown): string {
  if (Array.isArray(v)) return `[${v.map(canonicalJson).join(",")}]`;
  if (isPlainObject(v)) {
    return `{${Object.keys(v)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`)
      .join(",")}}`;
  }
  return JSON.stringify(v) ?? "undefined";
}

/**
 * CPE-1900 — the chain is a SEQUENCE, and both halves of that (which files, in which order) have to be
 * guarded separately.
 *
 * The membership half is structural: `deriveBuildLegs` reads the workflow, so a `--config` overlay
 * added there enters the chain with nobody editing anything. The ORDER half is the one a derivation
 * can still get wrong on its own — a `.sort()`, a `reverse()`, a de-dupe, a `Set` round-trip — and
 * every one of those produces a chain with exactly the right membership and a DIFFERENT merged config
 * than the one that ships. A membership-only check calls that correct.
 *
 * **RED-PROOFED, both halves separately (2026-08-28, measured locally on this commit; every sabotage
 * reverted, `git status --porcelain` clean afterwards).**
 *
 *  - MEMBERSHIP, sabotage 1 — a fourth `--config` overlay added to `release-sidecar.yml`'s `args:`
 *    naming a file that is not committed. RED across the two test files: **13 failed / 49 passed of
 *    62**, each failure printing the full derived chain and naming
 *    `src-tauri/tauri.sidecar.injected.conf.json`.
 *  - MEMBERSHIP, sabotage 2 — the same fourth overlay, this time with the file present and carrying
 *    an attacker `plugins.updater.pubkey`/`.endpoints`. This is the one that matters: the overlay
 *    SHIPS and rewrites the root of trust. RED: **6 failed / 56 passed**, exactly the three shipped
 *    OSes x {pubkey, endpoints}.
 *    **And the measurement that says why this ticket existed:** with that same sabotage in place, the
 *    OLD hand-copied guard — `git show HEAD:src/lib/sidecarBundleResources.test.ts`, run unmodified —
 *    was **20 passed / 20, exit 0**. A green suite over a shipping build whose updater key belongs to
 *    someone else.
 *  - ORDERING, sabotaged separately from membership so the pair is not proved by one change. Making
 *    `configOverlaysFromArgs` return `[...out].sort()`: **3 failed / 38 passed** in this file, all
 *    three in the describe block below, naming both orders — and every membership assertion (the
 *    resource guard, the updater pin) stayed GREEN, which is the AC's point restated as a
 *    measurement. `[...out].reverse()`: identical, 3 failed / 38 passed here plus 3 in
 *    `tauriConfigChain.test.ts`.
 *    Note WHY membership stays green under a reversal, because it bounds what this leg proves: the
 *    base config is prepended by `configChainForLeg` and so stays first, and all 6 base-first
 *    orderings of today's chain agree (measured below). The ordering leg is therefore the only thing
 *    standing between a reordered chain and a silent pass.
 *
 * The last test below is the one that stops this pair from being decorative. An ordering guard over a
 * chain whose merge happens to be order-INVARIANT proves nothing, and it would read exactly like this
 * one. So it measures, per leg, how many of the chain's 24 orderings actually compute a different
 * merged config from the shipped one — and refuses if the answer is zero.
 */
describe("the config chain is DERIVED from the release workflows, and its ORDER is load-bearing (CPE-1900)", () => {
  BUILD_LEGS.forEach((leg) => {
    it(`${leg.where}: the derived overlay COUNT is the number of --config flags in the args`, () => {
      // Nothing counted overlays before (Reviewer, F6): dropping the FIRST overlay from the chain
      // left this file at 40/41, and the single red was the order-vacuity test — which caught it only
      // INCIDENTALLY, because a 3-file chain still has orderings that differ. Dropping the LAST one
      // red 9 tests, for unrelated reasons. A guard whose failure depends on which element went
      // missing is not counting; it is noticing side effects.
      //
      // RED-PROOFED (2026-08-28): with `configOverlaysFromArgs` returning `out.slice(1)`, this file
      // goes to 4 failed / 43 passed and three of those four are THIS assertion, one per sidecar leg,
      // each printing "the derived chain has 2 overlay(s) but the workflow passes 3 --config
      // flag(s)" with both lists. Before this test the same sabotage was 40/41. Reverted.
      //
      // A regex sweep of the flag occurrences, not another token walk — the same "different
      // mechanism" discipline as the ordering assertion below.
      const flagCount = (leg.args.match(/(?:^|\s)(?:--config|-c)(?:=|\s)/g) ?? []).length;
      expect(
        leg.overlays.length,
        `${leg.where}: the derived chain has ${leg.overlays.length} overlay(s) but the workflow ` +
          `passes ${flagCount} --config flag(s).\n  derived: ${leg.overlays.join(", ") || "(none)"}` +
          `\n  args:    ${leg.args}\n` +
          `An overlay the extractor drops is a file that SHIPS into the merged config with no ` +
          `assertion over it at all — the exact shape of the bug this ticket closed.`,
      ).toBe(flagCount);
    });

    it(`${leg.where}: the derived overlay order is the workflow's own order`, () => {
      // A DIFFERENT mechanism from the tokenizer that produced the list: substring position in the
      // leg's own resolved `args:` string. Re-running the token walk cannot notice a token walk that
      // sorts; a positional scan can.
      const positions = leg.overlays.map((p) => leg.args.indexOf(p));
      expect(
        positions.filter((p) => p < 0),
        `${leg.where}: a derived overlay path does not appear in the args string it was derived ` +
          `from -- the extractor is rewriting paths. args: ${leg.args}`,
      ).toEqual([]);
      const inArgsOrder = [...leg.overlays].sort(
        (a, b) => leg.args.indexOf(a) - leg.args.indexOf(b),
      );
      expect(
        leg.overlays,
        `${leg.where}: the derived --config chain is not in the order the workflow passes it.\n` +
          `  derived:  ${leg.overlays.join(" -> ")}\n` +
          `  workflow: ${inArgsOrder.join(" -> ")}\n` +
          `RFC 7396 merge is order-dependent, so a reordered chain computes a config that is NOT the ` +
          `one that ships, while every membership check stays green.`,
      ).toEqual(inArgsOrder);
    });

    it(`${leg.where}: every file in the merged chain exists and is strict JSON`, () => {
      const chain = configChainForLeg(leg);
      const broken = chain.filter((p) => {
        try {
          loadConfig(p);
          return false;
        } catch {
          return true;
        }
      });
      expect(
        broken,
        `${leg.where}: the release workflow passes config file(s) that cannot be read as JSON from ` +
          `this checkout: ${broken.join(", ")}. Full chain: ${chain.join(" -> ")}. Either the ` +
          `workflow names a file that is not committed, or a committed config is malformed -- both ` +
          `mean the build this guard describes is not the build that runs.`,
      ).toEqual([]);
      expect(chain[0], `${leg.where}: the base config must be first in the chain`).toBe(BASE_CONFIG);
      expect(
        new Set(chain).size,
        `${leg.where}: the chain repeats a file: ${chain.join(" -> ")}`,
      ).toBe(chain.length);
    });
  });

  // Does the order actually MATTER for the real chains, or is this file's ordering guard decorative?
  // Measured rather than assumed — CLAUDE.md, "do not name a backstop without checking it can fire".
  //
  // MEASURED 2026-08-28 on this commit, key-order-independent (see `canonicalJson`): of the 24
  // orderings of each sidecar leg's 4-file chain, **16 (windows) / 12 (linux) / 12 (macos)** compute a
  // genuinely different merged config than the shipped order. So the guard above can fire.
  //
  // The same run says something the assertion alone would not, and it is the more useful half. All
  // **6** base-first orderings agree with the shipped one, on every OS. Today's three overlays write
  // DISJOINT keys — `tauri.sidecar.conf.json` sets productName/identifier/createUpdaterArtifacts, the
  // per-OS one sets bundle.targets + an OBJECT-form bundle.resources, the pdfium one adds two more
  // keys to that same object — so they commute with each other, and the only position that is
  // load-bearing right now is the base's: its ARRAY-form `bundle.resources` either replaces the
  // overlays' object or is replaced by it, which is CPE-1270's footgun deciding the answer.
  //
  // That is exactly why the assertion above pins the FULL SEQUENCE and not just "base first". Overlay
  // commutativity is a property of today's three files, not of the mechanism; the first overlay that
  // writes a key another overlay also writes makes them non-commutative, silently, and nothing would
  // announce it. A guard scoped to what is load-bearing today would have to be widened by whoever
  // adds that file — which is the kind of "remember to update the guard" this ticket exists to delete.
  //
  // release.yml's legs have a 1-file chain -- 1 ordering, nothing to permute -- so they are excluded
  // rather than silently counted as "0 differing", which would read as a failure of this measurement
  // instead of a chain with no order to get wrong.
  it("reordering the real chain really does change the merged config (so the guard above can fire)", () => {
    const multiFile = BUILD_LEGS.filter((leg) => configChainForLeg(leg).length > 1);
    expect(
      multiFile.length,
      "no derived build leg has a multi-file config chain, so the ordering guard above is vacuous",
    ).toBeGreaterThanOrEqual(3);
    for (const leg of multiFile) {
      const chain = configChainForLeg(leg);
      const shipped = canonicalJson(mergedConfig(leg));
      const differing = permutations(chain).filter((order) => {
        const merged = order.map(loadConfig).reduce((acc, cfg) => mergeJson(acc, cfg));
        return canonicalJson(merged) !== shipped;
      }).length;
      expect(
        differing,
        `${leg.where}: NONE of the ${chain.length}! orderings of its config chain produces a ` +
          `different merged config, so this file's ordering assertions cannot fail and must not be ` +
          `read as coverage. Chain: ${chain.join(" -> ")}`,
      ).toBeGreaterThan(0);
    }
  });
});

// CPE-1873 (round 2 — independent Security Auditor, DEMONSTRATED not inferred): the updater
// root-of-trust pin in crates/updater-verify only ever reads the BASE `tauri.conf.json`. The build
// every install actually ships is release-sidecar.yml's: base config + the `--config` overlay chain
// derived from that workflow. Tauri's `--config` merge (RFC 7386 recursive object merge — the same
// mechanism the guard above proves overrides `bundle.resources`, CPE-1270) lets any overlay in that
// chain override `plugins.updater.pubkey` / `.endpoints` too. Proven: adding an updater override block
// to `tauri.sidecar.conf.json` alone (base file untouched) left crates/updater-verify's entire test
// suite green, including its base-config pin, while the actual shipped sidecar channel's root of trust
// was attacker-controlled. This guard checks the full merged `--config` overlay chain instead, so an
// override anywhere in the chain is caught regardless of which file introduced it. CPE-1900 made that
// chain derived rather than copied, and widened this from the three sidecar OSes to every derived
// build leg of every release channel. The `--config` half alone still does NOT cover a config Tauri
// merges automatically — `configChainForLeg` folds that enumerated half in, and the separate describe
// block below (CPE-1873 finding 6 / CPE-1903) refuses it outright.
//
// CPE-1987 — these two used to be LITERALS here, under a comment reading "Keep these two literals in
// lockstep with crates/updater-verify/src/pinned_pubkey.rs". That is CPE-1933's shape on the root of
// trust: a provenance claim with nothing checking it, and worse than no comment because the green
// tests around it read as vouching for it. They are now READ out of that file at run time, comments
// stripped first (`rustSource.ts`, the same machinery three hundred lines below reads
// TAURI_PLATFORM_TOKENS with) so a commented-out old value cannot be mistaken for the live one.
//
// **What the copy was actually costing, stated precisely, because "drift" is the wrong word for it.**
// A stale literal here could not drift SILENTLY — it is compared against the real merged config, so a
// stale copy simply reds. What the copy bought an attacker was the opposite: this file's pin was
// independent of the Rust one, so writing an attacker key into an OVERLAY *and* into this literal hid
// it from every guard that could see it (the Rust pin only ever reads the BASE config, untouched in
// that scenario). Deriving closes that: the value asserted against every merged leg is now the Rust
// const itself.
//
// **The size of that attack, corrected to what actually reproduces (PR #1108 review, CLAIM-1).** The
// first write-up here said "two edited files, six shipped legs, nothing red". Measured on the base
// commit, the TWO-file version (overlay + this literal) is **3 failed / 44 passed**: `release.yml`'s
// plain channel takes no overlay, so its three legs keep the real merged pubkey while the literal has
// moved, and they say so. Only the three SIDECAR legs are compromised, and it is NOT silent. The
// genuinely all-green shape needs a THIRD file — the overlay must also be added to `release.yml`'s
// matrix `args:` — and that one does reproduce in full: whole suite green, attacker root of trust on
// all six legs. The fix holds: at this file's head the two remaining files of that attack red
// **6 failed / 42 passed**, every leg.
//
// **And what deriving gives up, so the next reader does not have to re-derive it.** The deleted
// literal was also a THIRD copy, and a rotation that edits `tauri.conf.json` + `pinned_pubkey.rs`
// together used to red HERE on the stale third copy. It no longer does — that rotation is now a
// two-file self-consistent diff, which is exactly the scope `pinned_pubkey.rs`'s "What none of this
// proves" section already declares out of bounds (nothing here consults a value from outside the
// tagged commit). The trade is deliberate: it turns a shape that was green-when-compromised into one
// that is red-when-compromised, at the cost of a review-surface bullet that was never a guarantee.
const UPDATER_PIN_RS = stripRustComments(
  readFileSync(join(process.cwd(), "crates", "updater-verify", "src", "pinned_pubkey.rs"), "utf8"),
);
const EXPECTED_UPDATER_PUBKEY = rustStrConstAfter(
  UPDATER_PIN_RS,
  "pub const EXPECTED_TAURI_UPDATER_PUBKEY",
);
const EXPECTED_UPDATER_ENDPOINTS = rustStrSliceAfter(
  UPDATER_PIN_RS,
  "pub const EXPECTED_TAURI_UPDATER_ENDPOINTS",
);

function mergedUpdaterConfig(leg: BuildLeg): { pubkey: unknown; endpoints: unknown } {
  const merged = mergedConfig(leg);
  const plugins = isPlainObject(merged.plugins) ? merged.plugins : undefined;
  const updater = plugins && isPlainObject(plugins.updater) ? plugins.updater : undefined;
  return { pubkey: updater?.pubkey, endpoints: updater?.endpoints };
}

// CPE-1900 — CPE-1873's injection re-run after the restructure, so "did not regress" is a measurement
// and not a hope. An attacker `plugins.updater.pubkey`/`.endpoints` was written into EACH file of the
// shipped chain in turn (2026-08-28, locally, each file restored with `git checkout --` afterwards;
// `git status --porcelain` clean at the end). Every one reds, and reds exactly the shipped OSes that
// file actually governs — `sidecarBundleResources.test.ts` alone, of 41 tests:
//
//   tauri.conf.json                        12 failed / 29 passed   all 6 legs (both channels, 3 OSes)
//   tauri.sidecar.conf.json                 6 failed / 35 passed   3 sidecar legs (all shipped OSes)
//   tauri.sidecar.unix.conf.json            4 failed / 37 passed   linux + macos
//   tauri.sidecar.windows.conf.json         2 failed / 39 passed   windows
//   tauri.sidecar.pdfium.windows.conf.json  2 failed / 39 passed   windows
//   tauri.sidecar.pdfium.linux.conf.json    2 failed / 39 passed   linux
//   tauri.sidecar.pdfium.macos.conf.json    2 failed / 39 passed   macos
//
// Read the right-hand column as the point rather than the counts: a per-OS overlay reds the OSes it
// ships to and no others, which is what a guard describing real builds should do. "All three OSes red
// for every file" would mean the legs were not really distinct.
describe("shipped bundles — updater root of trust survives the FULL merge, every channel (CPE-1873)", () => {
  // CPE-1900 widened this from the three hand-listed sidecar OSes to EVERY derived build leg, which
  // today is 6: release-sidecar.yml's 3 plus release.yml's 3. The plain channel passes no `--config`
  // overlay, so its chain is the base config plus whatever Tauri auto-merges — previously asserted
  // only by the Rust base-config pin, and not at all for the auto-merged half.
  it("the derived leg set covers both release channels", () => {
    expect(
      [...new Set(BUILD_LEGS.map((l) => l.workflow))].sort(),
      `the tauri-action build discovery no longer sees both release channels. A channel that ` +
        `disappears from this list ships with its merged updater config asserted by nothing.`,
    ).toEqual([".github/workflows/release-sidecar.yml", ".github/workflows/release.yml"]);
  });

  // CPE-1987. The pin every leg below compares against is DERIVED from `pinned_pubkey.rs`, so this
  // asserts the derivation landed on a value rather than on nothing. A vacuous derivation would not
  // pass silently — an empty pubkey reds all 6 pubkey legs — but it would red with a message about the
  // shipped config when the fault is in the reader, which is the wrong place to send the next person.
  //
  // It deliberately does NOT restate the key: a second copy of the value here is the exact thing this
  // ticket deleted. It asserts the SHAPE — the base64 of "untrusted comment: minisign public key: ",
  // minisign's own fixed preamble, not a secret and not per-key — plus a non-empty endpoint list of
  // absolute https URLs.
  it("the pinned updater values were really read out of pinned_pubkey.rs", () => {
    expect(
      EXPECTED_UPDATER_PUBKEY.startsWith("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6"),
      "the value derived from crates/updater-verify/src/pinned_pubkey.rs's " +
        "EXPECTED_TAURI_UPDATER_PUBKEY is not a minisign public key. Every assertion below compares " +
        "the shipped config against it, so a reader that came back with the wrong thing would be " +
        "pinning the wrong thing -- and would say so in the language of a config override.",
    ).toBe(true);
    expect(EXPECTED_UPDATER_ENDPOINTS.length).toBeGreaterThanOrEqual(1);
    expect(
      EXPECTED_UPDATER_ENDPOINTS.filter((e) => !e.startsWith("https://")),
      "a pinned updater endpoint is not an absolute https URL",
    ).toEqual([]);
  });

  BUILD_LEGS.forEach((leg) => {
    it(`${leg.where}: merged plugins.updater.pubkey equals the pinned value`, () => {
      const { pubkey } = mergedUpdaterConfig(leg);
      expect(
        pubkey,
        `${leg.where}: the FINAL merged config's plugins.updater.pubkey does not match the pinned ` +
          `value -- some file in the chain (${configChainForLeg(leg).join(" -> ")}) overrides it. ` +
          `This IS a shipped channel's actual root of trust; an override here is a live compromise, ` +
          `not a lint nit. See crates/updater-verify/src/pinned_pubkey.rs (CPE-1873) for the ` +
          `rotation procedure if this is deliberate -- update the pin there too, in the same commit.`,
      ).toEqual(EXPECTED_UPDATER_PUBKEY);
    });

    it(`${leg.where}: merged plugins.updater.endpoints equals the pinned value`, () => {
      const { endpoints } = mergedUpdaterConfig(leg);
      expect(
        endpoints,
        `${leg.where}: the FINAL merged config's plugins.updater.endpoints does not match the pinned ` +
          `value -- some file in the chain (${configChainForLeg(leg).join(" -> ")}) overrides it. A ` +
          `changed endpoint can silently downgrade users to an older, genuinely-signed but vulnerable ` +
          `build forever, even with the pubkey pin intact. See ` +
          `crates/updater-verify/src/pinned_pubkey.rs (CPE-1873).`,
      ).toEqual(EXPECTED_UPDATER_ENDPOINTS);
    });
  });
});

// CPE-1903 (supersedes CPE-1873 finding 6): Tauri merges a per-platform config file AUTOMATICALLY,
// with no `--config` flag and no workflow involvement — `tauri-utils::config::parse::read_from` reads
// `tauri.conf.json` and then merges a per-platform file from the same directory via RFC 7396, on every
// build for that platform. Whatever that file sets wins, `plugins.updater.pubkey`/`.endpoints`
// included, and it appears in no `--config` chain, so no derivation from a workflow's `args:` can
// see it — which is why this half is ENUMERATED from the directory instead (see `configChainForLeg`).
//
// CPE-1873 round 3 closed this by hardcoding three `.json` filenames. Tauri's real surface is FIFTEEN:
// `ConfigFormat::into_platform_file_name` crosses three formats (`tauri.<t>.conf.json`,
// `tauri.<t>.conf.json5`, `Tauri.<t>.toml`) with five `Target` variants (macos/windows/linux/
// android/ios), `does_supported_file_name_exist` returns true if ANY enabled format's platform file
// exists, and `do_parse` falls through json -> json5 -> toml. Both non-`.json` formats were
// demonstrated ingesting an attacker config through this repo's own installed `@tauri-apps/cli` while
// every guard reported green.
//
// So this does not name files: it lists the directory and classifies what is actually there.
// `readdirSync` reports the on-disk spelling, so an ASCII-lowercased shape match behaves identically
// on a case-insensitive NTFS build runner and on the byte-exact `ubuntu-latest` host where
// `release-sidecar.yml`'s `verify-updater-pin` job runs this file. Round 3's `existsSync(join(dir,
// name))` was a LOOKUP, and therefore silently blind on exactly that host while Windows merged the
// file anyway.
//
// This guard exists TWICE — here and in `crates/updater-verify/src/platform_config_guard.rs`, which
// additionally runs inside `verify-release-artifacts` so it reaches `release.yml`'s tag path (no
// `#[test]` and no vitest can). The Rust module doc carries the full rationale.
//
// CPE-1950: the second copy used to be held together by the sentence "Keep the two derivations in
// lockstep" — a provenance claim, untested by construction, on the app's updater root of trust. It is
// now derived instead: the token list below is read out of the Rust const at run time, and both
// implementations execute the shared case file `platformConfigGuard.cases.json`. See the "DERIVED"
// describe block at the end of this file for what those two legs do and do not cover.

/**
 * The second dot-segment of every name `ConfigFormat::into_platform_file_name` can produce.
 *
 * Declared here so this file stays readable on its own, and pinned to the Rust const it duplicates by
 * the derivation at the end of this file — a token added on one side reds on the other, which is the
 * failure the old "keep in lockstep" comment could not produce.
 */
const TAURI_PLATFORM_TOKENS: readonly string[] = ["macos", "windows", "linux", "android", "ios"];

/** ASCII-only case fold, so this matches the Rust side's `to_ascii_lowercase` exactly. */
function asciiLower(s: string): string {
  return s.replace(/[A-Z]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 32));
}

/**
 * Does `fileName` name a config file Tauri merges automatically? Matched by shape — `tauri` `.`
 * <platform token> `.` <at least one more segment> — never by a particular spelling of the tail, so a
 * format Tauri adds tomorrow is covered without anyone editing a list. Deliberately NOT matched:
 * `tauri.conf.json` / `Tauri.toml` (the base config, pinned by value above) and `tauri.sidecar.*`
 * (the explicit `--config` overlays, covered by the derived chain) — their second segment is not a
 * platform token, which is exactly the property Tauri itself keys on.
 */
function isAutoMergedPlatformConfigName(fileName: string): boolean {
  const segments = asciiLower(fileName).split(".");
  return (
    segments.length >= 3 && segments[0] === "tauri" && TAURI_PLATFORM_TOKENS.includes(segments[1])
  );
}

/**
 * Why this per-platform config file must be refused, or `null` if it is clean.
 *
 * Refused for CARRYING a `plugins.updater` key — never for merely existing, never by comparing its
 * value to the pin. A per-platform file setting only `plugins.cli` stays allowed; a
 * `{"plugins":{"updater":null}}` is refused, because under RFC 7396 a `null` DELETES the base
 * config's updater block. Both were deliberate CPE-1873 round-3 choices, preserved verbatim.
 *
 * Strict JSON is inspected structurally. Anything else (JSON5, TOML, or a `.json`-named file holding
 * JSON5 — which Tauri's `do_parse` accepts) gets a conservative textual scan, because no JSON5/TOML
 * parser is carried here on purpose and a guard on the root of trust must not silently pass a format
 * it cannot read.
 */
function platformConfigUpdaterRefusal(text: string): string | null {
  let parsed: unknown;
  let parsedAsStrictJson = true;
  try {
    parsed = JSON.parse(text);
  } catch {
    parsedAsStrictJson = false;
  }
  if (parsedAsStrictJson) {
    // CPE-1903 finding 9: apply the refusal at EVERY level RFC 7396 can delete from, not just at
    // `plugins.updater`. `{"plugins":null}` deletes the whole plugins block -- updater included -- and
    // a non-object root replaces the entire config. Deleting the parent deletes the child.
    if (!isPlainObject(parsed)) {
      return (
        "exists and its top-level value is not a JSON object. As an RFC 7396 merge patch that REPLACES " +
        "the entire base config -- `plugins.updater` and all -- rather than adding to it"
      );
    }
    if ("plugins" in parsed && !isPlainObject(parsed.plugins)) {
      return (
        "exists and sets `plugins` to something other than an object. Under RFC 7396 that REPLACES the " +
        "base config's whole `plugins` block -- and a `null` there DELETES it -- so the shipped app can " +
        "end up with no updater configuration at all: update suppression, which freezes every install " +
        "on the build it already has and stops security fixes reaching it (CPE-1903 finding 9)"
      );
    }
    return isPlainObject(parsed.plugins) && "updater" in parsed.plugins
      ? "exists and sets a `plugins.updater` key. Tauri merges this file into the build automatically " +
          "via RFC 7396, so that key decides the shipped updater's root of trust -- and a `null` there " +
          "DELETES the pinned block just as effectively as a value replaces it"
      : null;
  }
  if (/updater/i.test(text)) {
    return (
      "exists, is not strict JSON (so this guard cannot parse it structurally -- no JSON5/TOML parser " +
      "is carried here on purpose), and mentions `updater`. Refused rather than guessed at"
    );
  }
  if (text.includes("\\")) {
    return (
      "exists, is not strict JSON (so this guard cannot parse it structurally), and contains backslash " +
      "escape sequences that could spell a `plugins.updater` key without the literal token ever " +
      "appearing. Refused rather than guessed at"
    );
  }
  return null;
}

/** Every auto-merged per-platform config in `src-tauri/` that sets `plugins.updater`. */
function scanForPlatformConfigUpdaterOverrides(): { fileName: string; reason: string }[] {
  // CPE-1903 finding 8: there is deliberately NO `e.isFile()` filter here. Node's `Dirent.isFile()`
  // is lstat-shaped -- it does not traverse -- so a SYMLINK named `tauri.linux.conf.json` pointing at
  // an innocuous committed file was dropped before it was ever read, while Tauri's own `read_platform`
  // (`exists()` + `read_to_string`, both of which FOLLOW links) merged the payload. Demonstrated with
  // the real CLI. Git stores such a link as mode 120000, so it is a real symlink on the ubuntu and
  // macOS runners -- including the `ubuntu-latest` host that runs this file in `verify-updater-pin`.
  // `readFileSync` follows links exactly as Tauri does, and a directory or unreadable entry now
  // becomes a refusal through the fail-closed catch below instead of a silent skip.
  return readdirSync(SRC_TAURI, { withFileTypes: true })
    .filter((e) => isAutoMergedPlatformConfigName(e.name))
    .flatMap((e) => {
      let reason: string | null;
      try {
        reason = platformConfigUpdaterRefusal(readFileSync(join(SRC_TAURI, e.name), "utf8"));
      } catch (err) {
        reason =
          `exists but could not be read as text (${err instanceof Error ? err.message : String(err)}), ` +
          `so this guard cannot rule out a plugins.updater key in a file Tauri merges automatically. ` +
          `Refused rather than skipped`;
      }
      return reason === null ? [] : [{ fileName: e.name, reason }];
    })
    .sort((a, b) => a.fileName.localeCompare(b.fileName));
}

describe("shipped bundle — no auto-merged per-platform Tauri config overrides the updater pin (CPE-1903)", () => {
  it("src-tauri/ holds no per-platform config file that sets plugins.updater, in any format or casing", () => {
    const hits = scanForPlatformConfigUpdaterOverrides();
    expect(
      hits,
      `SECURITY (CPE-1903): a per-platform Tauri config file in src-tauri/ can override the updater's ` +
        `root of trust:\n` +
        hits.map((h) => `  - ${h.fileName} ${h.reason}`).join("\n") +
        `\n\nTauri picks these up with NO --config flag: read_from() merges ` +
        `tauri.<platform>.conf.json / .json5 / Tauri.<platform>.toml next to tauri.conf.json via RFC ` +
        `7396 on every build for that platform, so such a file is invisible to the base-config pin, ` +
        `invisible to any derivation from a workflow's --config args, and ships on the plain AND ` +
        `sidecar channels alike. ` +
        `If deliberate: it must not set plugins.updater at all -- route any real key/endpoint change ` +
        `through tauri.conf.json (or a --config overlay the release workflow already passes) and update ` +
        `crates/updater-verify/src/pinned_pubkey.rs in the same commit. If not deliberate: STOP, this ` +
        `commit's builds are not trustworthy.`,
    ).toEqual([]);
  });

  // The derivation itself, exercised directly. These are the cases round 3's three-filename list let
  // through, asserted here so the next variant has to beat the SHAPE, not a spelling.
  it("matches every filename Tauri can auto-merge — 3 formats x 5 targets, any casing", () => {
    const everyName = TAURI_PLATFORM_TOKENS.flatMap((t) => [
      `tauri.${t}.conf.json`,
      `tauri.${t}.conf.json5`,
      `Tauri.${t}.toml`,
    ]);
    expect(everyName).toHaveLength(15);
    for (const name of everyName) expect(isAutoMergedPlatformConfigName(name), name).toBe(true);
    for (const name of [
      "Tauri.Windows.Conf.json",
      "TAURI.WINDOWS.CONF.JSON",
      "tauri.WINDOWS.conf.JSON5",
      "Tauri.LINUX.Toml",
      "tauri.windows.conf.yaml", // a format Tauri has not shipped yet
    ]) {
      expect(isAutoMergedPlatformConfigName(name), name).toBe(true);
    }
  });

  it("does not match the base config or the explicit --config overlays", () => {
    for (const name of [
      "tauri.conf.json",
      "tauri.conf.json5",
      "Tauri.toml",
      "tauri.sidecar.conf.json",
      "tauri.sidecar.windows.conf.json",
      "tauri.sidecar.pdfium.macos.conf.json",
      "Cargo.toml",
      "tauri.windows",
      "notauri.windows.conf.json",
    ]) {
      expect(isAutoMergedPlatformConfigName(name), name).toBe(false);
    }
  });

  it("refuses a plugins.updater key in every format, including an RFC 7396 delete", () => {
    expect(platformConfigUpdaterRefusal(`{"plugins":{"updater":{"pubkey":"x"}}}`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`{"plugins":{"updater":null}}`)).not.toBeNull();
    expect(
      platformConfigUpdaterRefusal(`// c\n{ plugins: { updater: { pubkey: 'x' } } }`),
    ).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`[plugins.updater]\npubkey = "x"\n`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`plugins.updater.pubkey = "x"\n`)).not.toBeNull();
    expect(platformConfigUpdaterRefusal(`{ plugins: { "\\u0075pdater": {} } }`)).not.toBeNull();
  });

  // CPE-1903 finding 9: `{"plugins":null}` is 17 bytes and deletes the base config's whole plugins
  // block under RFC 7396 — updater included — so the shipped app receives no updates at all. The first
  // version of this guard only looked at `plugins.updater` and let it through. Deleting the parent
  // deletes the child, so the refusal applies at every level a null can reach.
  it("refuses a null/non-object plugins block, and a non-object root", () => {
    for (const body of [
      `{"plugins":null}`,
      `{"plugins":[]}`,
      `{"plugins":"gone"}`,
      `{"plugins":42}`,
      `null`,
      `[]`,
      `"gone"`,
      `0`,
    ]) {
      expect(platformConfigUpdaterRefusal(body), body).not.toBeNull();
    }
  });

  it("still allows a per-platform file that does not touch the updater", () => {
    expect(platformConfigUpdaterRefusal(`{"plugins":{"cli":{"args":[]}}}`)).toBeNull();
    expect(platformConfigUpdaterRefusal(`{"bundle":{"targets":["msi"]}}`)).toBeNull();
    expect(platformConfigUpdaterRefusal(`[plugins.cli]\ndescription = "hi"\n`)).toBeNull();
  });
});

/**
 * CPE-1950 — "keep the two derivations in lockstep", derived instead of asked for.
 *
 * This guard is duplicated across the Rust/TS boundary on purpose (each side reaches a CI path the
 * other cannot), and the duplication was held together by a comment. That is the highest-blast-radius
 * provenance claim in this repo: it guards the **updater root of trust**, so a platform token added on
 * one side leaves a config-injection path green on the other, and nothing reds.
 *
 * Two legs, which fail in different ways on purpose:
 *
 *  1. **The token list is READ out of the Rust const** (`TAURI_PLATFORM_TOKENS` in
 *     `platform_config_guard.rs`), comments stripped first via `rustSource.ts` so a commented-out old
 *     list cannot be mistaken for the live one. Nobody has to remember to write a case: adding a token
 *     to Tauri's `Target` enum on the Rust side alone reds here immediately.
 *  2. **Both implementations execute the same case file**, `platformConfigGuard.cases.json` (23 name
 *     cases + 20 refusal cases) — this file below, and `platform_config_guard.rs`'s
 *     `both_implementations_agree_on_every_shared_case`.
 *     That covers the behaviour the const cannot express: the `>= 3` segment rule, the ASCII-only case
 *     fold, and the RFC-7396 refusal set (null-deletion at `plugins.updater`, at `plugins`, and at the
 *     root).
 *
 * **What leg 2 cannot catch: shared blindness.** A shared oracle proves the two sides agree; it cannot
 * prove either is right. A shape neither implementation considered is simply absent from the case
 * file, both answer it the same wrong way, and this passes green. That is measured, not theoretical —
 * on PR #1060 a `<<` inside a quoted string opened a phantom heredoc in *both* the TS and the Rust
 * shell scanners and their shared case file agreed with itself. Leg 1 is the part that does not depend
 * on anyone having thought of the case; the two sides' own independent tests are the rest of the
 * cover. If you touch either implementation, add the case to the SHARED file, never to one side.
 *
 * **Red-proofed, not assumed.** Leg 1: appending `"visionos"` to the Rust const fails the first test
 * here with the SECURITY message (`expected [ 'macos', 'windows', 'linux', …(2) ] to deeply equal
 * […(3)]`). Leg 2: making the Rust matcher demand one extra segment (so it stops matching
 * `Tauri.<t>.toml`) fails `both_implementations_agree_on_every_shared_case` on the Rust side, on
 * shared case "TOML tail, ConfigFormat::Toml's exact spelling". Both reverted.
 */
describe("the platform-config guard is DERIVED from its Rust twin, not claimed to match it (CPE-1950)", () => {
  const RUST_GUARD = stripRustComments(
    readFileSync(
      join(process.cwd(), "crates", "updater-verify", "src", "platform_config_guard.rs"),
      "utf8",
    ),
  );

  it("the platform token list is read out of platform_config_guard.rs, not copied", () => {
    const fromRust = rustStrSliceAfter(RUST_GUARD, "pub const TAURI_PLATFORM_TOKENS");
    expect(
      [...TAURI_PLATFORM_TOKENS],
      "SECURITY (CPE-1903/CPE-1950): this file's platform token list no longer matches " +
        "crates/updater-verify/src/platform_config_guard.rs's TAURI_PLATFORM_TOKENS. A token present " +
        "on one side only means a `tauri.<token>.conf.json` that Tauri merges into the build — and " +
        "which can rewrite plugins.updater.pubkey/endpoints — is refused by one guard and invisible " +
        "to the other. Update BOTH, in the same commit.",
    ).toEqual(fromRust);
  });

  it("the case file is not empty or truncated (an empty oracle agrees with everything)", () => {
    // CPE-1932: enumerate, don't recall. A vacuous fixture is how a shared oracle passes while
    // proving nothing, so both sides assert a floor on its size.
    expect(guardCases.names.length).toBeGreaterThanOrEqual(20);
    expect(guardCases.refusals.length).toBeGreaterThanOrEqual(18);
  });

  it("isAutoMergedPlatformConfigName answers every shared name case the way the oracle says", () => {
    for (const c of guardCases.names) {
      expect(isAutoMergedPlatformConfigName(c.fileName), `${c.name} (${c.fileName})`).toBe(
        c.autoMerged,
      );
    }
  });

  it("platformConfigUpdaterRefusal answers every shared refusal case the way the oracle says", () => {
    for (const c of guardCases.refusals) {
      // The oracle pins the DECISION, never the message text: the two sides word their refusals
      // differently on purpose (each names its own remediation path), and pinning prose would make
      // this fixture red on a copy edit while staying silent on a semantic divergence.
      expect(platformConfigUpdaterRefusal(c.text) !== null, c.name).toBe(c.refused);
    }
  });
});
