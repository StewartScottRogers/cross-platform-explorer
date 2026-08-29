// CPE-1900 — the `--config` overlay chain the shipped app is built with, DERIVED from the release
// workflows instead of hand-copied into a test.
//
// ## What was wrong
//
// `sidecarBundleResources.test.ts` guarded the shipped merged configuration — `bundle.resources`
// (CPE-1270/1271) and, since CPE-1873, the updater `pubkey`/`endpoints` root of trust — by walking a
// `CONFIG_CHAIN` literal listing the `--config` overlays `release-sidecar.yml` layers on
// `tauri.conf.json`. `grep -rn CONFIG_CHAIN` found the literal and a comment asking the reader to
// "keep this in lockstep with the workflow". Nothing read the workflow. Adding a fifth overlay would
// have shipped a file that is merged into the config every install runs on, while the guard stayed
// green and narrower than it looked. A convention is not a mechanism (CLAUDE.md, CPE-1933).
//
// ## What this derives, and how
//
// Structurally, never textually. `parseWorkflowFile` (the app's own bounded-subset YAML parser, via
// `workflowShellSources.ts`) hands back the parsed document, so every `#` comment in the file — and
// `release-sidecar.yml` is more comment than YAML — is gone before anything here looks at a string.
// That is CPE-1933 rule 2 ("anchor on code, never on prose") satisfied by construction rather than by
// a hand-rolled stripper: the only strings this module scans are the VALUES of `with.args` and
// `runs-on`, which cannot contain a YAML comment. The parser refuses anything outside its subset
// rather than returning a partial document, so a workflow that outgrows it fails loudly here instead
// of silently deriving an empty chain.
//
// Enumerated, never recalled (CPE-1932): every workflow in `.github/workflows/` is walked, and every
// step that `uses:` `tauri-apps/tauri-action` is a build leg. Each leg's job matrix is expanded and
// `${{ matrix.* }}` resolved, because the real overlay filenames only exist after that substitution
// (`tauri.sidecar.${{ matrix.overlay }}.conf.json`). Floors below refuse a near-empty discovery, which
// is the half of that rule that keeps getting left off.
//
// ## What this does NOT cover — AT LEAST these, and the list is open
//
// **1. The derivation keys on ONE build shape: a step whose `uses:` is `tauri-apps/tauri-action`,
// passing its overlays through `with.args`.** A build driven any other way is invisible to it, and
// the floors do not help — they catch SHRINKAGE, so an extra channel leaves the count at 6 and
// reports clean. Measured shapes that yield ZERO legs, silently: a `run: npx tauri build --config
// src-tauri/tauri.evil.conf.json` step, a local composite action wrapping the build, a
// reusable-workflow call (`uses: ./.github/workflows/x.yml`), and tauri-action's own `tauriScript:`
// input naming a wrapper script that adds flags this module never sees. Those are the ones that were
// tried; there will be others.
//
// The honest good news, so this reads as a bounded gap rather than an open door: adding a third
// channel that DOES use tauri-action reds immediately — `sidecarBundleResources.test.ts`'s "the
// derived leg set covers both release channels" pins the workflow list with `toEqual`, so a new
// tauri-action workflow fails it by name and has to be dealt with deliberately. The gap is the OTHER
// build shapes, not a third channel as such.
//
// **2. Tauri merges `tauri.<platform>.conf.json` with no `--config` flag at all** — via
// `tauri-utils::config::parse::read_from`, on every build for that platform. That file class cannot
// appear in any workflow's `args:`, so a derivation from the workflow is a derivation of ONE HALF of
// the config the shipped app runs on. The other half is enumerated by listing `src-tauri/` and
// classifying by shape; see `sidecarBundleResources.test.ts`, which composes the two and states which
// is which at the point where it builds the chain.
//
// Do not read this module as "the shipped config", and do not read the two items above as a closed
// count of what it misses — CLAUDE.md's rule is *"at least these"*, because the last three times
// someone wrote down a remainder as a number, the number was wrong within the day.

import { discoverWorkflows, parseWorkflowFile } from "./workflowShellSources";

/** The three operating systems this repo ships installers for. */
export type ShipOS = "windows" | "linux" | "macos";

/** The action a release workflow uses to build + publish. Matched on the owner/repo, not the ref. */
const TAURI_ACTION = "tauri-apps/tauri-action";

/**
 * The Tauri project directory, holding `tauri.conf.json` and every `--config` overlay.
 *
 * **This one is a CONSTANT, not a derivation, and that is deliberate** — say it rather than let the
 * module's shape imply otherwise. tauri-action locates the project itself (its `projectPath` input is
 * unset in both workflows), so the workflow YAML genuinely does not state where the base config
 * lives; there is nothing here to read. What keeps it honest is
 * {@link assertOverlaysLiveInProjectDir}: every overlay path derived from a workflow must sit in this
 * directory, so moving the project without updating this line is a loud failure rather than a guard
 * quietly pointed at the wrong tree.
 */
export const TAURI_PROJECT_DIR = "src-tauri";

/** The base config Tauri loads before any overlay. Repo-relative. */
export const BASE_CONFIG = `${TAURI_PROJECT_DIR}/tauri.conf.json`;

/**
 * Discovery floors (CPE-1932 / CPE-1969's `MIN_EXPECTED_WORKFLOWS`, same reasoning). A guard that
 * enumerates nothing reports clean, so an empty or shrunken discovery must be RED, never a vacuous
 * pass over zero legs.
 *
 * Today: 2 workflows (`release.yml`, `release-sidecar.yml`) x 3 matrix legs each = 6. Set AT today's
 * count, not below it: release channels are heavyweight and never bulk-deleted, so any number under
 * this means discovery broke (a renamed action, a matrix shape the expander does not understand, the
 * wrong working directory), not that the repo genuinely stopped shipping. If a channel is really
 * retired, lower these in the same diff and say which in the Work Log.
 */
export const MIN_EXPECTED_BUILD_WORKFLOWS = 2;
export const MIN_EXPECTED_BUILD_LEGS = 6;

/** One `tauri-action` build, with its matrix leg resolved: a concrete OS and a concrete overlay chain. */
export interface BuildLeg {
  /** Repo-relative workflow path. */
  workflow: string;
  /** Job id within that workflow. */
  job: string;
  /** Step name (or `(unnamed)`). */
  step: string;
  /** The resolved `runs-on` label, e.g. `windows-latest`. */
  runner: string;
  /** Which shipped OS that runner builds for. */
  os: ShipOS;
  /**
   * The repo-relative `--config` overlay paths, **in the order the workflow passes them** — which is
   * the order Tauri applies them, and RFC 7396 merge is order-dependent, so this list is a sequence
   * and never a set. Empty for a channel that ships the base config unmodified.
   */
  overlays: string[];
  /**
   * The full `args:` string this leg receives, with `${{ matrix.* }}` resolved — the exact string
   * {@link configOverlaysFromArgs} tokenized to produce {@link overlays}.
   *
   * Carried so a consumer can re-check the derived ORDER by a different mechanism than the tokenizer
   * that produced it (substring position rather than a token walk). A chain that came back sorted,
   * reversed or deduped is a different merged config than the one that ships, and re-running the code
   * that made the mistake cannot notice.
   */
  args: string;
  /** Human label for failure messages. */
  where: string;
}

/** A `${{ matrix.key }}` reference, tolerant of the whitespace GitHub allows inside the braces. */
const MATRIX_REF = /\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}/g;

/** Any remaining expression, once matrix refs are resolved. */
const ANY_EXPRESSION = /\$\{\{/;

/**
 * Resolves `${{ matrix.* }}` in one workflow string against one matrix leg.
 *
 * Throws on a reference the leg does not define, and on any OTHER `${{ … }}` expression left behind:
 * an unresolved expression means this module does not know what the build actually receives, and
 * guessing is how a guard ends up describing a build nobody runs. Refuse rather than guess.
 */
export function resolveMatrixRefs(text: string, leg: Record<string, string>, where: string): string {
  const resolved = text.replace(MATRIX_REF, (_whole, key: string) => {
    if (!(key in leg)) {
      throw new Error(
        `${where}: references \${{ matrix.${key} }}, which this matrix leg does not define ` +
          `(leg keys: ${Object.keys(leg).join(", ") || "(none)"}). The derived config chain would be ` +
          `wrong, so it is refused instead.`,
      );
    }
    return leg[key];
  });
  if (ANY_EXPRESSION.test(resolved)) {
    throw new Error(
      `${where}: still holds an unresolved \${{ … }} expression after matrix substitution: ` +
        `"${resolved}". Only matrix references are resolvable from the workflow file alone, so this ` +
        `guard cannot know what the build receives. Extend src/lib/tauriConfigChain.ts deliberately ` +
        `rather than letting the chain be derived from a half-substituted string.`,
    );
  }
  return resolved;
}

/**
 * The `--config` values in one resolved `args:` string, **in order**.
 *
 * Both spellings the Tauri CLI accepts are recognised: the long `--config` and the short `-c`, each in
 * separate-token (`--config x`) and attached (`--config=x`) form. `-c` is not hypothetical — it is a
 * real, documented alias, and a scanner that only knew `--config` would read a workflow switched to it
 * as having NO overlays at all: a guard that silently reports the base config as the whole story,
 * which is this ticket's own defect wearing a different hat.
 *
 * Order is carried through untouched — never sorted, never deduped. RFC 7396 merge is
 * order-dependent, so a chain with the right files in the wrong order computes a config that is not
 * the one that ships, and a membership-only view would call that correct.
 *
 * Quoting is REFUSED rather than parsed. `args:` is a single string that the action splits with its
 * own rules; a quoted path with a space in it would need those rules reimplemented here, and nothing
 * in this repo has one. A quote character therefore throws, so the day someone needs one they get a
 * clear failure instead of a chain silently split down the middle of a path.
 */
export function configOverlaysFromArgs(args: string, where: string): string[] {
  if (/["']/.test(args)) {
    throw new Error(
      `${where}: the args string contains a quote character, which this extractor deliberately does ` +
        `not parse: ${args}. Splitting a quoted path on whitespace would silently produce a wrong ` +
        `chain. Add real quote handling to configOverlaysFromArgs (with cases) rather than guessing.`,
    );
  }
  const tokens = args.split(/\s+/).filter((t) => t.length > 0);
  const out: string[] = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const t = tokens[i];
    const attached = /^(?:--config|-c)=(.*)$/.exec(t);
    if (attached) {
      if (attached[1].length === 0) {
        throw new Error(`${where}: empty --config value in args: ${args}`);
      }
      out.push(attached[1]);
      continue;
    }
    if (t !== "--config" && t !== "-c") continue;
    const value = tokens[i + 1];
    if (value === undefined || value.startsWith("-")) {
      throw new Error(
        `${where}: "${t}" is not followed by a config path in args: ${args}. Refused rather than ` +
          `treated as no overlay.`,
      );
    }
    out.push(value);
    i += 1;
  }
  return out;
}

/**
 * Every matrix leg of one job, as a plain key -> value context.
 *
 * Only `include:`-style matrices are understood, because that is the only shape either release
 * workflow uses. A matrix with product axes (or `exclude:`) THROWS rather than being partially
 * expanded: a half-expanded matrix silently guards fewer builds than ship, which is the exact failure
 * this module exists to remove. A job with no matrix is one leg with an empty context.
 */
export function matrixLegs(job: Record<string, unknown>, where: string): Record<string, string>[] {
  const strategy = job.strategy as Record<string, unknown> | undefined;
  const matrix = strategy?.matrix as Record<string, unknown> | undefined;
  if (matrix === undefined) return [{}];
  const extraKeys = Object.keys(matrix).filter((k) => k !== "include");
  if (extraKeys.length > 0) {
    throw new Error(
      `${where}: the job matrix uses key(s) ${extraKeys.join(", ")} besides "include". Only ` +
        `include-style matrices are expanded here; a product axis or an exclude would be expanded ` +
        `WRONG, guarding fewer builds than ship. Teach matrixLegs the shape deliberately.`,
    );
  }
  const include = matrix.include;
  if (!Array.isArray(include) || include.length === 0) {
    throw new Error(`${where}: the job matrix has no non-empty include list.`);
  }
  return include.map((entry, idx) => {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      throw new Error(`${where}: matrix include entry ${idx} is not a mapping.`);
    }
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(entry as Record<string, unknown>)) {
      out[k] = typeof v === "string" ? v : String(v);
    }
    return out;
  });
}

/**
 * Which shipped OS a GitHub runner label builds for.
 *
 * Matched on the label the workflow actually names, so a version bump (`ubuntu-22.04` ->
 * `ubuntu-24.04`, `macos-latest` -> `macos-15`) needs no edit here. An unrecognised label throws:
 * a runner this guard cannot classify is a build it cannot describe, and silently dropping the leg
 * would narrow the guard exactly the way the hand-copied literal did.
 */
export function shipOSForRunner(runner: string, where: string): ShipOS {
  const label = runner.toLowerCase();
  if (label.includes("windows")) return "windows";
  if (label.includes("macos") || label.includes("mac-")) return "macos";
  if (label.includes("ubuntu") || label.includes("linux")) return "linux";
  throw new Error(
    `${where}: runs-on "${runner}" does not name a recognised OS. Classify it in ` +
      `src/lib/tauriConfigChain.ts rather than leaving a shipped build leg unguarded.`,
  );
}

/**
 * Refuses an overlay path that does not live in the Tauri project directory — see
 * {@link TAURI_PROJECT_DIR}.
 *
 * A `..` SEGMENT is refused before the prefix test, not after it. A bare `startsWith` is a string
 * test, not a path test: `src-tauri/../../planted.conf.json` passes it while resolving outside the
 * repo entirely (measured — that exact path was accepted by the first version of this function).
 * The consequence is mild, which is why this is worth stating rather than assuming: such a file is
 * still loaded, still merged, and still pinned, so the updater assertions fire on it regardless. What
 * was broken was this refusal firing where its own doc comment says it does — a guard that reports
 * clean on the input it names is worse than no guard, because it reads as coverage.
 *
 * RED-PROOFED (2026-08-28): with the `..` filter forced to match nothing, `tauriConfigChain.test.ts`
 * goes to 1 failed / 23 passed, the failure being "refuses a --config that walks back OUT of the
 * project directory with ..". Reverted. The complementary case is pinned too — a filename that merely
 * contains `..` (`tauri..odd.conf.json`) is NOT a traversal and is still accepted, so this cannot be
 * satisfied by refusing every dot-dot it sees.
 */
function assertOverlaysLiveInProjectDir(overlays: string[], where: string): void {
  const normalized = overlays.map((p) => ({ raw: p, path: p.replace(/\\/g, "/") }));
  const traversing = normalized.filter((p) => p.path.split("/").includes(".."));
  if (traversing.length > 0) {
    throw new Error(
      `${where}: --config overlay(s) containing a ".." path segment: ` +
        `${traversing.map((p) => p.raw).join(", ")}. A path that walks up out of ` +
        `${TAURI_PROJECT_DIR}/ is refused rather than resolved — the prefix test below is a STRING ` +
        `test, and "${TAURI_PROJECT_DIR}/../.." satisfies it while naming a file somewhere else ` +
        `entirely. If a config genuinely lives outside the project directory, say so deliberately.`,
    );
  }
  const stray = normalized.filter((p) => !p.path.startsWith(`${TAURI_PROJECT_DIR}/`));
  if (stray.length === 0) return;
  throw new Error(
    `${where}: --config overlay(s) outside ${TAURI_PROJECT_DIR}/: ` +
      `${stray.map((p) => p.raw).join(", ")}. Either the ` +
      `Tauri project moved (update TAURI_PROJECT_DIR in src/lib/tauriConfigChain.ts, which is a ` +
      `stated constant precisely so this reds) or a config is being loaded from somewhere the ` +
      `guards do not look.`,
  );
}

function refuseNearEmpty(kind: string, found: number, floor: number): void {
  if (found >= floor) return;
  throw new Error(
    `${kind} discovery came back near-empty: found ${found}, floor is ${floor}. Either the walk over ` +
      `.github/workflows/ broke, or the "${TAURI_ACTION}" step this derivation keys on was renamed/` +
      `replaced — both of which would leave the shipped config guarded by nothing while every test ` +
      `stayed green (CPE-1932). Lower the floor in src/lib/tauriConfigChain.ts in the same diff as a ` +
      `deliberate channel retirement, and say which channel.`,
  );
}

/**
 * Every `tauri-action` build leg in the repo, matrix-expanded, with its `--config` overlay chain in
 * workflow order. Throws rather than returning a short list — see the floors above.
 */
export function deriveBuildLegs(root: string = process.cwd()): BuildLeg[] {
  const legs: BuildLeg[] = [];
  const workflowsWithBuilds = new Set<string>();
  for (const workflow of discoverWorkflows(root)) {
    const found = legsFromWorkflowDoc(workflow, parseWorkflowFile(workflow, root));
    if (found.length > 0) workflowsWithBuilds.add(workflow);
    legs.push(...found);
  }
  refuseNearEmpty("release workflow", workflowsWithBuilds.size, MIN_EXPECTED_BUILD_WORKFLOWS);
  refuseNearEmpty("build leg", legs.length, MIN_EXPECTED_BUILD_LEGS);
  return legs;
}

/**
 * Every build leg in ONE already-parsed workflow document.
 *
 * Split out from {@link deriveBuildLegs} so the extraction can be exercised against a YAML fixture
 * without a whole fake repository — in particular the CPE-1933 rule-2 property, that a `--config`
 * written in a COMMENT or inside a `run:` script is not mistaken for one the build receives. That is
 * true by construction here (the parser has already discarded comments; only `with.args` and
 * `runs-on` values are ever read), and `tauriConfigChain.test.ts` pins it with fixtures rather than
 * leaving it as a claim about a mechanism.
 */
export function legsFromWorkflowDoc(workflow: string, document: unknown): BuildLeg[] {
  const legs: BuildLeg[] = [];
  const doc = (document ?? {}) as { jobs?: Record<string, Record<string, unknown>> };
  for (const [job, jobDoc] of Object.entries(doc.jobs ?? {})) {
    const steps = (jobDoc.steps ?? []) as Record<string, unknown>[];
    for (const step of steps) {
      const uses = step.uses;
      if (typeof uses !== "string" || !uses.startsWith(TAURI_ACTION)) continue;
      const stepName = typeof step.name === "string" ? step.name : "(unnamed)";
      const base = `${workflow} [${job} / ${stepName}]`;
      const runsOn = jobDoc["runs-on"];
      if (typeof runsOn !== "string") {
        throw new Error(`${base}: the job's runs-on is not a single string; cannot classify its OS.`);
      }
      const withBlock = (step.with ?? {}) as Record<string, unknown>;
      const rawArgs = withBlock.args === undefined ? "" : withBlock.args;
      if (typeof rawArgs !== "string") {
        throw new Error(`${base}: the tauri-action step's \`args:\` is not a string.`);
      }
      for (const leg of matrixLegs(jobDoc, base)) {
        const runner = resolveMatrixRefs(runsOn, leg, `${base} runs-on`);
        const args = resolveMatrixRefs(rawArgs, leg, `${base} args`);
        const where = `${base} on ${runner}`;
        const overlays = configOverlaysFromArgs(args, where);
        assertOverlaysLiveInProjectDir(overlays, where);
        legs.push({
          workflow,
          job,
          step: stepName,
          runner,
          os: shipOSForRunner(runner, where),
          overlays,
          args,
          where,
        });
      }
    }
  }
  return legs;
}

