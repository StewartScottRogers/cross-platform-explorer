---
id: CPE-1945
title: `gui-smoke/` is a second npm project that no Dependency Steward pass has ever audited — 17 advisories, 5 non-major fixable, including the same `brace-expansion` high the root just fixed
type: task
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

This repo has **two** npm projects. `git ls-files '*package-lock.json'` returns exactly two: the root,
and **`gui-smoke/`**. Every Dependency Steward pass to date has audited the root and stopped.

`gui-smoke/` is not clean:

    $ cd gui-smoke && npm audit --json | .metadata.vulnerabilities
    {"info":0,"low":0,"moderate":1,"high":16,"critical":0,"total":17}

Most are `@wdio/*` semver-majors and belong with the toolchain migration. But **five carry a plain
non-major `fixAvailable: true`**: `@puppeteer/browsers`, `extract-zip`, `js-yaml`, `webdriver`, and
**`brace-expansion` (high)**.

That last one is the sharp part. **`brace-expansion` is the same package CPE-1926 just fixed at the
root.** The advisory is still live in this repo, in a second lockfile, non-major fixable — and
`gui-smoke.yml` runs this project on CI, which is precisely the "CI runners with repo credentials"
exposure CPE-1926's own risk framing argues from.

## The lesson, which matters more than the five packages

From the worker who ran the pass, on being shown the gap:

> The failure wasn't that I skipped `gui-smoke/`, it's that I never asked how many npm projects there
> were. I read "run `npm audit`" and executed it where I happened to be standing.

That is **CPE-1932 exactly**, in a different costume: a rule followed from memory rather than
enumerated. That ticket found seventeen `Cargo.lock` files while the rule was being applied to one.
The enumerate step here costs a single `git ls-files '*package-lock.json'`.

## Acceptance criteria

- [x] Take the **five non-major fixes** in `gui-smoke/`. `npm audit fix` **without `--force`**, so a
      semver-major is structurally impossible. Commit `gui-smoke/package-lock.json`.
- [x] **Prove it**: `npm audit --json` totals before and after, and confirm the five cleared **by
      name** from the `.vulnerabilities` map, not inferred from the count dropping.
- [x] **`gui-smoke` must still run.** These are the WebdriverIO/puppeteer packages that drive the real
      app; a bumped `webdriver` or `@puppeteer/browsers` that breaks the harness would blind the whole
      GUI verification leg. Run the suite, or say plainly that you could not and that CI is the first
      real test.
- [x] Record what remains deferred, with the same honesty CPE-1926 used: which advisories need the
      `@wdio/*` majors, and whether that migration belongs with CPE-1443's four root majors or is
      separate.
- [x] **Make the enumeration permanent.** A Steward pass that audits one project and reports a repo
      total is worse than none. Either add a check that runs `npm audit` across every
      `package-lock.json` `git ls-files` finds, or write the enumeration step into wherever the
      Steward's procedure lives — preferably the first.
- [x] Correct any surviving statement of the repo's dependency position that reads as repo-wide when
      it is root-only. CPE-1926 and CPE-1443 were already qualified to say **root project only**;
      check nothing else carries the unqualified number.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1057's independent Reviewer, which found the second
project while verifying a pass that was otherwise correct in every particular.

Related: **CPE-1926** (the root pass, PR #1057), **CPE-1443** (the four root semver-majors),
**CPE-1932** (enumerate, do not recall — the same defect in `Cargo.lock` files), **CPE-1171** /
**CPE-1753** (the gui-smoke harness this project drives).

## Work Log

**2026-08-27 — worked and shipped.** npm **10.9.8** / node **v22.22.3** (identical to PR #1057's
toolchain, so the lockfile-rewrite risk that would have forced a stop did not apply). Verified before
touching anything: `npm ci` in `gui-smoke/` left `package-lock.json` **byte-identical**, and it was
byte-identical again after the fix — the same check #1057's reviewer ran.

### Enumeration, first — the actual point of the ticket

`git ls-files '*package-lock.json'` -> exactly two: the root, and `gui-smoke/`.

### The fix

`npm audit fix` **without `--force`**, run to convergence (a third run was a no-op and left the
lockfile byte-identical). `gui-smoke/package.json` was **not** modified — lockfile only, no manifest
change, no semver-major.

| project | before | after |
|---|---|---|
| root (`/`) | `{"moderate":8,"high":1,"critical":1,"total":10}` | unchanged (untouched) |
| `gui-smoke/` | `{"moderate":1,"high":16,"total":17}` | `{"moderate":1,"high":14,"total":15}` |

### Proof, by set-difference on the `.vulnerabilities` key sets (not inferred from the count)

Cleared: **`brace-expansion`**, **`js-yaml`**. New: **none**. Installed versions confirm it:
`brace-expansion` 2.1.4 + 1.1.18 (advisory range `<=1.1.17 || 2.0.0 - 2.1.3`), `js-yaml` 4.3.2
(range `4.0.0 - 4.3.0`). The `brace-expansion` high — the sharp part of this ticket, the same package
CPE-1926 fixed at the root — is gone from the second lockfile.

### Correction to this ticket: it was **two**, not five

The ticket's "five non-major fixable" was read off npm's `fixAvailable` field, which is **optimistic and
wrong here**. `npm audit fix` cleared two and is now a proven no-op, yet `@puppeteer/browsers`,
`extract-zip` and `webdriver` still report `fixAvailable: true`. `extract-zip`'s advisory range is `*` —
no published version is unaffected — so there is no fix to apply and npm claims otherwise anyway.
(`webdriver` flips between "fixable" and "needs a major" across consecutive runs of the same command.)
Building the CI guard on that flag would have red-flagged CI on day one over an unactionable advisory,
so **the guard measures instead of believing**: it runs a real `npm audit fix --package-lock-only` on a
scratch copy and fails only if the lockfile actually moves. Verified both ways — green on the fixed
tree, and red naming exactly `brace-expansion, js-yaml` when pointed at the pre-fix lockfile.

### Does the harness still run?

Partly verified locally; **CI is the first full test**, stated plainly:

- `npm run typecheck` — **passes**. Covers all **43** spec files + `wdio.conf.ts` + `lib/` + `scripts/`
  against the new types, including the one transitive *major* the fix took: **`expect-webdriverio`
  5.7.0 -> 6.0.9** (permitted without `--force` because it is not a declared dependency).
- `npm run test:unit` — **130/130 pass** across 38 suites.
- Runtime module-load smoke over every bumped package (`webdriver`, `webdriverio`, `@wdio/*`,
  `expect-webdriverio`, `@puppeteer/browsers`, `js-yaml`, `extract-zip`, `brace-expansion`) plus live
  `expect()` matchers — all load and work.
- **Not run: the full `npm test` WebdriverIO suite.** It needs a release binary
  (`src-tauri/target/release/cross-platform-explorer.exe`), absent here, and the Windows leg is a
  known-red non-blocking diagnostic anyway (CPE-1048 — WebView2 never gets a `DevToolsActivePort` on
  stock `windows-latest`), so a local run would have been a guaranteed red proving nothing. **The Linux
  leg of `gui-smoke.yml` — the blocking gate — is the first real test of these bumps.**

### The enumeration, made permanent (the option the ticket preferred)

Three parts, because the written-procedure half is the half that already failed:

1. **`scripts/audit-npm-projects.mjs`** — sweeps every project `git ls-files` finds, prints per-project
   totals **and** a repo-wide total explicitly labelled as a sum over N projects, and refuses to run on
   a near-empty enumeration rather than passing vacuously ("0 vulnerabilities across 0 projects" is the
   false green this exists to stop). Deliberately shaped after ci.yml's `lockfile-preflight`, CPE-1932's
   fix, down to the sanity floor.
2. **`npm-audit-sweep` job in `ci.yml`** — runs it on every push/PR. Off the `needs:` chain on purpose:
   `npm audit` is a live registry query whose verdict can change without the tree changing, so a newly
   published advisory must not retroactively block an unrelated PR's builds.
3. **`src/lib/npmProjects.test.ts`** — 5 offline tests pinning discovery against an independent
   `git ls-files`, the floor, the known-project list (a new npm project reds this until acknowledged —
   the tripwire for the human half), lockfile/manifest pairing, and that ci.yml still wires the sweep up.

Plus the prose that should have been enough on its own and wasn't: `.claude/commands/sprint.md`'s
Dependency Steward row now leads with **ENUMERATE FIRST**, and CLAUDE.md gains a "there are TWO npm
projects" guardrail under "Guards and ratchets".

### What remains deferred — and it is not a migration

`gui-smoke/` keeps **15** advisories (1 moderate / 14 high). Investigated rather than assumed: **every
one is upstream-gated with no forward fix in existence.** The project is already on the **latest**
`@wdio/*` (9.31.4) and latest `expect-webdriverio` (6.0.9); the advisories cascade from pins
WebdriverIO's own current release still carries — `deepmerge-ts@^7.0.3` (advisory needs `>=8`),
`mocha@^10.3.0` -> `serialize-javascript@^6.0.2` (needs `>7.0.4`), and `@puppeteer/browsers@2.13.2`
(latest) -> `extract-zip@^2.0.1` whose advisory range is `*`.

**So the answer to the ticket's question is: separate from CPE-1443, and there is no `@wdio/*` major
migration to schedule at all.** CPE-1443 owns four *root* majors that have real forward fixes. What npm
*offers* for the gui-smoke advisories is a **downgrade** — `@wdio/local-runner@7.40.0`,
`@wdio/cli@8.14.6` against an installed 9.31.4 — walking backwards to a tree that predates the advisory.
`--force` would have taken it, silently regressing the harness that guards the entire GUI verification
leg by two major versions. That is the concrete reason "no `--force`" is structural, not stylistic.
No new ticket filed: there is no action available to anyone here. `npm-audit-sweep` will go red on its
own the day an upstream fix lands, which is the correct trigger. Recorded in CPE-1443's Notes too.

### Repo-wide npm position (both projects, stated as a sum — npm 10.9.8)

`{"moderate":9,"high":15,"critical":1,"total":25}` — root 10 + `gui-smoke/` 15. All dev-only; nothing
here ships in the redistributed binary. Also swept for the "reads repo-wide but is root-only" defect:
CPE-1926 and CPE-1443 were already qualified, CPE-1820's line says "at repo root", and nothing else
carried an unqualified number.

## Work Log — review round 2 (2026-08-27)

**CHANGES REQUESTED on #1065; one blocking defect, and it was mine, in the sharpest possible form: the
new guard reported a false green while auditing nothing — the exact failure class it was built to
prevent.** Reproduced both of the reviewer's cases before fixing anything.

`npmAuditJson()` guarded on *"is stdout parseable JSON?"*. npm's `--json` **error** path emits
well-formed JSON with no `metadata` key, so `JSON.parse` succeeded and the guard could never fire:

    $ npm_config_registry=http://127.0.0.1:9/ node scripts/audit-npm-projects.mjs
    Repo-wide total across all 2 npm project(s): {"moderate":0,"high":0,"critical":0,"total":0}
    SWEEP_EXIT=0

The right error message was written and wired to a condition that could not trigger — and a flaky
registry is the likeliest failure mode of this job on CI. With one lockfile corrupted (merge-conflict
markers, a realistic state) it was worse: the broken project contributed `{}` and **the root-only number
printed as the repo-wide sum** — CPE-1945 reproduced inside the guard meant to close it.

**Fixed** by checking `metadata.vulnerabilities` after the parse, extracted as an exported
`isUsableAuditReport()` so it could be pinned offline. Verified all four paths:

| case | before | after |
|---|---|---|
| dead registry | exit 0, "0 across 2 projects" | **exit 1**, `::error::`, no total printed |
| corrupt `gui-smoke/` lockfile | exit 0, root's 10 shown as repo-wide | **exit 1**, `::error::`, no sum printed |
| happy path | exit 0, sum 25 | unchanged — exit 0, sum 25 |
| pre-fix lockfile (red control) | exit 1, names `brace-expansion, js-yaml` | unchanged |

Also taken, as advised: the bare `catch {}` around `npm audit fix` now distinguishes a genuine failure
from "vulnerabilities remain" — measured, the normal path writes **nothing** to stderr while a real
failure carries npm's own `npm error` marker, so a failed probe throws instead of being swallowed into
"no unapplied fix". And an unmakeable scratch dir now fails closed with an `::error::` line instead of a
raw `ENOTDIR` stack. Every throw is caught at top level and printed as
`::error::npm audit sweep FAILED -- it did not audit, which is not the same as finding nothing`.
**4 new offline regression tests** pin npm's real error payload as unusable (9 tests total in
`src/lib/npmProjects.test.ts`).

### Four corrections to my own claims — all were overstatements, all verified independently

1. **`@puppeteer/browsers@2.13.2` is not "(latest)".** Latest is **3.2.1**, whose deps are
   `{yargs, modern-tar}` — it **dropped `extract-zip` entirely**. The real gate is `@wdio/utils` pinning
   `^2.2.0`. Conclusion survives; my stated reason did not.
2. **"no forward fix in existence" was too strong.** `deepmerge-ts@8.0.2` and
   `serialize-javascript@7.1.0` are both **published**; they are unreachable through `@wdio/*`'s
   `^7.0.3` and mocha's `^6.0.2` ranges. Now stated as *gated behind WebdriverIO's and mocha's
   dependency ranges*.
3. **`extract-zip`'s advisory range is `<=2.0.1`, not `*`** — corrected in all three places.
4. **The `expect-webdriverio` claim was wrong, in the scary direction.** I called the 5.7.0 → 6.0.9
   major load-bearing because "that is where every spec's `expect()` comes from". **It is not.** All 43
   spec files import `expect` from **chai**; `@wdio/globals` is imported only for `$`, `$$`, `browser`.
   `chai` is **5.3.3** and appears **zero** times in the lockfile diff — the assertion path was never
   touched. The scariest-sounding risk in the PR was not one, and leaving it would have made a reader
   over-weight it.

### The `--force` finding, restated with measured numbers

Understated the first time. On a scratch copy, `npm audit fix --force` in `gui-smoke/`: downgrades
`@wdio/local-runner` 9.31.4 → **7.40.0**, `@wdio/cli` 9.31.4 → **7.40.0**, `@wdio/mocha-framework`
9.31.2 → **8.14.0**; **rewrites `package.json`'s pins** (so it is not lockfile-local); leaves an
incoherent **v7/v8/v9** mix (`@wdio/types` stays 9.29.1, `webdriverio`/`webdriver` stay 9.31.4); and
takes the project from **15 advisories to 28**. It makes the number worse while regressing the GUI-verify
harness by two majors.

### The deferral still stands

It rested entirely on the sweep being trustworthy, and a registry blip made it green. With the metadata
check landed and regression-tested, "no follow-up ticket — `npm-audit-sweep` reds when upstream ships"
holds as written.
