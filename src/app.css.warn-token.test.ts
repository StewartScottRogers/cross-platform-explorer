// CPE-1810: `--warn` was referenced across five components (AgentTimeline/ConsentSheet/
// ExplorerPane/ImageCompareView/SidecarManager) as `var(--warn, <hex>)` for years without ever
// being defined anywhere in src/ — the token itself always resolved to nothing, so the literal hex
// fallback silently won at every call site, giving a "caution" colour that never changed with the
// theme (least legible in the one theme — dark — that most needed it right). This is the guard
// that CPE-1810 shipped alongside the fix, so it can't silently regress back to that shape.
//
// CPE-1821 found the SAME defect class at three more tokens — `--text-muted`, `--accent-2`,
// `--bg-dim` — referenced app-wide via `var(--token, <fallback>)` but defined nowhere, and
// extended this file (rather than adding a parallel guard, per that ticket's own instruction) to
// cover all five previously-undefined tokens with the identical three invariants below.
//
// Three invariants, independent of every other app.css guard:
//  (a) every guarded token resolves to a concrete hex in ALL FIVE live theme selectors (bare
//      `:root`, light, dark, hc-light, hc-dark) — not just light/dark. This is deliberately a HARD
//      failure, unlike src/app.css.solid-fill-contrast.test.ts's/hc-solid-fill-contrast.test.ts's
//      own resolution, which silently SKIPS asserting a pairing it can't resolve (documented there
//      as "nothing to assert against statically") — exactly the blind spot that let --warn (and
//      later --text-muted/--accent-2/--bg-dim) go undefined for years without any test failing. A
//      silent skip cannot read as a pass (CPE-1806); this test makes "missing from a theme" loud
//      instead.
//  (b) no `.svelte` component still writes `var(--<token>` with a fallback, for any guarded token —
//      the "half-migration" CPE-1810's ticket explicitly calls worse than none, since it would leave
//      some call sites live-themed and others silently stuck on a hex again.
//  (c) none of the raw literals CPE-1810 exists to retire — `#b5872b`/`#b8860b` (both named by that
//      ticket's own "Why it matters") — appears anywhere in a `.svelte` file for a caution/warning
//      use. (a) and (b) above were written against the FIRST sweep's own blind spot: grepping for
//      `var(--token, <fallback>)` only ever finds sites that already reference `--warn` through
//      `var()` — it is structurally incapable of seeing a site that hard-codes the identical amber
//      literal directly with NO token reference at all, which is exactly how a PR review round found
//      19 more real call sites (20 occurrences) this ticket's first pass had missed entirely. This
//      third invariant guards the actual literals, not the idiom around them, so that blind spot
//      can't recur the same way twice. CPE-1821 did NOT extend this specific sub-guard to
//      `--text-muted`'s old `#9a9a9a` fallback or `--bg-dim`'s old `#0f0f0f` fallback: both are
//      generic enough greys/near-blacks that they have legitimate unrelated uses elsewhere (e.g.
//      DiskSpaceView.svelte's `var(--text-dim, #9a9a9a)` — a REAL, already-defined token's own
//      harmless leftover fallback — and Icon.svelte's `stroke="#9a9a9a"`, a plain SVG icon colour
//      with no theme-token meaning at all), so a blanket literal-ban would false-flag code this
//      ticket has no business touching. `--accent-2`'s `#209764` IS specific enough (grepped as the
//      only occurrence in src/ before this ticket) to guard the same way `--warn`'s literals are —
//      see WARN_LITERAL_RE below.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");

/** Bodies of every top-level block matching `selector { ... }` (brace-balanced), in source order —
 *  same brace-balanced helper every other app.css guard in this repo duplicates (single-file-per-
 *  guard precedent — see src/app.css.dark-contrast.test.ts's header comment for why). */
function allBlocks(source: string, selector: RegExp): string[] {
  const out: string[] = [];
  let m: RegExpExecArray | null;
  const re = new RegExp(selector, selector.flags.includes("g") ? selector.flags : selector.flags + "g");
  while ((m = re.exec(source)) !== null) {
    const open = source.indexOf("{", m.index);
    let depth = 0;
    for (let i = open; i < source.length; i++) {
      if (source[i] === "{") depth++;
      else if (source[i] === "}") {
        depth--;
        if (depth === 0) {
          out.push(source.slice(open + 1, i));
          break;
        }
      }
    }
  }
  return out;
}

function extractDecls(block: string): Map<string, string> {
  const clean = stripComments(block);
  const decls = new Map<string, string>();
  const re = /(--[a-zA-Z0-9-]+)\s*:\s*([^;]+);/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2].trim());
  return decls;
}

const HEX_RE = /^#[0-9a-fA-F]{3,8}$/;

/** All `--pal-*`/`--pal-dark-*`/`--pal-hc-light-*`/`--pal-hc-dark-*` primitive declarations,
 *  regardless of which bare :root block declares them. */
function allPaletteDecls(): Map<string, string> {
  const decls = new Map<string, string>();
  for (const block of allBlocks(css, /:root\s*\{/)) {
    for (const [name, value] of extractDecls(block)) {
      if (/^--pal-/.test(name) && HEX_RE.test(value)) decls.set(name, value);
    }
  }
  return decls;
}
const paletteDecls = allPaletteDecls();

/** Resolve a semantic token's value through the palette layer to a concrete hex, following a
 *  `var(--pal-...)` reference (or a `var(--warn)` self-reference for --warn-fill's alias case). No
 *  fallback resolution here on purpose — a token this guard checks must resolve on its own primary
 *  reference, not fall through to something else (that's exactly the "half-defined" shape being
 *  guarded against). */
function resolveHex(semanticDecls: Map<string, string>, name: string, depth = 0): string | undefined {
  if (depth > 5) return undefined;
  const value = semanticDecls.get(name);
  if (!value) return undefined;
  if (HEX_RE.test(value)) return value;
  const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!varMatch) return undefined;
  const ref = varMatch[1];
  if (paletteDecls.has(ref)) return paletteDecls.get(ref);
  if (semanticDecls.has(ref)) return resolveHex(semanticDecls, ref, depth + 1);
  return undefined;
}

const THEMES: { label: string; selector: RegExp }[] = [
  { label: "bare :root (default)", selector: /:root\s*\{/ },
  { label: ':root[data-theme="light"]', selector: /:root\[data-theme="light"\]\s*\{/ },
  { label: ':root[data-theme="dark"]', selector: /:root\[data-theme="dark"\]\s*\{/ },
  { label: ':root[data-theme="hc-light"]', selector: /:root\[data-theme="hc-light"\]\s*\{/ },
  { label: ':root[data-theme="hc-dark"]', selector: /:root\[data-theme="hc-dark"\]\s*\{/ },
];

/** The bare :root block that carries the semantic (not palette) layer — the one declaring --bg,
 *  same finder src/app.css.test.ts already established. */
function semanticDeclsFor(label: string, selector: RegExp): Map<string, string> {
  if (label === "bare :root (default)") {
    const bareBlocks = allBlocks(css, selector);
    // Three bare :root blocks carry --pal-* declarations (light palette, dark palette, hc palette);
    // the semantic (fallback/default) block is the one with a real --bg declaration that ISN'T
    // itself a palette block (a palette block only ever declares --pal-* raw hexes, never --bg).
    const block = bareBlocks.find((b) => /--bg\s*:/.test(b) && !/--pal-[a-zA-Z0-9-]+:\s*#/.test(b));
    if (!block) throw new Error("could not find the bare :root semantic block (the one declaring --bg)");
    return extractDecls(block);
  }
  const blocks = allBlocks(css, selector);
  if (blocks.length !== 1) throw new Error(`expected exactly one ${label} block, found ${blocks.length}`);
  return extractDecls(blocks[0]);
}

// The five tokens the OLD hard-coded coverage block enumerated by hand. Kept only as the input to
// the still-scoped "no dead fallback" (b) and "no raw literal" (c) checks below, which carry extra
// meaning (they're retiring a SPECIFIC historical fallback idiom / literal, not just re-deriving
// coverage) and which this ticket's own instructions say to keep. --warn/--warn-fill are CPE-1810's
// originals; --text-muted/--accent-2/--bg-dim are CPE-1821's three.
const GUARDED_TOKENS = ["--warn", "--warn-fill", "--text-muted", "--accent-2", "--bg-dim"];

// ---------------------------------------------------------------------------------------------
// CPE-1875: the detector that replaces the hard-coded five-name list above as the COVERAGE
// mechanism. The old block only ever asserted resolution for GUARDED_TOKENS — anything outside
// that list was invisible to it, which is exactly how CPE-1810 and CPE-1821 each had to be found
// by hand, and how a SIXTH undefined token (`--mono`, ~24 call sites, font-family fallback rather
// than a colour — see CPE-1876) shipped green through this file without anyone noticing. Instead
// of hand-listing tokens, this DISCOVERS every `var(--token, <fallback>)` call site across every
// `.svelte` component (and any standalone `.css`/`.ts` that emits one — none currently do outside
// app.css itself, which is the definer, not a consumer, and is excluded below) and requires each
// discovered token to resolve to a concrete hex in all five live theme blocks — the identical bar
// the old list enforced for its five names, now applied to every name automatically.
function walkTokenSources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      walkTokenSources(p, out);
    } else if (
      (name.endsWith(".svelte") || name.endsWith(".css") || name.endsWith(".ts")) &&
      !name.endsWith(".test.ts") &&
      !name.endsWith(".spec.ts") &&
      p !== APP_CSS_PATH
    ) {
      out.push(p);
    }
  }
  return out;
}

/** Strips a line comment ONLY when `//` (optional leading whitespace) is the FIRST thing on the
 *  line — to end of line. Nothing else is inspected: no quote state, no regex-literal awareness.
 *
 *  CPE-1875 re-review attempt 2 tried a quote-aware scanner that tracked `'`/`"`/`` ` `` string
 *  state so a `//` inside a string (a URL, plain text) would survive. That scanner had no concept
 *  of a REGEX LITERAL — `\/\/` inside `/\/\//` (an ordinary pattern for matching a literal double
 *  slash) tripped the same `//`-start check and silently deleted the rest of that line, including
 *  any real `var(--token, <fallback>)` call site sharing it. Proven by a controlled A/B: an
 *  undefined token alone on its own line was caught (5 failures); the identical token sharing a
 *  line with `const R = /\/\//;` vanished from output entirely, suite fully green — a FALSE
 *  NEGATIVE, the guard going blind with no signal anything was missed. That is strictly worse than
 *  the false positive (a `//` doc comment on its own line quoting the idiom by name) this exists
 *  to fix in the first place.
 *
 *  Distinguishing a real comment from one inside a string from one inside a regex literal from one
 *  inside a template-literal expression is a full tokenizer's job, and every attempt to approximate
 *  it here has only grown a new failure mode. So this deliberately stops trying and biases to FAIL
 *  SAFE instead: a comment-ONLY line (nothing but whitespace before `//`) is stripped — that is the
 *  one shape that can never contain a real call site, so it can never eat one, no matter what
 *  regexes, strings, or escapes appear on OTHER lines. A trailing `// comment` after real code is
 *  deliberately left alone — worst case it gets scanned as if it were code, producing a false
 *  positive (noise: an extra failure naming a token that's actually just prose), which is rare and,
 *  if it ever happens, allowlistable. That is the safe direction to err in: under-stripping only
 *  adds noise, while over-stripping silently blinds the guard. */
function stripLineComments(s: string): string {
  return s
    .split("\n")
    .map((line) => (/^\s*\/\//.test(line) ? "" : line))
    .join("\n");
}

const stripBlockAndHtmlComments = (s: string) => s.replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
const stripAllComments = (s: string) => stripLineComments(stripBlockAndHtmlComments(s));

/** Every top-level `var(--token, <fallback>)` 2-argument call in `content`, fully balanced-paren
 *  parsed so a fallback that itself contains parens (`var(--a, var(--b, #fff))`, `color-mix(...)`)
 *  doesn't truncate at the wrong comma. A bare `var(--token)` with no fallback is not this idiom
 *  and is skipped — an undefined token with no fallback fails loudly/visibly (nothing renders),
 *  which is a different, self-announcing bug, not the "looks themed but silently isn't" shape this
 *  file guards. */
function scanFallbackCalls(content: string): { token: string; fallback: string }[] {
  const out: { token: string; fallback: string }[] = [];
  const openRe = /var\(/g;
  let m: RegExpExecArray | null;
  while ((m = openRe.exec(content)) !== null) {
    let depth = 1;
    let i = m.index + m[0].length;
    for (; i < content.length && depth > 0; i++) {
      if (content[i] === "(") depth++;
      else if (content[i] === ")") depth--;
    }
    const inner = content.slice(m.index + m[0].length, i - 1);
    let innerDepth = 0;
    let commaAt = -1;
    for (let j = 0; j < inner.length; j++) {
      if (inner[j] === "(") innerDepth++;
      else if (inner[j] === ")") innerDepth--;
      else if (inner[j] === "," && innerDepth === 0) {
        commaAt = j;
        break;
      }
    }
    if (commaAt === -1) continue;
    const token = inner.slice(0, commaAt).trim();
    const fallback = inner.slice(commaAt + 1).trim();
    if (/^--[a-zA-Z0-9-]+$/.test(token)) out.push({ token, fallback });
  }
  return out;
}

/** True if `fallback` is (only) a nested `var(--otherToken ...)` reference and `otherToken` itself
 *  resolves to a concrete hex in every one of the five theme blocks. Distinguishes the two shapes
 *  a 2-argument `var()` can take when its OWN token is undefined:
 *    - `var(--undefinedToken, #hex)` / `var(--undefinedToken, ui-monospace, monospace)` — the
 *      fallback is a LITERAL. The literal is the only value that will ever render, in every theme,
 *      forever — this is the exact CPE-1810/1821/1876 defect shape and must resolve or be
 *      allowlisted as debt.
 *    - `var(--undefinedToken, var(--realToken))` — the fallback is itself a LIVE, already-verified
 *      token reference. `--undefinedToken` is dead weight (an odd, never-populated first choice)
 *      but every render already resolves through to `--realToken`'s correctly-themed value in every
 *      theme, so this is NOT the masking bug — nothing ever silently locks to an untheed literal.
 *      `--agent-accent` (FileList.svelte, `var(--agent-accent, var(--agent-unknown))`),
 *      `--accent-soft` (IcalPreview.svelte, `var(--accent-soft, var(--surface))`) and `--surface-2`
 *      (DiffPeek.svelte, `var(--surface-2, var(--surface, #181818))`) are the current instances —
 *      already investigated and explicitly ruled out as "not this bug" by CPE-1876's own UAT pass.
 */
function fallbackIsLiveChain(fallback: string, semanticDeclsByTheme: Map<string, string>[]): boolean {
  const chain = fallback.match(/^var\(\s*(--[a-zA-Z0-9-]+)\s*(?:,[\s\S]*)?\)$/);
  if (!chain) return false;
  const refToken = chain[1];
  return semanticDeclsByTheme.every((decls) => HEX_RE.test(resolveHex(decls, refToken) ?? ""));
}

const allSemanticDecls = THEMES.map(({ label, selector }) => semanticDeclsFor(label, selector));

/** token -> referencing files (relative, forward-slashed), in first-seen order.
 *  `discovered` excludes tokens whose every fallback occurrence is a live chain to an
 *  already-themed token (see the function above) — that is not an instance of the defect this file
 *  guards against. Component-local custom properties (`--indent`, `--sw`, ...) are NOT filtered
 *  here — see LOCAL_CUSTOM_PROPERTIES below for why that exemption moved to an explicit list instead
 *  of living in this scan.
 *  `allSeen` is the SAME scan before the live-chain filter is applied — every token with at least
 *  one real `var(--token, <fallback>)` call site anywhere in `src/`, full stop. Used by the
 *  LOCAL_CUSTOM_PROPERTIES stale-entry check below: `--sw`'s fallback (`var(--surface-alt)`) is
 *  itself a live chain, so `--sw` is ALREADY absent from `discovered` by the time that check would
 *  run against it — checking against `discovered` there would misreport a real, live local property
 *  as a stale entry. `allSeen` is call-site truth, independent of which exemption mechanism would
 *  end up covering the token. */
function discoverFallbackTokens(): { discovered: Map<string, string[]>; allSeen: Map<string, string[]> } {
  const found = new Map<string, string[]>();
  const liveChainOnly = new Map<string, boolean>(); // token -> "every occurrence so far is a live chain"
  for (const f of walkTokenSources(SRC)) {
    const content = stripAllComments(readFileSync(f, "utf8"));
    const rel = f.replace(SRC, "src").replace(/\\/g, "/");
    for (const { token, fallback } of scanFallbackCalls(content)) {
      const isLiveChain = fallbackIsLiveChain(fallback, allSemanticDecls);
      liveChainOnly.set(token, (liveChainOnly.get(token) ?? true) && isLiveChain);
      const files = found.get(token) ?? [];
      files.push(rel);
      found.set(token, files);
    }
  }
  const allSeen = new Map(found);
  for (const [token, allLiveChain] of liveChainOnly) if (allLiveChain) found.delete(token);
  return { discovered: found, allSeen };
}

// Known-open debt: tokens the detector above genuinely finds undefined-with-a-literal-fallback in
// at least one theme block TODAY, each pinned to the ticket that owns fixing it, dated when it was
// added here. This is what keeps the guard green without silently hiding the gap — an allowlisted
// token still shows up in `npx vitest` output (search this file's describe title), and the "stale
// entry" check below fails the moment an owning ticket actually fixes its token and someone forgets
// to delete the entry, so the list can't quietly go stale in either direction (silently grow OR
// silently outlive its bug). Disposition chosen over splitting the work: CPE-1876 already exists
// and owns `--mono` end to end (~24 call sites — a real, scoped follow-up); re-deriving that fix
// inside this coverage ticket would duplicate that ticket's scope for no benefit, and an unlisted
// new token still fails immediately, which is the property this ticket exists to establish.
const ALLOWLIST: { token: string; ticket: string; added: string; note: string }[] = [
  {
    token: "--mono",
    ticket: "CPE-1876",
    added: "2026-08-26",
    note: "font-family fallback (`ui-monospace, monospace`) masks an undefined token at ~24 call " +
      "sites across the app — the sixth occurrence of the CPE-1810/1821 defect shape, surfaced by " +
      "this ticket's detector. CPE-1876 owns defining --mono (or retokenizing its call sites) in " +
      "all five theme blocks.",
  },
];
const ALLOWLISTED_TOKENS = new Set(ALLOWLIST.map((e) => e.token));

// Explicit, CLOSED allowlist of genuinely COMPONENT-LOCAL custom properties — tokens a component
// sets on itself (via an inline `style="--foo: ..."` attribute) that are never expected to come
// from app.css's cascade at all, so requiring them to resolve there would assert something false
// about code that isn't broken.
//
// CPE-1875 re-review attempt 3: this REPLACES a heuristic, not extends one. Two successive
// heuristics tried to infer "is this exemption legitimate" from a declaration's static text —
// attempt 1 checked only that a `token:` declaration existed anywhere in the file; attempt 2
// tightened that to require the declared value contain a `{`. Both were defeated in the same shape
// two seconds after being proposed: `style="--newtok: {'#654321'}"` (a braced STRING LITERAL) and
// `style="--newtok: {SOME_CONST}"` (a braced reference to a script-level constant that is itself
// `"#654321"`) both satisfy "contains `{`" while being exactly as static and exactly as
// theme-invariant as a bare hex — the CPE-1810/1821/1876 defect wearing a `{}` costume. Confirmed
// by construction: injected alone into a real component, the full suite ran green with zero
// mention of the token. No further heuristic on the STATIC TEXT of a declaration can distinguish
// "genuinely computed" from "static value dressed up to look computed", so this stops guessing.
//
// Enumeration is the RIGHT tool here, unlike the token-name enumeration this very ticket replaced
// above: that failed because the surface being enumerated was open-ended and attacker/author
// chosen (any token name, at any new call site, forever). This surface is a small, closed set of
// AUTHOR-chosen custom-property NAMES that a new masking literal cannot add itself to merely by
// existing — getting a token onto this list requires an actual entry in this file, i.e. a
// deliberate, reviewable code change, which is exactly the property a heuristic can never have.
// Same dated, ticket-owning shape as ALLOWLIST above so the two lists read alike; here `ticket`
// names the ticket that made the reviewable decision to allowlist the entry (not one that owes a
// future fix — there is nothing to fix, these are permanently local by design).
const LOCAL_CUSTOM_PROPERTIES: { token: string; ticket: string; added: string; note: string }[] = [
  {
    token: "--indent",
    ticket: "CPE-1875",
    added: "2026-08-26",
    note: 'PreviewPane.svelte sets it per-line from fold state (`style="--indent: {indent[i] ?? 0}"`) ' +
      "to drive a background-size calc() for indent guides — a per-row layout NUMBER, not a colour, " +
      "and never a semantic theme token.",
  },
  {
    token: "--sw",
    ticket: "CPE-1875",
    added: "2026-08-26",
    note: "TagEditor.svelte sets it per-swatch from a fixed LABEL_COLORS lookup " +
      "(`` `--sw: ${LABEL_COLORS[key]}` ``) to paint that one swatch — a per-instance value drawn " +
      "from a fixed palette, not a token app.css is expected to define.",
  },
];
const LOCAL_CUSTOM_PROPERTY_TOKENS = new Set(LOCAL_CUSTOM_PROPERTIES.map((e) => e.token));

const { discovered, allSeen } = discoverFallbackTokens();

describe("every var(--token, <fallback>) call site resolves to a real hex in every live theme, or is on an explicit allowlist (CPE-1875)", () => {
  for (const { label, selector } of THEMES) {
    const semanticDecls = semanticDeclsFor(label, selector);

    for (const [token, files] of discovered) {
      if (ALLOWLISTED_TOKENS.has(token) || LOCAL_CUSTOM_PROPERTY_TOKENS.has(token)) continue;
      it(`${label} defines ${token} as a concrete hex (referenced from ${files.join(", ")})`, () => {
        const hex = resolveHex(semanticDecls, token);
        // CPE-1875 re-review finding 3: `expect(undefined).toMatch(HEX_RE)` throws vitest's own
        // "expects to receive a string, but got undefined" TypeError BEFORE this custom message is
        // ever used — the remediation advice below was unreachable dead code. Asserting definedness
        // FIRST (with the same message) means the failure that actually fires carries the message.
        const msg = `${token} (referenced from ${files.join(", ")}) did not resolve to a hex in ${label} — got raw value ${JSON.stringify(semanticDecls.get(token))}. If this is known, pre-existing debt, add a dated entry to ALLOWLIST above pointing at the ticket that owns it — do not fix the underlying token here unless that IS the ticket this work is filed under.`;
        expect(hex, msg).toBeDefined();
        expect(hex, msg).toMatch(HEX_RE);
      });
    }
  }

  it("the five originally-guarded tokens still resolve to a real hex in every theme (no silent narrowing)", () => {
    // These five no longer have a LIVE var(--token, <fallback>) call site at all — invariant (b)
    // below already bans that idiom for them outright, so `discovered` correctly finds nothing to
    // check. What must not regress is the tokens' own definitions: assert resolution directly,
    // independent of any call site existing, so this file keeps proving the exact bar CPE-1810/1821
    // established for these five, and (b)'s ban means the generic detector above would immediately
    // pick back up any of them the moment a fallback idiom reappeared for one.
    for (const { label, selector: sel } of THEMES) {
      const decls = semanticDeclsFor(label, sel);
      for (const token of GUARDED_TOKENS) {
        const hex = resolveHex(decls, token);
        const msg = `${token} did not resolve to a hex in ${label} — got raw value ${JSON.stringify(decls.get(token))}`;
        expect(hex, msg).toBeDefined();
        expect(hex, msg).toMatch(HEX_RE);
      }
    }
  });

  it("every allowlisted token is genuine, still-open debt — not a stale entry masking a fix that already landed", () => {
    for (const { token, ticket } of ALLOWLIST) {
      expect(discovered.has(token), `${token} is on the ALLOWLIST for ${ticket} but no var(${token}, <fallback>) call site was found anywhere in src/ — remove the stale allowlist entry.`).toBe(true);
      const stillBroken = THEMES.some(({ label, selector }) => {
        const semanticDecls = semanticDeclsFor(label, selector);
        return !HEX_RE.test(resolveHex(semanticDecls, token) ?? "");
      });
      expect(stillBroken, `${token} is allowlisted for ${ticket} as known-open debt, but it now resolves to a hex in every theme block — the fix has landed. Remove this allowlist entry so the detector covers ${token} for real.`).toBe(true);
    }
  });

  it("every LOCAL_CUSTOM_PROPERTIES entry still has a live call site (no stale entry enumerated for nothing)", () => {
    for (const { token, ticket } of LOCAL_CUSTOM_PROPERTIES) {
      expect(allSeen.has(token), `${token} is on LOCAL_CUSTOM_PROPERTIES (${ticket}) but no var(${token}, <fallback>) call site was found anywhere in src/ — remove the stale entry.`).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------------------------
// No component may reintroduce the undefined-token-with-hex-fallback idiom this ticket removed.
function walkSvelte(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkSvelte(p, out);
    else if (name.endsWith(".svelte")) out.push(p);
  }
  return out;
}

// Matches `var(--warn`/`var(--warn-fill`/`var(--text-muted`/`var(--accent-2`/`var(--bg-dim`
// immediately followed by a fallback comma — i.e. the exact idiom this guard exists to retire, for
// ANY of the guarded tokens, built from GUARDED_TOKENS so a future token added to that list is
// automatically covered here too.
const FALLBACK_IDIOM_RE = new RegExp(
  `var\\(\\s*(?:${GUARDED_TOKENS.map((t) => t.replace(/[-/\\^$*+?.()|[\]{}]/g, "\\$&")).join("|")})\\s*,`,
  "g",
);

describe("no .svelte component falls back to a hard-coded hex for a guarded token (CPE-1810/CPE-1821)", () => {
  it("no `var(--warn`/`var(--warn-fill`/`var(--text-muted`/`var(--accent-2`/`var(--bg-dim` call site carries a fallback", () => {
    const offenders: string[] = [];
    for (const f of walkSvelte(SRC)) {
      // Strip <!-- --> and /* */ comments first — AgentTimeline.svelte, SidecarManager.svelte, and
      // TrashView.svelte all carry doc comments that mention an old `var(--token, <hex>)` idiom as
      // PROSE, quoting it by name to explain why it was avoided/replaced; those aren't real CSS call
      // sites and must not trip this guard.
      const raw = readFileSync(f, "utf8");
      const content = raw.replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
      if (FALLBACK_IDIOM_RE.test(content)) offenders.push(f.replace(SRC, "src").replace(/\\/g, "/"));
    }
    expect(offenders, `component(s) still using var(--<guarded-token>, <fallback>) instead of the real token: ${offenders.join(", ")}`).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// The bare literals CPE-1810/CPE-1821 exist to retire must not reappear for the semantic use they
// replaced, anywhere in a .svelte file, independent of whether they arrive via
// var(--token, <fallback>) — (b) above — or with no token reference at all, which is the shape (b)
// is structurally blind to (see this file's header comment). A PR review round found 19 such sites
// (20 occurrences) across 11 components on the first pass alone, so this guard exists specifically
// because grep-for-the-idiom was proven insufficient once already. Only literals specific enough to
// be unambiguous are listed here — see this file's header comment for why --text-muted's
// `#9a9a9a`/--bg-dim's `#0f0f0f` are deliberately NOT included (both have legitimate unrelated uses
// elsewhere that would false-flag).
const WARN_LITERAL_RE = /#(?:b5872b|b8860b)\b/i;
const ACCENT_2_LITERAL_RE = /#209764\b/i;

describe("the raw --warn/--accent-2 hex literals never reappear in a .svelte file (CPE-1810/CPE-1821)", () => {
  it("no .svelte file hard-codes #b5872b, #b8860b, or #209764", () => {
    const offenders: string[] = [];
    for (const f of walkSvelte(SRC)) {
      // Only .svelte files are walked (see walkSvelte above) — src/lib/sessionChip.ts's own
      // #b5872b is a .ts file and so is never a candidate here. That exclusion is deliberate even
      // beyond the file-extension scope: sessionChip.ts's amber is a FIXED categorical
      // session-identity palette colour (one of several stable hues assigned round-robin to
      // distinguish concurrent agent sessions), not a caution/warning semantic — its own comment
      // says so — so it must stay theme-invariant and must never be pointed at --warn.
      const content = readFileSync(f, "utf8");
      if (WARN_LITERAL_RE.test(content) || ACCENT_2_LITERAL_RE.test(content)) offenders.push(f.replace(SRC, "src").replace(/\\/g, "/"));
    }
    expect(offenders, `.svelte file(s) still hard-coding a raw --warn/--accent-2 hex instead of the token: ${offenders.join(", ")}`).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// CPE-1821's own "guard gap worth closing" note: none of src/app.css.light-contrast.test.ts /
// dark-contrast.test.ts / hc-contrast.test.ts / solid-fill-contrast.test.ts check a token against a
// COMPONENT-LOCAL fixed background — they all reason about --surface/--bg (or white, for the
// solid-fill scanners) directly. That's exactly how CPE-1810 round 2's `.log-error`/`.log-warn`
// retokenization onto var(--danger)/var(--warn) shipped a real regression against
// SidecarManager.svelte's `.logs` pane (measured against --surface, which that pane never actually
// renders — its background was the fixed #0f0f0f literal `--bg-dim`'s fallback, undefined
// everywhere) and had to be reverted in round 3. Now that CPE-1821 makes `--bg-dim` a real
// per-theme token, `.log-error`/`.log-warn` are retokenized again (see SidecarManager.svelte) — this
// guard is what makes that safe by construction: it resolves `--bg-dim` to its ACTUAL per-theme hex
// (not assumed to be --surface) and asserts `--danger`/`--warn` against THAT, closing the exact
// blind spot named above so a future re-pointing of --bg-dim away from --surface would fail here
// instead of shipping silently broken contrast in the log pane again.
function relativeLuminance(hex: string): number {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.substring(i, i + 2), 16)).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrastRatio(hexA: string, hexB: string): number {
  const lA = relativeLuminance(hexA);
  const lB = relativeLuminance(hexB);
  const lighter = Math.max(lA, lB);
  const darker = Math.min(lA, lB);
  return (lighter + 0.05) / (darker + 0.05);
}

const BG_DIM_CONTRAST_FLOORS: Record<string, number> = {
  "bare :root (default)": 4.5,
  ':root[data-theme="light"]': 4.5,
  ':root[data-theme="dark"]': 4.5,
  ':root[data-theme="hc-light"]': 7,
  ':root[data-theme="hc-dark"]': 7,
};

// src/app.css.solid-fill-contrast.test.ts / hc-solid-fill-contrast.test.ts's dynamic scanners
// discover the `.mirror.auto { background: var(--accent-2) }` + inherited `.mirror { color: #fff }`
// pairing automatically — but only for light/dark: the hc scanner resolves ONLY `--pal-hc-light-*`/
// `--pal-hc-dark-*`-prefixed primitives (by design, documented in its own header), so it can't see
// `--accent-2` -> `var(--pal-accent2-fill)`, an intentionally theme-invariant, unprefixed primitive
// (see app.css's own comment on --pal-accent2-fill for why one value serves every theme) — the
// pairing is silently SKIPPED there rather than asserted. This closes that gap directly, using this
// file's own theme-agnostic resolveHex (which — unlike the hc scanner — already proved above it
// resolves --accent-2 in all five theme selectors) instead of relying on the hc scanner ever
// learning to look outside its prefix.
describe("white text on --accent-2 clears WCAG's 3:1 UI-component floor in every theme (CPE-1821)", () => {
  for (const { label, selector } of THEMES) {
    const semanticDecls = semanticDeclsFor(label, selector);

    it(`${label}: white on --accent-2 clears >=3:1 (BackupDashboard .mirror.auto)`, () => {
      const accent2 = resolveHex(semanticDecls, "--accent-2");
      expect(accent2, `--accent-2 did not resolve to a hex in ${label}`).toMatch(HEX_RE);
      const ratio = contrastRatio("#ffffff", accent2!);
      expect(ratio, `white on --accent-2 (${accent2}) in ${label} = ${ratio.toFixed(2)}:1, want >=3:1`).toBeGreaterThanOrEqual(3);
    });
  }
});

describe("--danger/--warn clear WCAG against --bg-dim's OWN resolved background, not --surface (CPE-1821)", () => {
  for (const { label, selector } of THEMES) {
    const semanticDecls = semanticDeclsFor(label, selector);
    const floor = BG_DIM_CONTRAST_FLOORS[label];

    it(`${label}: --danger and --warn vs the real --bg-dim hex clear >=${floor}:1 (SidecarManager .log-error/.log-warn's actual backdrop)`, () => {
      const bgDim = resolveHex(semanticDecls, "--bg-dim");
      const danger = resolveHex(semanticDecls, "--danger");
      const warn = resolveHex(semanticDecls, "--warn");
      expect(bgDim, `--bg-dim did not resolve to a hex in ${label}`).toMatch(HEX_RE);
      expect(danger, `--danger did not resolve to a hex in ${label}`).toMatch(HEX_RE);
      expect(warn, `--warn did not resolve to a hex in ${label}`).toMatch(HEX_RE);

      const dangerRatio = contrastRatio(danger!, bgDim!);
      const warnRatio = contrastRatio(warn!, bgDim!);
      expect(dangerRatio, `--danger (${danger}) on --bg-dim (${bgDim}) in ${label} = ${dangerRatio.toFixed(2)}:1, want >=${floor}:1`).toBeGreaterThanOrEqual(floor);
      expect(warnRatio, `--warn (${warn}) on --bg-dim (${bgDim}) in ${label} = ${warnRatio.toFixed(2)}:1, want >=${floor}:1`).toBeGreaterThanOrEqual(floor);
    });
  }
});

