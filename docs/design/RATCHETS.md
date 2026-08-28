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
2. **Raising is still possible — never quiet.** A raise passes only if **this diff ADDS** a row to the
   ledger below naming the baseline, the *exact* old and new values, the owning ticket, and why — a
   row **not already present at the base revision**. A row is a **one-time** licence for the raise made
   in its own diff, not a standing permit: otherwise burning a baseline back down and re-raising it
   later would pass silently under someone else's ticket. A row that doesn't match the actual movement
   authorises nothing.

   Rows are **counted, not looked up**, so the same `from → to` can legitimately happen more than once
   over the repo's life — append a new row and leave the historical one in place. You should never need
   to delete or edit an existing row to get past this guard; if you think you do, that is a bug here.
3. **A guard that cannot measure goes red, not green — and never a guessed number.** This is the one
   the first implementation got wrong three ways, so it is worth stating as a rule rather than a hope:
   a measurer that returns the *wrong* value passes a raise, which is the whole defect. Concretely, all
   of these fail the job rather than producing a number:
   - a baseline constant that stops being a bare integer (`= 200 + 78`, `= Number("278")`);
   - **two declarations of the same constant** — a decoy in a `/* … */` block or a template literal
     used to outrank the live one, because the search ran on raw source and took the first match.
     Comments and string interiors are now masked *before* the search, and more than one match is a
     red in itself: which one is live becomes a question that cannot arise;
   - an allowlist that spreads another list into itself (`[...MORE_GAPS, "x"]`), or whose literal isn't
     the whole initialiser (`[...].concat(MORE)`);
   - a constant that was renamed, or a file that was deleted;
   - a baseline with no value **at the base revision**: git's own rename detection is followed first,
     and anything still unresolved must be declared as `| id | new -> N | CPE-NNNN | why |`. Head-side
     and base-side unmeasurable are treated identically — the asymmetry (head red, base green) is
     exactly how a rename could reset a ratchet;
   - an unresolvable base revision, or no base revision at all.

   "0 of 0 checked" is the CPE-1932 anti-pattern; so is "measured something, just not the truth".

## Adding a new ratchet — you get the guard for free

`src/lib/ratchetBaselines.test.ts` scans `src/`, `gui-smoke/` and `scripts/` for ratchet-shaped
*declarations* — `const *X*` where X is any of ALLOWLIST, ALLOW_LIST, ALLOWED_LINES, ALLOWED_,
KNOWN_GAPS, KNOWN_FAILING, BASELINE, OFFENDER, SUPPRESS, TOLERAT, WAIVER, WAIVED, OPTOUT, OPT_OUT,
EXEMPT, EXCLUD, DEBT, GRANDFATHER, LEGACY_, REGISTRY, CEILING, THRESHOLD, PENDING, EXISTING — and
fails if a matching file is neither in `REGISTRY` nor in `NOT_A_RATCHET` with a stated reason. So a
new ratchet cannot land without either being gated or saying out loud why it doesn't need to be.

The scan's non-vacuity check is derived from the registry, not a magic floor: it asserts the scan
still matches **every registered baseline file** and every file the exclusion list names, so a broken
regex or a broken tree-walk reds instead of reporting a comfortable count of nothing.

To register one, add a `REGISTRY` entry in `scripts/ratchet-baselines.mjs` with a stable `id`, the
file, one line saying what the number counts, and a `measure` — one of `numericConst`,
`arrayLength`, `recordOfArraysTotal`, `jsonArrayLength`, or a small function of your own. Keep the
baseline a **plain literal**: a bare integer, or an array/object literal with its entries written out.
Anything the scanner cannot count exactly (an expression, a spread, a `.concat`) is refused by design.

A brand-new baseline has no value at the base revision, so its first commit declares it:
`| <id> | new -> <N> | CPE-NNNN | why this guard is landing here |`.

## The enumeration

Found 2026-08-27 by four independent sweeps (the literal word `ratchet`; `toBeLessThanOrEqual`
assertions; declaration-shaped `const *ALLOW|EXEMPT|KNOWN|LEGACY|WAIV|BASELINE*` names across TS and
Rust; and prose markers like "only ever shrink" / "one-way"), **not** from memory — CPE-1932 lost an
hour to a rule followed from recall when seventeen instances existed.

**The `today` column is asserted, not maintained (CPE-1948).** `src/lib/ratchetsDoc.test.ts` parses
this table structurally — the header row above pins it, and each `today` cell must be a bare integer
(optionally plus the not-gated marker, which is itself derived from `unenforced`) so nothing here can
be a number that went unchecked. It then measures every baseline in `REGISTRY` and fails naming the
row, the stored value and the measured one. The `id` list is compared to the registry as an ordered
whole, so a baseline added to `scripts/ratchet-baselines.mjs` and not to this table is also a red.
Update the table when a baseline legitimately moves; the test will tell you it needs it.

Why this table keeps its numbers at all, rather than deleting them and pointing at
`node scripts/ratchet-baselines.mjs print`: the numbers are the reason anyone opens this page — the
*scale* of each debt is what tells you whether an allowlist is a rounding error or a project. A page
without them is honest and useless. Asserting them costs one guard test, which is the trade this repo
already makes for `sectionDocs.test.ts` and `keymap.test.ts`.

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
| `bidi-render-registry` | `src/lib/bidiEscape.guard.test.ts` | component render sites showing a raw filesystem name (CPE-1757/1885) | 1553 |
| `bidi-app-markup-offenders` | `src/lib/bidiEscape.guard.test.ts` | the same, in `App.svelte`'s markup | 31 |
| `bidi-app-script-basename-allowlist` | `src/lib/bidiEscape.guard.test.ts` | `App.svelte` `<script>` `baseName()` calls skipping `displaySafeName` | 2 |
| `manual-test-mvd` | `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` | still-manual verification surfaces (MVD) | 14 — **enumerated, not gated** |

`manual-test-mvd` is enumerated but deliberately **not** gated: the MVD legitimately *rises* whenever
a QA-Architect audit discovers pre-existing unlogged debt (the burndown's own ledger records a +5
shift on 2026-08-11), and that discovery is the behaviour we want. Gating it would push audits toward
not logging what they find. Ungated is not unwatched, though — its `today` value is asserted here like
every other row, which matters most for this one, because being ungated is exactly why it was the
first row to go stale (CPE-1922, then CPE-1946).

### Recount, 2026-08-27 (CPE-1934)

Every baseline was recomputed from scratch rather than trusted, to catch a raise that already
happened quietly. **None was inflated.**

- `hex-files` / `hex-occurrences`: baselines temporarily set to `0` and the ratchet re-run, so its own
  matcher reported the truth. Exactly the stored values, zero slack. (The figures are deliberately not
  repeated here — the table above is the one asserted site, and a second copy in prose is the defect
  CPE-1948 was filed about.)
- The eight allowlists: each carries its own "no stale entries" test, and all eight are green — every
  entry still points at debt that is really there. `bidi-render-registry` asserts exact equality with
  the tree, so it cannot be inflated by construction.
- `gui-smoke-known-failing` cannot be recounted off-CI (it needs a built app on the Linux leg), but
  the ratchet's clause 2 fails the job the moment a listed case starts passing, so an entry that is no
  longer needed reds every run rather than sitting there.

### Recount, 2026-08-27, later the same day (CPE-1948)

Recounted again from `REGISTRY` rather than from the table, because the table being wrong is the
premise of CPE-1948 and it therefore cannot also be the list of what to recount. **One row was
already stale**: `bidi-render-registry` read `1552` against a measured `1553`. Every other row
matched, `manual-test-mvd` included.

The movement was real and legitimate — PR #1056 (CPE-1928) recorded one new render site,
`text:blockedRemedy` in `MacroRunConfirm.svelte`. Two things are worth writing down about how it got
past the guard, because neither is a bug in the guard:

- **The `ratchet-guard` job never saw it.** The decisive fact is a content check, not a timestamp one:
  `ratchet-guard` is absent from `ci.yml` at #1056's head SHA — grep 0 — so no run of that PR could
  have judged it, however recent the run. A merge whose checks predate a guard is a merge that guard
  did not make. Nothing here can fix that; branch protection with required, up-to-date checks can.
  Filed as **CPE-1970**, which carries the measurement and is where this belongs rather than here.
- **The doc had no such excuse.** Nothing was watching it at all, which is what this ticket changed.

## What this guard does *not* catch

Written down here rather than only in a PR body, because a limitation nobody can find is
indistinguishable from a bug.

**Regex literals are not tracked by the source masker.** Telling `/` division from a regex needs real
parsing. Getting this wrong is the *safe* direction, which is why it is documented rather than fixed:
an unmasked regex can only ADD apparent entries (`["a", /x,y/, "b"]` counts 4, not 3), which
over-reports debt and therefore fails closed on a raise; and a quote inside a regex masks at most to
the end of its own line, yielding a "no declaration found" red rather than a wrong number. No
registered baseline contains a regex today. Stated plainly because a hand-rolled character scanner
over JS is the shape that produced several bugs in this repo in one week, every one found by
adversarial input rather than by reading — so assume the next one is there and probe for it.

**It measures counts, not identities.** A diff that removes one offender and adds a different one
leaves the count flat and passes. Catching that needs per-entry identity diffing — which the
exact-equality guards (`bidi-render-registry`, `bidi-app-markup-offenders`, and every allowlist's own
"no stale entries" test) already do within their own domains, and which would be a much larger change
here. Accepted as out of scope for CPE-1934.

## Raise ledger

A row here is what makes a raise legal. Two conditions, both required:

- `from`/`to` must match the actual movement **exactly**;
- the row must be **new in the diff that raises the baseline** — a row already present at the base
  revision is a spent licence and authorises nothing.

`from` is an integer, or `new` when the baseline has no value at the base revision at all (a
brand-new guard, or a rename git could not follow).

**Why this table is still empty when the recount above records a `1552 → 1553` movement.** A row
here is a **licence**, not a history entry. The two conditions directly above make a row meaningful
only inside the diff that performs the raise, and rows are counted against the base revision rather
than looked up — so a row added after the fact satisfies neither condition: it licenses a movement
that has already merged and can therefore never be consumed, and it would be the only row on the page
that is false by the page's own definition of a row. It would also cost the reading that makes an
empty ledger worth anything — *no raise got past the guard* — because once rows can appear
retroactively, their absence stops meaning that.

The `bidi-render-registry` movement is not unrecorded; it is recorded where it belongs, in the recount
above. PR #1056 (CPE-1928) added a real render site and never declared it, because `ratchet-guard`
did not yet exist in `ci.yml` at that PR's head — a stale-checks merge, filed as **CPE-1970**. The
fix for it is requiring up-to-date checks before merge, not backdating a licence here.

| baseline | from → to | ticket | why this raise is right |
|----------|-----------|--------|-------------------------|
| bidi-render-registry | 1553 → 1555 | CPE-1925 | Two new render sites in `BackupDashboard.svelte`: `plan.createDirs.length` and `plan.skippedDirs.length`. Both are `.length` **numbers** — no filesystem-derived text can reach either — and they exist to make the backup plan's own counts honest, which is the whole ticket: a run that recreates folders, or that declines to carry ones it could not read, previously showed neither. The one value in that block that IS a path, `sd.path`, goes through `displaySafePath` and therefore does not appear in the registry at all; the wording that could have lived in three more ternaries was moved into the surrounding static text instead, to keep this raise at two rather than seven. |
