# CLAUDE.md

Guidance for AI assistants (and humans) maintaining this repository.

## Purpose (read first)

See [PURPOSE.md](PURPOSE.md) for the app's guiding purpose statement and its
design tiebreaker. This app is a general cross-platform file explorer.

Modes are additive views layered over the explorer.
[AGENT-WATCH.md](AGENT-WATCH.md) describes the planned Agent Watch mode — a live
view of an AI coding agent's filesystem activity.

**Precedence:** inside Agent Watch, visibility outranks the explorer's
fast/small/predictable tiebreaker. If showing what the agent is doing costs
speed, size, or simplicity, pay the cost. The single hard constraint: with the
mode switched off, the plain explorer must remain fast, small, and predictable.

## What this is

A Tauri v2 desktop file explorer. Frontend is Svelte + TypeScript in `src/`.
Backend is Rust in `src-tauri/`. The app auto-updates via the Tauri updater
plugin, and CI builds/signs releases through GitHub Actions.

## Common commands

- `npm install` — install frontend deps
- `npm run tauri dev` — run the app with hot reload
- `npm run tauri build` — build local installers
- `npm run check` — type-check Svelte + TS
- `npm run tauri icon <png>` — regenerate app icons

## How the pieces connect

- The frontend calls Rust via `invoke("command_name", args)`.
- Rust commands live in `src-tauri/src/lib.rs`, annotated with `#[tauri::command]`
  and registered in the `generate_handler!` macro inside `run()`.
- **Adding a backend command:** write the `#[tauri::command]` fn, add it to
  `generate_handler![]`, then call it from Svelte with `invoke`.
- **Permissions:** any plugin capability the frontend uses must be listed in
  `src-tauri/capabilities/default.json`, or the call is denied at runtime.
- **The domain logic lives in `crates/server` (`cpe-server`), not in `lib.rs`.** The Tauri-free
  `cpe-server` crate owns the whole filesystem + preview domain behind a `ServerCtx` seam + the
  `cpe-contract` envelope; a `#[tauri::command]` is a **thin one-line dispatcher** into it (any
  `ipc::Channel` stays in the app adapter). New domain logic goes in `cpe-server`; new format crates go
  there too (the app is deliberately kept lean). Full architecture + the extraction recipe:
  [docs/design/SERVER-ARCHITECTURE.md](docs/design/SERVER-ARCHITECTURE.md) (epic CPE-810).

## Versioning — keep five files in sync

When releasing, bump the version in ALL of:

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`
4. `package-lock.json` — **two** places: the top-level `version` and `packages[""].version`
5. `src-tauri/Cargo.lock` — the `cross-platform-explorer` package entry

Then tag `vX.Y.Z` and push — CI does the rest.

**4 and 5 are the ones that get missed**, and for most of this repo's life nothing failed when they
drifted: neither build passed `--locked`, so both lockfiles were silently rewritten at build time and the
stale version never surfaced as an error. It surfaced instead as a **dirty working tree** the moment
anyone ran `npm install` or a local `cargo build` — which reads as unrelated noise and gets committed by
accident or discarded along with real work. Observed 2026-08-20: `package-lock.json` had been three
releases behind (`0.57.64` vs `0.57.67`).

**Both halves now have a backstop, and they are different mechanisms because npm offers no `--locked`.**

- **Item 5** — CPE-1865 put `--locked` on every Rust build and CPE-1932's `lockfile-preflight` runs
  `cargo metadata --locked` over all 17 `Cargo.lock`s. A drifted `cross-platform-explorer` entry is
  **exit 101** with cargo's own message.
- **Item 4 has no equivalent, measured rather than assumed (CPE-1904).** `npm ci` *is* npm's `--locked`
  and it is already what CI runs — with `package-lock.json`'s two version fields three and five releases
  behind, `npm ci`, `npm test` and `npm run check` all exit **0**, and `npm install` then **silently
  repairs both fields** and exits 0, destroying the evidence. npm treats those fields as metadata to
  rewrite, not as a constraint. So the backstop is `src/lib/appVersionSync.test.ts`: it enumerates every
  place in the tree carrying the app's version — keyed on the package **identity**, not a path list, so
  `gui-smoke/`'s project and the other 16 lockfiles are excluded by what they say about themselves — and
  reds naming the file, the field, both values and the command to run. It covers all six places
  (including item 5, which is cheaper and friendlier to hear about here than an hour into the matrix),
  refuses to render a verdict on a near-empty enumeration, and **throws rather than skipping** any file
  it cannot read or parse.

So a version drift is now a **failing test on every push, PR and local `npm test`** — not a dirty working
tree that reads as noise. If you add a sixth place, that test reds until this list and
`scripts/release.ps1`'s bump plan are updated too.

## Guardrails

- Never commit signing keys (`updater.key`, `*.key`, `.env`). See `.gitignore`.
- The updater `pubkey` and `endpoints` in `tauri.conf.json` must be filled in for
  auto-updates to work (see README "Auto-updates").
- Filesystem commands skip entries they can't read rather than failing the whole
  listing — preserve that behavior when editing `list_dir`.

## UI conventions

- **Menus** — every popup menu (right-click context menus + dropdowns) follows one standard:
  [docs/design/MENUS.md](docs/design/MENUS.md). Key rule: item text is always `var(--text)` (never a
  hard-coded colour, never red for "destructive"); colours come from theme variables so menus are
  identical light/dark and cross-platform. New menus must match it.
- **Tabs** — every tab strip (main window `.tabbar`, AI Console `#tabs`, future ones) uses one
  conventional active-tab treatment: an **accent top-bar** + content-surface background lifting it onto
  the pane, with **inactive tabs as recessed chips** (subtle fill + dimmed text), all from theme
  variables. Standard: [docs/design/TABS.md](docs/design/TABS.md). New tab strips reuse `.tab`/`.tab.active`.
- **Streaming liveness** — producers of large/slow payloads (directory listings, recursive searches,
  future bulk producers) stream results in batches over a Tauri `ipc::Channel` instead of a blocking
  `invoke` that returns one big `Vec`, so the pane paints the first rows immediately. One shared walker
  backs both a collect-to-vec command and its streaming variant; the frontend appends batches, flips
  `loading` off on the first batch, and supersedes an in-flight stream by generation token. Standard:
  [docs/design/STREAMING.md](docs/design/STREAMING.md). New bulk producers follow it. See
  [[prefer-streaming-liveness]].
- **Busy cursor / `invoke`** — production code imports `invoke` from `src/lib/invoke.ts` (the
  busy-tracking wrapper), **never** from `@tauri-apps/api/core`, so a slow command app-wide raises the
  OS wait cursor for free. Operations that render their own progress use `rawInvoke` + the guard-test
  allowlist. Full convention: [docs/design/BUSY-CURSOR.md](docs/design/BUSY-CURSOR.md).
- **Accent colour: `--accent` fills and rings, `--accent-text` reads (CPE-1919).** `--accent` backs
  three roles with three different WCAG bars — a solid button **fill** under white text (3:1), an
  **icon glyph / focus ring / border** (3:1), and **running text** (4.5:1) — and one value cannot be
  optimal for all three. It is tuned for the first two, so painting it as body text shipped the JSON
  preview's string values at **3.70:1** in the dark theme, with every palette guard green because
  each asserted `--accent` only at the loosest of its bars. **A token backing several roles gets
  pinned at the loosest one, and that assertion then reads like coverage.** So: `color:` on text you
  read → `--accent-text`; `background`, `border-color`, focus rings, and icon glyphs → `--accent`.
  Never the reverse — white on `--accent-text` is 2.81:1 (dark) / 2.44:1 (hc-dark), under 1.4.11's
  3:1 UI floor.
  `src/app.css.accent-text-contrast.test.ts` pins both directions, derives the JSON preview's colour
  roles from the component CSS rather than a list, and derives the **painted** surfaces
  (`.preview-pane`'s background, `.jt-row:hover`'s fill) instead of assuming `--bg`. It also sweeps
  **every** `color:` in `src/` resolving to `--accent`/`--accent-hover` and fails on each one unless
  its selector is declared in that file's `ICON_ROLES` list — so accent-coloured text fails on the
  day it lands, and claiming "it's an icon" costs a reviewable diff. Note the
  `var(--accent, <fallback>)` spelling: five of the seven sites the review round caught were hiding
  behind it, invisible to a grep for bare `var(--accent)`.
- **Pills / chips / badges ("tick-tacks")** — a row of pills must **reflow**: the container wraps the
  pills onto more rows and grows its height (`display:flex; flex-wrap:wrap; gap`), while each pill keeps
  its text on **one line** and doesn't shrink (`white-space:nowrap; flex:0 0 auto`; add
  `max-width`+ellipsis for a pill whose own text can be long). **Never** let text wrap inside a pill and
  overflow its background. Applies to context/capability/filter/agent chips everywhere.

## Guards and ratchets

- **Ratchets get their own guard for free (CPE-1934).** A *ratchet* is a stored count — or a stored
  allowlist, which is a count wearing a coat — that a test compares today's measurement against, so a
  defect class can't grow while the existing instances are burnt down (`BASELINE_TOTAL_HEX_OCCURRENCES`,
  `gui-smoke/known-failing.json`, the eight `ALLOWLIST`/`ALLOWED_LINES`/`KNOWN_GAPS` lists). Every one
  stores its baseline as a literal **inside the file it guards**, so a PR could raise the number in the
  same diff that violated it and pass. `scripts/ratchet-baselines.mjs` + the `ratchet-guard` CI job now
  measure every enumerated baseline against the merge base and **red on an increase**. Lowering always
  sails through; a raise stays possible but must be declared as a row in `docs/design/RATCHETS.md`
  naming the baseline, the exact old/new values, the ticket and the reason — and the raising diff must
  **add** that row (one **not already present at the base** revision). A row is a one-time licence,
  never a standing permit; rows are counted rather than looked up, so append a new one and leave the
  history alone — you never need to delete or edit a row to get past the guard. Keep every baseline a
  **plain literal** declared exactly once: an expression, a spread, a `.concat`, or a second
  declaration of the same name is refused rather than guessed at, because a measurer that returns the
  wrong number passes a raise, which is the whole defect.
  **Adding a new ratchet needs no wiring** — `src/lib/ratchetBaselines.test.ts` fails CI if a
  ratchet-shaped declaration appears in a file that is neither registered nor explicitly excluded with a
  reason. Full standard + the enumeration: [docs/design/RATCHETS.md](docs/design/RATCHETS.md).
  **That document's `today` column is asserted, not hand-kept (CPE-1948)** — it was a second,
  unguarded copy of every baseline, inside the page explaining why unguarded copies rot, and two of
  its twelve rows went stale within a day. `src/lib/ratchetsDoc.test.ts` parses the table
  *structurally* (header-anchored, whole-cell values, so no number can hide in a parenthetical or in
  surrounding prose) and asserts every row against the live measurer, plus the id list against
  `REGISTRY` in order — so a registered-but-undocumented baseline reds too. Update the table when a
  baseline legitimately moves; the test names the row and both values.
- **Enumerate, don't recall (CPE-1932).** Any guard over "all the X in this repo" derives its list at
  run time (`git ls-files`, a tree walk) and fails loudly when the list comes back near-empty — a
  hard-coded list of the instances someone remembered is how seventeen Cargo.lock files became two.
  Two jobs enforce this for the two lockfile families, and a third instance gets a third job rather
  than a note: `lockfile-preflight` (`cargo metadata --locked` over every `Cargo.lock`) and
  **`npm-audit-sweep`** (`scripts/audit-npm-projects.mjs`, `npm audit` over every `package-lock.json`).
- **Derive provenance, don't claim it (CPE-1933).** A comment asserting that code here reproduces
  something *there* — *"exactly the way `release.yml` invokes it"*, *"byte-identical to X"*,
  *"transcribed from Y"*, *"copied verbatim from Z"* — is **untested by construction**, and it is
  worse than no comment because the surrounding green test reads as vouching for it. CPE-1872's
  `release_guard.rs` carried three such claims; they were true for exactly one commit, then round 2
  moved the check and changed the arguments, and every test kept passing. So: **either read the
  referenced source at run time and assert against it, or do not make the claim.** Deriving is
  usually cheap — both sides are already data (`keymap.test.ts` joins `ACTIONS` to
  `SHORTCUT_GROUPS`), or the source is a file you can parse
  (`crates/updater-verify/tests/release_workflow_wiring.rs` reads both release workflows' argv and
  **executes** the real binary with it). Prioritise by blast radius: release plumbing and security
  guards earn a real derivation; a UI helper's claim is usually better simply deleted. If a claim is
  genuinely underivable, say **at the site** that it is unverified and why, so the next reader treats
  it as folklore rather than fact.
  **Three rules for writing one.**
  1. ***Enumerate case-insensitively.*** Comments start sentences — "Mirrors…", "Must match…",
     "Verbatim from…", "Exactly as…". CPE-1933's own first sweep ran `grep` without `-i` and missed
     **57 candidates in 56 files**, one of them a *fourth* copy of the very claim it was killing, in
     the same crate. A capital letter is not a hiding place; use `-i`.
  2. ***Anchor on code, never on prose.*** A scanner that finds "the first `format!(` after the fn",
     or "lines containing `gh release download`", will happily parse a **comment** that quotes the old
     value and pass **silently**. Do not hand-roll the stripper. For **shell/workflow** sources:
     `src/lib/shellScriptLines.ts` (TS) and `crates/updater-verify/src/workflow_scan.rs` (its Rust
     port, pinned to it by the shared `shellScriptLines.cases.json`) already handle quotes, escapes,
     trailing comments and heredoc bodies. For **Rust** sources: `src/lib/rustSource.ts`
     (`stripRustComments`, `rustStringLiteralAfter`, `rustStrSliceAfter`) — CPE-1950 lifted it out of
     `MacroRunConfirm.test.ts` rather than let a third scanner grow a fourth copy of the rules. For
     **JavaScript** sources: `src/lib/jsSource.mjs`, and **the entry point is
     `stripScriptBodiesChecked`, not the bare `stripJsComments`** — it runs `new vm.Script` over the
     result and throws when source that parsed before stripping does not after, which is the only
     leg that covers the shapes no case table names. Reach for `htmlScriptBodies` first when the
     input is HTML; call `stripJsComments` bare only when there is genuinely no parseable-JS baseline
     to compare against, and say at the call site why. CPE-1966 shipped the SIXTH private stripper,
     in a `.mjs` harness, imported nowhere and untested; a Reviewer's 31 adversarial shapes found 7
     wrong, **4 of them deleting real code** (that 31/7/4 tally is the Reviewer's round-2 count,
     recorded as provenance and never independently re-run — treat it as history, not as a measurement
     you can reproduce). A whole-line-comment filter is *not* enough — a **trailing** comment walks
     straight through it, which is how CPE-1933's first draft reintroduced the hole it was closing.
     Five JS-specific rules learned the hard way: decide regex-vs-division on the previous **token**,
     never the previous character (every keyword ends in a word char, so `return /[//]/;` reads as
     division and the `/` opens a comment that eats the line); decide a **`)`** by what its `(`
     opened, never by the `)` itself (round 3 fixed the keyword prefix, then *documented* `)` as a gap
     that "fails toward keeping source" — and `if (s.length) /[/*]/.test(s);` is valid JavaScript that
     the same mechanism deleted 144 characters of, one round later, with a green test beside the false
     claim); **a token-kind state must SURVIVE to the token that consumes it** — round 5's
     `for await (const x of y) /[//]/…` deleted 14 characters because `await` is the one word the
     grammar allows between a control word and its `(`, and it is also a regex-prefix keyword, so it
     overwrote the `"control"` state that the `)` was going to read; **account for every character a
     branch CONSUMES, not just the ones it emits** — a mis-read regex literal swallows the source's own
     parens (`{} / f(1 / 2)` scans as `/ f(1 /`), and a swallowed `(` that never reached the stack
     leaves the matching `)` popping the frame beneath it, another 14 characters (a real regex balances
     its parens, so pushing/popping the ones the literal eats is a no-op for the honest case and a fix
     for the dishonest one) — but **restoring the COUNT is only half of it: recover the KIND too**,
     because a swallowed region can hold a `)` and a `(` from different statements, and round 5's
     "push `false` for every swallowed `(`" broke four shapes round 4 got right; and **point a JS
     scanner at JS only** — `sessionChipColours` tokenized a
     whole HTML document, where one apostrophe in prose shifted the strip by 11,872 characters (a
     round-2 figure, **no longer reproducible**: round 3 made an unterminated string stop at the
     newline, so the same injection now shifts the strip by 0 — the rule stands on the mechanism, not
     the size).
     **The general lesson, which is the expensive one, and it cost two rounds because the fix looked
     like the lesson:** a *declared* gap is a claim like any other. Round 3 asserted "all of which fail
     toward keeping" over a list rather than deriving it per entry; round 4 split the list by
     direction, derived it per entry with `vm.Script` — and then wrote **"neither is reachable from
     valid JavaScript"** over the same list, which is the same defect one level up. A filter over a
     case table proves a fact about **the table**; it cannot prove a fact about the language, and next
     to a green run the generalisation reads as though it did. So: split a gap list by direction, pin
     the deleting shapes as their own cases, derive the property per entry — and then **state the
     assertion at the scope it was measured at.** `jsSource.test.ts` now says "no entry in this table
     parses before stripping", not "no valid JavaScript reaches this". The only leg that speaks for the
     shapes nobody enumerated is compiling the RESULT (`stripScriptBodiesChecked`).
     **And the same correction applies to a SWEEP, which is where round 6 caught it again.** Round 5
     reported "1,904 structured + 36,861 fuzzed inputs, 0 desyncs, no third family" and did not commit
     the generator; a reviewer wrote their own and found a third family in minutes. Two rules fell out.
     *(a) A negative over a generated space is a statement about the GENERATOR, never about the
     language* — say "no input this generator produces", and remember 38,765 samples missed a
     27-character input. *(b) If you cannot commit the generator, you have not measured anything a
     reviewer can check* — `scripts/dev-harness/js-strip-sweep.mjs` is now in the tree, with
     `--compare <ref>` so a change is scored as "N fixed, M regressed" instead of asserted to be an
     improvement. That comparison is what proved round 5's paren fix was 45-for-4 rather than free.
     Round 5's fuzz number was also inflated: its LCG lost precision in JS doubles and its 36,861
     "inputs" deduped to about 120 distinct programs.
     **A gap table earns its safety property; do not smuggle in an entry that breaks it.** When round
     6's third family was dropped into `DELETING_GAPS`, the `parses()` filter went red immediately —
     correctly, because that family DOES delete valid JavaScript. It got its own table
     (`DELETING_ON_VALID_JS`) asserting the worse fact out loud, plus that `stripScriptBodiesChecked`
     throws on it. A test file with more than one case table should also assert **which tables its
     oracles sweep**, so a family held back from the enumeration is visible rather than merely absent.
     **And that assertion must DERIVE its list of tables from the file's own source, not recall it**
     (CPE-1932 again, one scope in). Round 6 wrote the list by hand; round 7 ran the two sabotages and
     measured that it caught "registered the table, forgot the sweep" (1 red) and **missed** "declared
     a table and mentioned it nowhere" (65/65 green) — which is the exact half the assertion exists
     for. Scanning the file for `^const X: Case[] =` and requiring each name to appear closes the
     class; a decoy inside a comment reds, which is the safe direction.
     **Also: a claim about how many families exist is measured over whatever found them.** Round 6
     called `DELETING_ON_VALID_JS` "the honest third category" while the same commit shipped the
     generator that produces two more (`of` before a `/=`, and `yield`/`await` as plain identifiers in
     sloppy code — both pre-existing since round 3, both caught by `stripScriptBodiesChecked`). Run the
     committed generator and split its output before writing a number, and say which generator and
     which seed the number came from.
  3. ***Red-proof it.*** Change the referenced source and watch the test fail. A "derivation" that
     never actually re-reads its source is the same defect with extra steps. Write the red-proof's
     **result at the site**, not only in the PR body — a code comment that merely asserts, next to a
     PR body that argues, is how the claim gets re-established one review later.

  **A shared case file catches divergence, not shared blindness (CPE-1950).** Pinning two
  implementations to one oracle (`shellScriptLines.cases.json`, `platformConfigGuard.cases.json`)
  proves they agree; it cannot prove either is right. A shape nobody thought of is simply absent from
  the file, both sides answer it the same wrong way, and it passes green — measured on #1060, where a
  `<<` inside a quoted string opened a phantom heredoc in *both* scanners while their shared oracle
  agreed with itself. So pair every cross-language oracle with a leg that does **not** depend on
  anyone having written the case: read one side's own declaration out of the other's source
  (`sidecarBundleResources.test.ts` reads `TAURI_PLATFORM_TOKENS` out of the Rust guard), and say at
  the site what the oracle cannot catch. Better still, where the duplication is removable, remove it —
  CPE-1950 closed three of its seven by deleting the second copy rather than deriving it.

  Worked examples: `crates/updater-verify/tests/release_workflow_wiring.rs` (reads both release
  workflows' argv and **executes** the real binary with it), `src/lib/keymap.test.ts` (joins two data
  modules), `src/lib/components/MacroRunConfirm.test.ts` (walks a `format!` literal out of
  `fsutil.rs`, comments stripped first), `src/lib/channelPurityCoverage.test.ts`.
- **There are TWO npm projects (CPE-1945).** The root, and **`gui-smoke/`** — which has its own
  `package.json`, its own `package-lock.json`, its own advisories, and its own CI job. Any
  dependency/advisory statement must say which project it covers, or cover both: `gui-smoke/` went
  unaudited through every Dependency Steward pass because "run `npm audit`" was executed wherever the
  reader happened to be standing, and its root-only number was then quoted as the repo's. Never quote
  a single project's audit total as the repo's position — run `node scripts/audit-npm-projects.mjs`,
  which enumerates, sweeps all of them, and prints the per-project *and* summed totals.
- **`npm audit fix` is run WITHOUT `--force`, always.** `--force` accepts semver-majors, and npm's idea
  of a "fix" is frequently a **downgrade**. Measured in `gui-smoke/` (CPE-1945): `--force` walks
  `@wdio/local-runner` and `@wdio/cli` *backwards* 9.31.4 → 7.40.0 and `@wdio/mocha-framework` → 8.14.0,
  **rewrites `package.json`'s pins** to match, leaves an incoherent v7/v8/v9 mix, and takes the project
  from **15 advisories to 28**. It makes the number worse while regressing the harness that guards the
  whole GUI verification leg by two majors — a regression wearing a fix's clothing. A major bump is a
  migration decision and belongs in its own reviewed ticket.
- **Never treat "npm said nothing" as "nothing is wrong."** npm's `--json` **error** path emits
  well-formed JSON with no `metadata` key, so a parse-only check reads an unreachable registry as a
  clean audit. That shipped once in `scripts/audit-npm-projects.mjs` and printed "0 vulnerabilities
  across 2 npm projects", exit 0 — and with one lockfile corrupt it printed the *surviving* project's
  number as the repo-wide sum, re-emitting CPE-1945 from inside the guard built to prevent it. Any
  wrapper around an external tool must distinguish **"ran and found nothing"** from **"did not run"**,
  and fail closed on the latter. Related: npm's `fixAvailable: true` is optimistic and cannot be trusted
  as "there is something to do" — the sweep measures a real `npm audit fix --package-lock-only` instead
  of believing that flag.
- **Shadowed guards: two green sabotages on one guard mean it is unreachable (CPE-1929).** A guard
  cannot be given test coverage while an *earlier* guard answers on the same underlying fact — every
  input that would trip the later one trips the earlier one first, so it is **safe** and
  **unverifiable** at once, and those two are easy to mistake for each other. The tell is a **pair**:
  disabling the guard (`if false && …`) leaves the suite green, **and** forcing its predicate to lie
  changes no behaviour. Separately each reads as evidence of safety; together they mean *nothing can
  reach it*, and the next question is **which earlier check is shadowing it**. Run the pair by hand —
  do not reason about it, and do not trust a "Finished in 0.5s" cargo run on `/mnt/z` (touch the
  sources first). Measured instances: `batch_media::open_output_verified`'s handle-side reparse
  refusal (2,423 tests green with it disabled, no behaviour change with the predicate lying — the
  `symlink_metadata` path check in front of it reads the same Windows name-surrogate bit), and
  CPE-1896's leaf surrogate refusal before it. **The fix is reorder or delete — leaving it shadowed is
  the one wrong answer, because it reads as coverage.** Reorder when the later guard asks the more
  trustworthy question (a handle cannot be substituted after the open; a path can); delete when it is
  genuinely redundant. A guard kept **deliberately** as an unreachable backstop must say so at the
  site *and* say that it is untestable and why, so the next person's green sabotage is expected rather
  than alarming.
  **Not worth mechanising in full, and here is the honest reason:** the "disable it" half is
  automatable (mutation testing — `cargo-mutants` would flag exactly this as a surviving mutant), but
  the "force the predicate to lie" half needs a human to know *what a lie means* for that predicate,
  and the conclusion — *which* earlier check shadows it, and whether to reorder or delete — is not
  mechanisable at all. What IS cheap and is the actual ask: whenever you add or move a refusal, run
  the two sabotages once and write the numbers into the comment at the site. Every shadowed guard
  found so far was found that way, and none of them was found by reading the code.

## Docs

- **In-app docs are self-maintaining (CPE-579).** Every feature that adds a user-facing **section** must
  (a) ship/update its page in `src/docs/*.md`, and (b) add its `section → doc slug` entry in
  `src/lib/sectionDocs.ts` (the one source of truth). The guard test `src/lib/sectionDocs.test.ts` asserts
  every `Section` is mapped and every mapped slug exists in `DOCS`, so a new section without its doc — or a
  typo'd slug — **fails CI**. Contextual help (the toolbar "?" / F1) opens the current section's page via
  that registry; `DocsView` takes an optional `initialSlug`. See [[maintain-in-app-docs-library]].
- Tauri v2: https://v2.tauri.app
- Updater plugin: https://v2.tauri.app/plugin/updater/
- tauri-action: https://github.com/tauri-apps/tauri-action
- Menu design standard: [docs/design/MENUS.md](docs/design/MENUS.md)

## Managing this project — two surfaces

This repo is managed from **both** the Claude Code CLI and the Claude desktop (Cowork) app.
Both operate on the same files, so either can be used interchangeably.

### CLI (Claude Code)

Launch it by double-clicking **`RunClaude.cmd`** in the repo root (or run `claude` in this
directory). That starts a Claude Code session scoped to this repo with the slash commands in
`.claude/commands/` available:

| Command | Purpose |
|---------|---------|
| `/ticketing-list` | List the open ticket queue with an action menu |
| `/ticketing-new` | File a ticket interactively (auto-intercepts units of work; routes epics to the Epics queue) |
| `/ticketing-work CPE-NNN` | Pick up and work a ticket through to Done (redirects epics to `/ticketing-epic`) |
| `/ticketing-epic` | Manage epics — `list` / `activate CPE-NNN` / `close CPE-NNN`; decomposes an epic just-in-time |
| `/ticketing-sprint` | Manage sprints (time-boxed ticket batches) — `list` / `new` / `activate` / `close` / `assign CPE-NNN` |
| `/ticketing-organize` | Reorganise `Done/` when it grows large |
| `/ticketing-setup` | (Re)bootstrap the ticket system |
| `/skills-organise` | Manage the slash commands as named feature sets |
| `/run` | Publish the latest release (if draft), then install and launch it |
| `/remove` | Uninstall the application from this machine |

### Trigger words: "Run" and "Remove"

When the user says **"Run"**, execute `.claude/commands/run.md`:

1. Find the **latest** release, drafts included.
2. If it is still a draft, **publish it first** (`gh release edit <tag> --draft=false`) — but only
   after confirming the draft actually carries installer assets. A draft with no assets means the
   release build failed or is still running; publishing it would create an empty public release.
   In that case stop and report, rather than publishing.
3. Download the right installer for the current OS, install silently, verify the install, launch.

If **no release exists at all**, `/run` stops and says so — it never installs nothing and calls it
success.

When the user says **"Remove"**, execute `.claude/commands/remove.md` — close the app, uninstall it
silently, and verify it is gone. "Remove" means uninstall the **installed application**, never the
source repo or the user's files; if that is ambiguous in context, ask first.

Both commands act on the built app — they never touch this working tree.

`RunClaude.cmd` passes `--dangerously-skip-permissions` for an uninterrupted local session; it is
path-independent (`%~dp0`) so it works wherever the repo lives.

### Desktop (Cowork)

The desktop app manages releases and monitoring:

- **`RELEASING.md`** — runbook; say "cut a release 0.2.0", "check the build", "what needs updating".
- **`scripts/release.ps1`** — one-command version bump + tag + push.
- **`STATUS.html`** — local dashboard (gitignored), refreshed by a scheduled task.
- **Scheduled tasks** — `cpe-daily-status` (CI + dashboard refresh + notify) and
  `cpe-weekly-deps` (dependency scan).

### Using both together

The ticket system (`Ticketing/`, `.claude/commands/`) is committed to git, so tickets filed from the
CLI are visible on the desktop and vice-versa. Release/monitoring lives on the desktop; day-to-day
coding and ticket work happens in the CLI. Nothing is surface-specific except the desktop-only
scheduled tasks and the `gh`-driven release helpers (which also work from a CLI PowerShell session).

## Ticket System

The ticket system lives under the `Ticketing/` container: the status-flow queue in `Ticketing/Tickets/`,
plus the sibling `Epics/` and `Sprints/` queues. Folder location is the authoritative status:

`Ticketing/Tickets/` and `Ticketing/Epics/` have the **same five status folders** (CPE-1676), so both
queues read the same way; `Ticketing/Sprints/` is flat.

| Folder | Tickets queue | Epics queue (same folders, epic vocabulary) |
|--------|---------------|---------------------------------------------|
| `Backlog/`  | Open — ready to work | `Proposed` — dormant brief, not yet decomposed |
| `Doing/`    | In Progress — one at a time | `In Progress` — activated; children in `Tickets/Backlog/` (several epics may be active) |
| `Blocked/`  | Deferred on an **external** gate — not workable until it clears | same, for an epic (normally empty) |
| `Deferred/` | Postponed by **our** choice / an internal prereq — pickable anytime | same, for an epic (normally empty) |
| `Done/`     | Closed — dated `YYYY/QN/Month/Week-NN/` nesting via `/ticketing-organize` | Closed — **flat**, never nested (~70 epics total) |

`Ticketing/Sprints/` — time-boxed ticket batches, a **separate queue** (`SPR-NN`; `Planned` /
`Active` / `Closed`); orthogonal to epics, managed via `/ticketing-sprint`.

IDs are sequential: `CPE-NNN`. To work a ticket: `/ticketing-work CPE-NNN`. To file one
interactively: `/ticketing-new`. See `Ticketing/wiki.md` for full workflow rules.

**Epics** are handled specially: they live in `Ticketing/Epics/**` and are **not** researched, planned,
or sub-ticketed until *activated* with `/ticketing-epic activate CPE-NNN` (which `git mv`s the file
`Epics/Backlog/ → Epics/Doing/`). A dormant epic is just a brief; `/ticketing-work` never builds one
directly. See `Ticketing/wiki.md` → "Epics" and the `ticketing-epic` skill.

**Folder location is authoritative in both queues**, mirrored in each file's `status:`. For the Epics
queue that invariant is enforced by `src/lib/epicsQueueLayout.test.ts`: it fails CI if any `.md` sits
loose in `Ticketing/Epics/` (the pre-CPE-1676 flat shape) or if a file's `status:` disagrees with its
folder. Both board implementations (`crates/server` + `src-tauri`, and the `sidecar/agent-board`
sidecar) and the ticket MCP read these folders directly, so drift makes them **lie** rather than error
— change every reader in lockstep.

### Showing open tickets — ALWAYS include Blocked, Deferred, Epics, and Sprints

When the user asks to see "open tickets", "the tickets", "tasks", or "all tickets", ALWAYS show the
Backlog table **plus** the Blocked, Deferred, **Epics**, and **Sprints** tables — never just the
Backlog. (User preference, stated 2026-07-16: ticket listings must always surface **epics and
sprints**.):

1. **Open** — all `Ticketing/Tickets/Backlog/CPE-*.md`, as a table of ID, title, type, priority, tags, estimate.
   `tags` is the ticket's disposition (`ready`, `big-design`, `resource-blocked` + qualifier, etc.);
   the controlled vocabulary lives in `Ticketing/wiki.md` ("Disposition Tags").
2. **Blocked** — all `Ticketing/Tickets/Blocked/CPE-*.md`, as a table of ID, title, tags, and a one-line
   *blocked-on / unblocks-when* note read from the ticket's Notes or Work Log.
3. **Deferred** — all `Ticketing/Tickets/Deferred/CPE-*.md`, as a table of ID, title, tags, and a one-line
   *deferred-on / revisit-when* note. These are postponed by our choice (often an internal prereq),
   not externally gated, so they remain pickable.
4. **Epics** — all `Ticketing/Epics/*/CPE-*.md` (skipping each folder's `wiki.md`), as a table of ID,
   title, status, tags, and a one-line goal (plus `X of Y children Done` for an activated epic). The
   **folder gives the status** — `Backlog/`→`Proposed`, `Doing/`→`In Progress`, plus `Blocked/`,
   `Deferred/`, `Done/`. List the open ones (Backlog/Doing/Blocked/Deferred); `Epics/Done/` is history,
   surfaced only on request. This is the separate epic queue; epics are decomposed via
   `/ticketing-epic`, not worked by `/ticketing-work`.
5. **Sprints** — all `Ticketing/Sprints/SPR-*.md`, **Active first then Planned**, as a table of ID, title,
   status (`Active`/`Planned`), window (`start → end`), a one-line goal, and progress (`X of Y tickets
   Done`, counting tickets whose `sprint:` frontmatter names it). This is the separate, time-boxed sprint
   queue; sprints are managed via `/ticketing-sprint`, not worked directly. Orthogonal to epics — a
   ticket can appear in both.

Blocked, Deferred, Epic, and Sprint tickets are all outstanding work, so omitting them misrepresents the
queue. If a section is empty, say "none blocked" / "none deferred" / "no epics" / "no sprints" rather
than dropping it. Also surface anything sitting in `Ticketing/Tickets/Doing/` so stalled work-in-progress is never
silently lost.
