# QA Architecture — drive manual testing to zero

This folder is the substrate for the Workshift's **QA Architect** (see `.claude/commands/workshift.md`).

**The mission, stated by the user:** *"I would prefer to never test anything manually. The QA Architect
should eliminate my manual testing over time."* So this is not a per-ticket tester — it is a **standing,
compounding role** whose single job is to make the app **more automatically testable every shift**, until
the human's manual-verification burden reaches **zero**.

## The north-star metric — Manual Verification Debt (MVD)

**MVD = the number of app aspects that still require a human to look/click/confirm.** The QA Architect
drives MVD **monotonically downward**. Every shift it must not *rise* without being logged, and over time
it must *fall*. The burndown registry below is the authoritative MVD ledger; the wrap report surfaces the
current number so the user watches it drop.

Two hard rules give the ratchet teeth:

1. **New manual step = new debt, logged immediately.** Any change that ships with "verify this by hand", or
   any UAT that had to be *skipped-and-noted* for a user resource (interactive cross-OS GUI check, a Mac,
   credentials — escalation #2 in the workshift), **becomes a burndown row** the same shift. Debt is never
   silently absorbed.
2. **Automated stays automated (no regression).** Once a surface is automated, a CI job / guard test
   **pins** it. A surface can leave the "still-manual" column exactly once; it may never quietly return.

## Files

| Path | Committed? | What it is |
|------|-----------|------------|
| `README.md` | committed | This charter. |
| `MANUAL-TEST-BURNDOWN.md` | committed | The MVD ledger — every human-verified surface, its current automation, and the ticket retiring it. The list the QA Architect burns down. |

Cross-reference: `docs/MANUAL-TESTS.md` is the **user-facing runbook** ("how to hand-test X today"). Every
procedure that lives there is, by definition, an MVD item — it should have a matching burndown row aimed at
deleting the need for that procedure.

## How the QA Architect works each shift

1. **Audit.** Read the burndown + any UAT skip-and-note escalations from this shift + `git diff` of what
   shipped. Add rows for new manual debt; confirm no automated surface regressed.
2. **Prioritise by leverage.** Pick the manual surface whose automation removes the **most** future human
   testing (breadth × frequency × how often it currently blocks a shift). The GUI end-to-end gap is
   usually the top prize because it underlies most skip-to-user escalations.
3. **Design the automation, then file a `CPE-NNN` ticket** for a Worker to build it (the QA Architect
   architects; Workers implement — same split as PM/Foreman). Bias to the cheapest technique that fully
   retires the manual step.
4. **Ratchet + verify.** When the automation lands and is green in CI, flip the burndown row to
   *automated*, note the pinning job, and drop MVD by one.
5. **Report the number.** MVD and its delta this shift go in the wrap.

## The automation toolbox (what to reach for, cheapest first)

Grounded in what the repo already runs (extend these before inventing new stacks):

- **Already automated — keep extending:** Rust unit/integration tests across `crates/*` (`cargo test`);
  the large frontend **vitest + jsdom** suite (`src/**/*.test.ts`, incl. the launcher harness); `clippy
  --all-targets` both feature modes; the **3-OS backend CI matrix**; guard tests (e.g. `sectionDocs`).
- **Runnable `cargo run --example` end-to-end demos** — already the pattern for headless backend features
  (`docs/MANUAL-TESTS.md`). Convert each demo's "verify it yourself with OS tools" step into an
  **assertion the example makes itself**, so CI runs it instead of the user.
- **Headless GUI driving** — the biggest lever. Stand up **`tauri-driver` + WebDriver** (WebView2 via Edge
  WebDriver on Windows, WebKitWebDriver on Linux) so the *real* built app can be clicked and asserted in
  CI. Alternative/complement: Playwright against the webview.
  - **Off-screen + test-mode launch convention (CPE-1046):** automated GUI runs launch the real app with
    `--test-mode --x -4000 <other flags> --open <tmpdir>` (the geometry flags are CPE-600, `--open` is
    CPE-1043). `--x -4000` moves the window off the interactive monitor — WebDriver drives the DOM
    regardless of window position, so the automation works fully while nothing ever appears in front of
    the user. `--test-mode` additionally (a) renders a small, upper-left "🤖 automated test — don't
    touch" badge (`TestModeOverlay.svelte`, `pointer-events: none`, never a full-window frame) for the
    rarer case where a test window *is* on-screen (e.g. a manual reproduction of a CI run), and (b)
    launches the window itself unfocused (`.focused(false)` in `lib.rs`'s `setup()`) so it never steals
    OS-level mouse/keyboard focus from whatever the user is actively doing. Both flags are launch-only
    and cost nothing when absent.
  - **Faithful mouse input — use `gui-smoke/lib/mouse.ts` (CDP, NON-grabbing) — CPE-1155.** For any
    mouse behaviour (click / right-click-context-menu / hover / scroll / drag), drive it with
    `mouse.ts`, which injects via the Chrome/Edge DevTools Protocol (`Input.dispatchMouseEvent` /
    `Input.dispatchMouseWheelEvent`). CDP goes through the **real** input pipeline (true hit-testing,
    native context menu, real event order) yet **never moves the OS cursor** — so mouse tests run in the
    background while the user keeps working, and the tauri-driver window stays unfocused. **Do NOT** use
    `browser.action('pointer')` (grabs/hijacks input — violates the off-screen convention above) and
    **do NOT** use a bare `el.dispatchEvent(new MouseEvent(...))` (unfaithful — bypasses hit-testing;
    that blind spot is exactly how the CPE-1154 native-menu leak and then CPE-1157 slipped past a
    "passing" synthetic check). Verified on Windows (Edge/WebView2 150, msedgedriver 150, classic
    WebDriver against wry): CDP injection reaches the wry webview and the physical cursor provably does
    not move. See `gui-smoke/README.md` → "Faithful mouse input".
- **Visual / theme regression** — screenshot-diff the GUI (light/dark, menus per `docs/design/MENUS.md`,
  tabs per `TABS.md`) so appearance rules are checked without human eyes.
- **Smoke-install job** — a CI job that installs the **sidecar build** artifact, launches it, and asserts
  it responds — automating the human `build → deploy → run` verification loop.
- **Cross-OS runners** — macOS/Linux CI runners for GUI + OS-interop surfaces (Finder tag byte-interop,
  xattr/ADS) that currently need the user's other machines.
- **Property-based / fuzz** (`proptest`/`arbitrary`) for parsers and codecs; **golden/snapshot** for
  stable outputs; **contract tests** for the `cpe-contract` envelope + `cpe-net` wire.
- **Updater + release** end-to-end — an automated pass over the auto-update flow so releases aren't
  hand-verified.

## Relationship to the other QA roles

- **Reviewer** scrutinises *a PR's code*. **UAT Tester** exercises *this ticket's feature* (and stands in
  for the user per-ticket). **QA Architect** improves *the testing system itself* so both of them — and the
  human — have less to do by hand next time. When the UAT Tester has to skip-and-note a manual GUI check,
  that skip is the QA Architect's raw material.

## Ledger note

Log a `qa-architect` role row in `.claude/workshift-metrics/ledger.jsonl` when it runs; its payoff shows
up indirectly as **fewer `skipped` (user-resource) UAT rows over successive shifts** — that decline *is*
the MVD burndown, measured.
