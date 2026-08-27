---
id: CPE-1945
title: `gui-smoke/` is a second npm project that no Dependency Steward pass has ever audited — 17 advisories, 5 non-major fixable, including the same `brace-expansion` high the root just fixed
type: task
priority: Medium
status: Open
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

- [ ] Take the **five non-major fixes** in `gui-smoke/`. `npm audit fix` **without `--force`**, so a
      semver-major is structurally impossible. Commit `gui-smoke/package-lock.json`.
- [ ] **Prove it**: `npm audit --json` totals before and after, and confirm the five cleared **by
      name** from the `.vulnerabilities` map, not inferred from the count dropping.
- [ ] **`gui-smoke` must still run.** These are the WebdriverIO/puppeteer packages that drive the real
      app; a bumped `webdriver` or `@puppeteer/browsers` that breaks the harness would blind the whole
      GUI verification leg. Run the suite, or say plainly that you could not and that CI is the first
      real test.
- [ ] Record what remains deferred, with the same honesty CPE-1926 used: which advisories need the
      `@wdio/*` majors, and whether that migration belongs with CPE-1443's four root majors or is
      separate.
- [ ] **Make the enumeration permanent.** A Steward pass that audits one project and reports a repo
      total is worse than none. Either add a check that runs `npm audit` across every
      `package-lock.json` `git ls-files` finds, or write the enumeration step into wherever the
      Steward's procedure lives — preferably the first.
- [ ] Correct any surviving statement of the repo's dependency position that reads as repo-wide when
      it is root-only. CPE-1926 and CPE-1443 were already qualified to say **root project only**;
      check nothing else carries the unqualified number.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1057's independent Reviewer, which found the second
project while verifying a pass that was otherwise correct in every particular.

Related: **CPE-1926** (the root pass, PR #1057), **CPE-1443** (the four root semver-majors),
**CPE-1932** (enumerate, do not recall — the same defect in `Cargo.lock` files), **CPE-1171** /
**CPE-1753** (the gui-smoke harness this project drives).
