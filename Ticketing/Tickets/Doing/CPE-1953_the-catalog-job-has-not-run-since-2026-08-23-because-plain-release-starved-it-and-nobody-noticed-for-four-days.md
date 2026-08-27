---
id: CPE-1953
title: the catalog job has not run since 2026-08-23 — plain Release starved it for 23 consecutive runs, and the "fail loudly" guard that was supposed to catch this is untested
type: bug
priority: High
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

**No agent catalog has been published since `v0.57.33` on 2026-08-23.** Every release since then
skipped the catalog job entirely.

Measured by PR #1063's worker across all 60 `release.yml` runs. The `catalog` job is
`needs: release`, and its conclusion tracks the release job **exactly** — no exceptions:

    v0.57.33          run=success    catalog=success   <- last one that ran
    v0.57.33-sidecar  run=cancelled  catalog=cancelled
    v0.57.35 … v0.57.69   run=failure    catalog=skipped   (23 consecutive runs)

On that last successful run, `Detect catalog signing key` → **success**, `Build + sign` → success,
`Upload catalog assets` → success.

**So the signing key was never the problem, and the job is not succeeding at zero work.** This is
**CPE-1917**'s plain-Release breakage starving the catalog downstream — a second, larger consequence
of that bug than the one CPE-1917 was filed for.

## Why this went unnoticed for four days

`release.yml:395` claims CPE-1893 made this job *"run and FAIL LOUDLY at `gh release upload` rather
than skipping quietly."* Every `skipped` above **predates** that change — CPE-1893 landed
2026-08-26 20:25, the last release run was 2026-08-23 — so the claim is **untested**, not false. No
tag has been cut since it landed.

That is worth stating plainly: the guard designed to make exactly this failure loud has never once
been exercised against a real release.

The independent backstop **did** fire: `catalog-freshness.yml` has exactly one run,
`2026-08-27T19:14 schedule failure`. It is flagging this right now.

## Also correct the record

The asset scan says `v0.57.32` is the newest release **carrying** a catalog index — but `v0.57.33`
**produced** one and that release has since been deleted. **Use v0.57.33 as the last good publish.**
(Separately, `v0.57.32` is a *draft*, so no client ever fetched it — see CPE-1941's review.)

## Acceptance criteria

- [ ] **Confirm the chain is actually repaired.** CPE-1917's fix merged today as PR #1048 but **no tag
      has been cut since**, so nothing has proven the plain Release path works end to end. Cut a
      release, or exercise the path deliberately, and verify the catalog job runs **and uploads**.
- [ ] **Exercise CPE-1893's fail-loudly guard for real.** It has never run. Force the condition it
      guards (no signing key on a tag build) and confirm the job fails rather than skipping. An
      untested guard on the release path is the shape this repo has been finding all week.
- [ ] **Decide whether `needs: release` is the right coupling at all.** A broken installer build
      silently stops agent-catalog updates for every user — two failures with different blast radii
      chained to one condition. Either decouple them, or make the skip **loud** (a failing status, not
      a grey one), and record which and why.
- [ ] **Check `catalog-freshness.yml` is reaching someone.** It is failing on schedule right now and
      that failure sat unread. A backstop nobody reads is not a backstop — verify the notification
      path, or say what it should be.
- [ ] Report how long the gap actually was, in days and in releases, once the chain is repaired, and
      whether any client-visible state needs correcting after the resumption.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1063's worker, which measured the run history while
answering an incidental question about published catalog indexes.

Related: **CPE-1917** (the plain-Release breakage that starved it, PR #1048 — merged today, unproven),
**CPE-1893** (the fail-loudly guard, untested), **CPE-308** (the catalog pipeline), **CPE-1951** (the
publish-side lower-bound check, which must handle `latest` having no catalog index at all — which is
true today because of this), **CPE-1941** (the versioning change).

## Numbers corrected 2026-08-27 — the gap is 33 days, not four

PR #1064's worker re-derived the history and this ticket's own headline was wrong in the reassuring
direction:

- **Last catalog publish: `v0.57.33`, 2026-07-25.** Not v0.57.32 (whose index survives only because
  33's release was later deleted, and which is a *draft* no client ever fetched), and not 2026-08-23
  — that is the last *failed* run, not the last successful publish.
- **The gap is 33 days**, not four.
- The "23 consecutive runs" figure is exactly right, but only **2 of them were plain tags**: since
  CPE-1894 a `-sidecar` tag does not trigger `release.yml` at all.

## The freshness backstop worked. The intake failed.

`catalog-freshness.yml` run `33107498544` **filed GitHub issue #1062 ten seconds later**, correctly
diagnosing `HTTP 404 — no catalog is published there at all` at the exact URL a real client fetches.
It is open and was open for hours.

So AC 4 has a different answer than the ticket assumed: **the notification path is not broken.**
`Ticketing/wiki.md` → "External findings" already specifies a Foreman `gh issue list` sweep, and that
sweep simply did not happen. PR #1064's worker deliberately declined to add a second, louder channel
that would depend on someone reading it — the right call. **The fix is procedural, not technical.**

## One more thing the live 404 will not fix by itself

`/releases/latest/` currently resolves to `v0.57.69-sidecar`, so **a plain release alone will not
clear the 404** (CPE-1894 / CPE-1908 / CPE-1909). Whoever closes the gap needs to account for which
release `latest` points at, not just for a successful catalog job.

## Still unproven, and why

No release was cut (deliberately — that is the user's to authorise). So these remain open: a real
runner executing `catalog` end to end; **whether `CPE_CATALOG_SIGNING_KEY` is still valid at all**
(last evidence 2026-07-25 — PR #1064 converts an invalid key from *silent green* to *loud red* on the
next tag, but cannot tell you beforehand); and the gap's end date.
## Work Log

### 2026-08-27 — worker

**AC 1 (cut a release to prove the chain) was deliberately NOT done.** Cutting a release is an
outward-facing publishing action and belongs to the user, not to a sprint worker. Everything below is
what could be established without publishing, and the PR body states exactly what that leaves open.

#### Record corrections (AC 5, and the ticket's own "Also correct the record")

Measured directly from `gh run list --workflow release.yml` and `gh release list`:

| claim | corrected |
|---|---|
| "no catalog since v0.57.33 on **2026-08-23**" | v0.57.33's run succeeded **2026-07-25**. 2026-08-23 is the date of the *last release run* (v0.57.69, failed), not of the last successful one. |
| "four days" | **33 days** (2026-07-25 to 2026-08-27). |
| "23 consecutive runs" | Confirmed exactly 23 (v0.57.35-sidecar 2026-07-26 to v0.57.69-sidecar 2026-08-23) — but only **2** of those were plain tags (v0.57.37, v0.57.69). Since CPE-1894 a `-sidecar` tag no longer triggers `release.yml` at all, so the "23 runs" figure mixes two channels and overstates how many *plain* publishes were lost. |
| "v0.57.32 is the last release carrying an index" | **v0.57.33** is the last good publish. v0.57.33's release was later deleted (hence the asset scan's answer); v0.57.32 is a *draft* no client ever fetched. Corrected in CPE-1951's ticket too. |

Also confirmed live: `/releases/latest/` currently resolves to **v0.57.69-sidecar**, and only the
plain channel runs `catalog`, so the live index URL 404s today. Cutting a plain release does **not**
by itself fix that — CPE-1894/1908/1909 territory.

#### AC 3 — the coupling decision: keep `needs: release`, make every silent path loud

`needs: release` is a genuine **data** dependency, not ordering: this job's only publishing action is
`gh release upload "$TAG"`, which requires the release object `release`/tauri-action creates. A
decoupled `catalog` job would start at t=0 and race that creation, converting today's deterministic,
diagnosable coupling into a nondeterministic upload failure — strictly worse to read. So the coupling
stays and the *silence* is what was fixed.

CPE-1893 had already closed the job-level half (`if: ${{ !cancelled() }}`). Three ways this job could
still end **green having published nothing** were still open, and all three are now closed in
`release.yml`:

1. **No signing key on a tag build** — the big one. Every real step is
   `if: steps.k.outputs.has == 'true'`, so with `CPE_CATALOG_SIGNING_KEY` unset the job ran, skipped
   everything, and concluded `success`. A *green* vacuous success is worse than CPE-1893's grey skip,
   because green is not even suspicious. This is the identical hole **CPE-1923 finding 4** already
   fixed for `verify-published-manifest`'s `Detect updater signing key`; `catalog` was never given the
   same treatment. It is now the same shape, using the same `RELEASE_BUILD: github.ref_type == 'tag'`
   test and the same preserved `has=false` non-tag arm — not a second, differently-reasoned mechanism.
2. **Zero-work sign** — new `Verify the signed bundle before uploading it` step reads the exact file
   a client fetches (`catalog-out/catalog-index.json`), requiring it to exist, be non-empty, parse,
   carry its detached `.sig`, and have at least one `entries[]`. Before the upload, so a useless
   bundle never reaches the release.
3. **An upload that attached nothing useful** — new `Confirm the catalog is actually on the release`
   step re-reads the release's asset list from GitHub and requires `catalog-index.json` +
   `catalog-index.json.sig` by name. Same "verify the PUBLISHED state, not the local intent"
   principle `verify-published-manifest` already applies to `latest.json`.

Plus a terminal `if: always()` **`Catalog publish outcome`** gate: the single place that decides what
"catalog: success" is allowed to mean. Any step neither `success` nor a legitimately-configured skip
lands there as a red status naming which one.

#### AC 2 — CPE-1893's fail-loudly guard, forced

The claim at `release.yml` ("runs and FAILS LOUDLY at `gh release upload` rather than skipping
quietly") had never been exercised — every skip in the history predates it. Forced in
`src/lib/catalogPublishLoudFailure.test.ts` by extracting that step's real `run:` body from the parsed
workflow and executing it under bash with a stub `gh` reproducing GitHub's not-found response.
**Result: the claim holds** — the step exits non-zero, does not skip. It was also confirmed the glob
actually expands (a literal `catalog-out/*` reaching `gh` would be the shape that uploads nothing
while looking like it tried).

But the claim is **conditional in a way the comment did not say**: it only holds when the signing key
is present. With the key unset the step never ran at all and the job was green — which is hole 1
above. The comment now states the condition.

#### AC 4 — is `catalog-freshness.yml` reaching anyone?

**Yes, the automated path works.** Run `33107498544` (2026-08-27T19:14 schedule, failure) filed GitHub
issue **#1062** ("Agent catalog is stale or unreachable", label `catalog-stale`) ten seconds later,
correctly diagnosing HTTP 404. It is open. So the backstop is not silent: the run is red *and* an
issue exists.

What failed is **intake, not notification**. `Ticketing/wiki.md` -> "External findings" already
specifies it: the Foreman sweeps `gh issue list --state open` at sprint start, files a `CPE-NNN`, and
closes the issue. That sweep did not happen between 19:14 and this ticket being filed. No workflow
change would have helped — the signal was delivered; nobody ran the intake. Left as-is rather than
adding a second, louder channel that would also depend on someone reading it.

#### Red-proofs (CPE-1933 — a workflow edit that cannot be demonstrated is a provenance claim)

`src/lib/catalogPublishLoudFailure.test.ts`, 33 tests, executing real `run:` bodies pulled from the
parsed workflow (never regex over raw text), asserting exit codes and `$GITHUB_OUTPUT`:

- detect step: key+tag -> `has=true`/0; **no key + tag -> non-zero, and `has=false` is never
  written**; no key + non-tag -> `has=false`/0; plus a regression demo running the *old* one-liner to
  show it returned 0 + `has=false` on that same tag build.
- upload step: stub `gh` failing -> non-zero (CPE-1893's claim); succeeding -> 0 with an expanded glob.
- bundle verify: missing / empty / unsigned / zero-entry / unparseable index all -> non-zero with
  distinct messages; a real one-entry bundle -> 0 with `entries=1`; and with `jq` removed from PATH
  entirely the step still fails rather than passing.
- confirm step: both assets -> 0; only unrelated assets -> non-zero naming what is missing; index
  without `.sig` -> non-zero; read-back failure -> non-zero ("a lookup failure is not evidence").
- outcome gate: full publish -> 0; each of the four step outcomes set to `skipped` -> non-zero naming
  it; **key present with every outcome empty (the exact shape of this ticket) -> non-zero**.

**Mutation-checked:** with `release.yml` reverted to `HEAD`, **23 of 33** tests fail. The guards
measure the workflow, they do not merely accompany it.

One live bug was found and fixed while writing the confirm step: `printf ... | grep -Fxq` under
`pipefail` reports **141** when grep matches early and printf takes SIGPIPE — i.e. it reports the
asset *missing* precisely when it is *present*. Rewritten as a herestring; both shapes are executed
in the test.

#### CPE-1932 enumeration — what else is `needs:`-chained behind a silent disabler?

Derived at run time from the workflow files rather than recalled, and pinned as a ratchet so a new
chained job cannot be added without recording a verdict. All 11:

| job | verdict |
|---|---|
| `release.yml/verify-published-manifest` | guarded, `!cancelled()` (CPE-1872) |
| `release.yml/catalog` | guarded, `!cancelled()` + the new terminal gate |
| `release-sidecar.yml/verify-published-manifest-sidecar` | guarded, `!cancelled()` |
| `gui-smoke.yml/gui-smoke-linux-verdict` | guarded, `always()` |
| `release-sidecar.yml/release-sidecar` | accepted silent skip — same blast radius as the failure that causes it; nothing to build into |
| `gui-smoke.yml/gui-smoke-linux` | accepted silent skip — no binary to smoke; nothing stops being *delivered* |
| `ci.yml/{backend,crates,net-e2e,sidecar,msrv}` (x5) | accepted **with a recorded caveat** |

**The ci.yml caveat is a real second instance and is deliberately not fixed here.** All five hang off
`lockfile-preflight` with no `if:`. It differs from `catalog` in that nothing there publishes — a skip
withholds a *check* on a PR already red from the preflight, rather than freezing something users
receive. But GitHub counts a **skipped required status check as satisfied**, so if any of the five is
a required check, a `lockfile-preflight` failure could in principle let a PR look mergeable with its
test suite never having run. That is a different blast radius wanting a terminal `always()` verdict
job over the five (mirroring `gui-smoke-linux-verdict`), and it wants its own ticket rather than being
smuggled into this one. **Recommend the Foreman file it.**

#### What remains unproven because no release was cut

- That a plain tag now runs `catalog` to completion and uploads on a *real* GitHub runner.
- That `CPE_CATALOG_SIGNING_KEY` is still valid today (the last evidence is 2026-07-25).
- That the live 404 clears — it will **not** from a plain release alone while `/releases/latest/`
  resolves to a `-sidecar` release (CPE-1894/1908/1909).
- The gap's end date, and whether any client-visible state needs correcting after resumption.

### 2026-08-27 — worker, review round 1 (PR #1064, CHANGES REQUESTED)

Both blockers were in code **this PR added**, and both were the defect class the ticket exists to
eliminate — a guard that reports success while not guarding. Recording them here rather than only in
the PR, because "the fix introduced a smaller copy of the bug" is the part worth remembering.

**Blocker 1 — the pre-upload gate failed OPEN on a non-integer count.** `[ "${entries:-0}" -lt 1 ]`
exits **2** (not 1) when `$entries` is not one integer, and inside an `if` an exit of 2 is
indistinguishable from a plain "false". So the zero-entry check was **skipped**, the step exited 0,
and it went on to upload — the only trace being `[: integer expected` on stderr. Reachable with real
`jq` via a concatenated JSON stream (`{"entries":[]}{"entries":[{…}]}`), which jq reports one length
per document for, at exit 0. Fixed by validating the shape first with a `case` (no subshell, so no
repeat of the pipefail/SIGPIPE trap on the sibling step), then comparing.

**Blocker 2 — the honesty gate lied in this ticket's own headline scenario.** On a tag with no signing
key the detect step exits 1, so `HAS_KEY` reaches the terminal gate **empty**, not `"false"`. The gate
then printed `no catalog signing key configured and this is not a tag build — nothing was published,
and nothing was expected to be`. **Both clauses false**, in precisely the situation the ticket was
filed about, from the step commented as "the single place this job's honesty is decided". The job was
still red via step `k`, so only the summary line lied — which for that step is the same defect in a
smaller place. Fixed by reading the trigger (`RELEASE_BUILD`) directly instead of inferring it from a
missing output; the tag case is now its own `::error::` and exit 1. The adjacent "Only reachable on a
NON-tag trigger" comment was also wrong and is gone.

**Non-blocking, both fixed:**

- **The ratchet was not derived at run time.** The workflow list was hard-coded to the four files that
  happen to carry `needs:` chains; **8** exist. That is exactly the CPE-1932 rule this PR's own body
  invokes. Now read from the directory with the near-empty backstop that rule requires.
- **Three tests passed vacuously where `jq` is absent** (`if (!hasJq) return;` reports a green PASS for
  a test that never ran). Worse than reported: **`jq` is absent on this machine**, so the "a real
  one-entry bundle → exit 0" case would have *failed* had it ever run — the vacuous pass was hiding a
  test that could not work locally at all. Probes moved to module scope and every guard converted to
  `it.skipIf(...)`, so un-runnable cases now report as **skipped**, visibly not-run. The three are
  additionally driven by a faithful `jq` stub (its whole contract with this step is: print the count,
  exit non-zero when the document will not parse), so they run everywhere, with real-jq corroboration
  kept as separate `skipIf` tests for CI's ubuntu runner.

**Two residuals named in the workflow rather than fixed**, as asked: the `.sig` is checked for
presence, never **validity** (a garbage signature passes — verifying it needs the client's verify path,
which is CPE-1954's `VerifiedIndex` work); and the post-upload check reads `.assets[].name` but not
`.size`, so a zero-byte asset would pass (the honest version is a re-download-and-verify, which is
`verify-published-manifest`'s shape and wants its own ticket).

**Red-proofs for the fixes.** 42 tests now (40 runnable here, 2 skipped for absent `jq`; all 42 on CI).
Every assertion is on the step's **own diagnostic**, not merely on a non-zero exit — the reviewer's
methodological note applies directly: its first harness produced a wall of green PASSes that were all
artifacts (rc=127 unreachable temp paths, `set -u` aborts, non-executable stubs), each satisfying
"expect non-zero" for the wrong reason.

- **Whole-file mutation vs `main`: 31 of 40 runnable tests fail** (was 23/33 before this round).
- **Targeted mutation, one fix at a time:** removing only the `case` shape-check kills exactly the two
  blocker-1 tests; removing only the outcome gate's tag branch kills exactly the two blocker-2 tests.
- Blocker 1 is also proved at the mechanism level — raw bash showing `[ "0\n1" -lt 1 ]` exits 2, the
  `if` branch is not taken, the script continues — plus a regression demo running the shipped body
  with only the `case` removed, which exits **0** and would have uploaded.
- The concatenated-stream claim is corroborated against **real jq** where present, so the stub is shown
  faithful rather than convenient.

**One more vacuous green, caught in the tooling.** The first targeted-mutation script for blocker 2
matched a structurally identical `if [ "${RELEASE_BUILD:-}" = "true" ]` block in the *unrelated*
`verify-published-manifest` job earlier in the file, mutated that instead, and the suite stayed green —
which briefly read as "the guard doesn't work". The script's own no-op guard could not fire, because
the pattern *did* match something. Re-anchored on the gate's own error text. Same lesson as the
reviewer's: a green result is only evidence once you have shown what it would take to make it red.

**Rebase note.** `main` had meanwhile added CPE-1953's own numbers-corrected sections and filed
**CPE-1956** for the `ci.yml`-behind-`lockfile-preflight` chain this PR's enumeration recommended.
Both merged in; the recommendation is now tracked.
