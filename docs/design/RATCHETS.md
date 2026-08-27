# Ratchets — and the guard that guards them

A **ratchet** in this repo is a stored number (or a stored allowlist, which is a number wearing a
coat) that a test compares today's measurement against, to stop a defect class from *growing* while
the existing instances are burnt down. `src/app.css.test.ts`'s hard-coded-hex count is the
archetype; `gui-smoke/known-failing.json` is the same idea over test cases.

Every one of them shares a structural weakness, which is what CPE-1934 fixes:

> The baseline is a plain literal **inside the file it guards**. A PR that adds a new offender *and*
> edits the literal upward in the same diff passes trivially. The ratchet cannot see its own baseline
> move — the only backstop was a reviewer noticing that a number went up.

That is precisely the move a one-way ratchet exists to prevent, so "a reviewer will spot it" is not a
mechanism. Two things observed in one night (2026-08-27) made it weaker still: a failure message that
named only a number made editing one digit the path of least resistance (fixed in CPE-1931), and the
same ratchet had just produced two *false* positives (CPE-1931) — a guard with a history of crying
wolf trains people to reach for the baseline.

## The mechanism

`scripts/ratchet-baselines.mjs` measures every enumerated baseline **in the working tree** and **at
the merge base**, and fails CI when one increased. The `ratchet-guard` job in `.github/workflows/ci.yml`
runs it on every push and PR; it needs no `npm ci` and no toolchain (plain `node`, zero dependencies),
so it costs a checkout plus a few hundred milliseconds.

Three properties, deliberately:

1. **Lowering always sails through.** Fixing debt must never need paperwork.
2. **Raising is still possible — never quiet.** A raise passes only if the same diff adds a row to
   the ledger below naming the baseline, the *exact* old and new values, the owning ticket, and why.
   A row that doesn't match the actual movement authorises nothing.
3. **A guard that cannot measure goes red, not green.** An unresolvable base revision, a missing file,
   or a literal the scanner can't parse fails the job. "0 of 0 checked" is the CPE-1932 anti-pattern.

## Adding a new ratchet — you get the guard for free

`src/lib/ratchetBaselines.test.ts` scans the tree for ratchet-shaped declarations
(`const *ALLOWLIST*`, `*ALLOWED_LINES*`, `*KNOWN_GAPS*`, `*KNOWN_FAILING*`, `*BASELINE*`) and fails
if a matching file is neither in `REGISTRY` nor in `NOT_A_RATCHET` with a stated reason. So a new
ratchet cannot land without either being gated or saying out loud why it doesn't need to be.

To register one, add a `REGISTRY` entry in `scripts/ratchet-baselines.mjs` with a stable `id`, the
file, one line saying what the number counts, and a `measure` — one of `numericConst`,
`arrayLength`, `recordOfArraysTotal`, `jsonArrayLength`, or a small function of your own.

## The enumeration

Found 2026-08-27 by four independent sweeps (the literal word `ratchet`; `toBeLessThanOrEqual`
assertions; declaration-shaped `const *ALLOW|EXEMPT|KNOWN|LEGACY|WAIV|BASELINE*` names across TS and
Rust; and prose markers like "only ever shrink" / "one-way"), **not** from memory — CPE-1932 lost an
hour to a rule followed from recall when seventeen instances existed.

| id | file | what the number counts | today |
|----|------|------------------------|-------|
| `hex-files` | `src/app.css.test.ts` | `.svelte` files with a hard-coded hex in a style position (CPE-1534) | 85 |
| `hex-occurrences` | `src/app.css.test.ts` | total such hex occurrences (CPE-1534) | 277 |
| `gui-smoke-known-failing` | `gui-smoke/known-failing.json` | GUI smoke cases allowed to fail (CPE-1594/1677) | 25 |
| `docs-known-gaps` | `src/docs.coverage.test.ts` | shipped surfaces with no docs yet (CPE-1571) | 14 |
| `warn-token-allowlist` | `src/app.css.warn-token.test.ts` | tokens allowed not to resolve to a hex (CPE-1875) | 0 |
| `invoke-optout-allowlist` | `src/lib/invoke.guard.test.ts` | modules bypassing the busy-cursor `invoke` wrapper | 0 |
| `mojibake-allowlist` | `src/lib/mojibakeGuard.test.ts` | lines allowed to look like mojibake (CPE-1723) | 5 |
| `pwsh-encoding-allowed-lines` | `src/lib/workflowPwshFileEncoding.test.ts` | workflow pwsh writes with no explicit encoding (CPE-1842) | 1 |
| `bidi-render-registry` | `src/lib/bidiEscape.guard.test.ts` | component render sites showing a raw filesystem name (CPE-1757/1885) | 1552 |
| `bidi-app-markup-offenders` | `src/lib/bidiEscape.guard.test.ts` | the same, in `App.svelte`'s markup | 31 |
| `bidi-app-script-basename-allowlist` | `src/lib/bidiEscape.guard.test.ts` | `App.svelte` `<script>` `baseName()` calls skipping `displaySafeName` | 2 |
| `manual-test-mvd` | `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` | still-manual verification surfaces (MVD) | 16 — **enumerated, not gated** |

`manual-test-mvd` is enumerated but deliberately **not** gated: the MVD legitimately *rises* whenever
a QA-Architect audit discovers pre-existing unlogged debt (the ledger records a +5 shift on
2026-08-11), and that discovery is the behaviour we want. Gating it would push audits toward not
logging what they find. Its stored-vs-real drift is CPE-1922's.

### Recount, 2026-08-27 (CPE-1934)

Every baseline was recomputed from scratch rather than trusted, to catch a raise that already
happened quietly. **None was inflated.**

- `hex-files` / `hex-occurrences`: baselines temporarily set to `0` and the ratchet re-run, so its own
  matcher reported the truth — 85 files, 277 occurrences. Exactly the stored values, zero slack.
- The eight allowlists: each carries its own "no stale entries" test, and all eight are green — every
  entry still points at debt that is really there. `bidi-render-registry` asserts exact equality with
  the tree, so it cannot be inflated by construction.
- `gui-smoke-known-failing` cannot be recounted off-CI (it needs a built app on the Linux leg), but
  the ratchet's clause 2 fails the job the moment a listed case starts passing, so an entry that is no
  longer needed reds every run rather than sitting there.

## Raise ledger

A row here is what makes a raise legal. `from`/`to` must match the movement exactly, or the guard
still fails.

| baseline | from → to | ticket | why this raise is right |
|----------|-----------|--------|-------------------------|
| _(none yet)_ | | | |
