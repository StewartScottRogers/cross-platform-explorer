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

**4 and 5 are the ones that get missed**, because nothing fails when they drift: neither build passes
`--locked`, so both lockfiles are silently rewritten at build time and the stale version never surfaces as
an error. It surfaces instead as a **dirty working tree** the moment anyone runs `npm install` or a local
`cargo build` — which reads as unrelated noise and gets committed by accident or discarded along with real
work. Observed 2026-08-20: `package-lock.json` had been three releases behind (`0.57.64` vs `0.57.67`).

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
  Never the reverse — white on `--accent-text` is 3.53:1 (dark), under the UI floor.
  `src/app.css.accent-text-contrast.test.ts` pins both directions, derives the JSON preview's colour
  roles from the component CSS rather than a list, and derives the **painted** surfaces
  (`.preview-pane`'s background, `.jt-row:hover`'s fill) instead of assuming `--bg`.
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
     value and pass **silently**. Do not hand-roll the stripper: `src/lib/shellScriptLines.ts` (TS) and
     `crates/updater-verify/src/workflow_scan.rs` (its Rust port, pinned to it by the shared
     `shellScriptLines.cases.json`) already handle quotes, escapes, trailing comments and heredoc
     bodies. A whole-line-comment filter is *not* enough — a **trailing** comment walks straight
     through it, which is how CPE-1933's first draft reintroduced the hole it was closing.
  3. ***Red-proof it.*** Change the referenced source and watch the test fail. A "derivation" that
     never actually re-reads its source is the same defect with extra steps.

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
