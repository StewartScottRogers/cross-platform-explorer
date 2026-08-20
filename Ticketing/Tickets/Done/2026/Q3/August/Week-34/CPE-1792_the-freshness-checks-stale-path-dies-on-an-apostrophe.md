---
id: CPE-1792
title: the freshness check's stale path dies on an apostrophe before it can report anything
type: bug
priority: High
status: Done
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

CPE-1763's freshness check merged (PR #938) and the Foreman immediately ran the post-merge
demonstration both reviews had deferred. The fresh path passed. **The stale path is broken.**

```
gh workflow run ffmpeg-pin-freshness.yml --ref main -f override_ffmpeg_build_tag=autobuild-2026-08-01-13-21
-> run 32326260348: FAILURE

  3. Extract pins ......................... success
  4. Derive real asset URLs ............... success
  5. HEAD-check pinned assets ............. success      <- correctly detected the pruned tag
  6. Determine current live tags .......... FAILURE
  7. File an issue (deduped) .............. skipped      <- never runs

.../a90bd420.sh: line 29: unexpected EOF while looking for matching `''
##[error]Process completed with exit code 2
```

So the check detects staleness correctly and then **dies before it can tell anyone**. No issue was
filed, and the `dep-pin-stale` label was never created — confirmed with `gh issue list --state all`
(empty) and `gh label list` (no such label).

## Cause

`.github/workflows/ffmpeg-pin-freshness.yml:273`:

```bash
echo "ffmpeg_live_ver=${ffmpeg_live_ver:-<could not determine -- read it off the release's -linux64-lgpl-*.tar.xz asset name>}" >> "$GITHUB_OUTPUT"
```

Inside a `${parameter:-word}` expansion, the `word` is **still parsed for quoting**, even when the whole
expansion sits inside double quotes. So the apostrophe in `release's` opens a single-quoted section that
never closes, and bash fails to parse the rest of the script.

This is the same class of defect as **CPE-1767**, merged earlier the same day: a parser that broke on an
apostrophe inside a comment. An apostrophe in ordinary English prose is not an edge case.

## Why nobody caught it

Both the Reviewer and the UAT flagged that the notification path had never been exercised end-to-end —
`gh issue list` returned empty and no run of this workflow existed anywhere in the repo's history. Both
correctly declined to fire a real issue into the live repo as a probe side effect, and the pre-merge
`workflow_dispatch` demonstration was genuinely impossible (GitHub 404s a workflow absent from the
default branch).

So this is not a gate failure — it is the known, documented, deliberately-deferred gap arriving exactly
where everyone said it would. **The lesson is that "verify post-merge" needs to be a scheduled action
with an owner, not a note.** It was only caught because the Foreman ran the demonstration within minutes
of merging.

## What to do

- Stop putting prose in `${var:-...}` defaults. Assign fallbacks with plain statements instead, which
  are immune to punctuation:
  ```bash
  if [ -z "$ffmpeg_live_ver" ]; then
    ffmpeg_live_ver="<could not determine — read it off the release's -linux64-lgpl-*.tar.xz asset>"
  fi
  ```
  All three `${…:-…}` lines (`:272`, `:273`, `:274`) should be converted; only `:273` has an apostrophe
  today, but the other two are one edit away from the same trap.
- **The verification is now possible and must be done.** The workflow exists on the default branch, so
  `gh workflow run … --ref <branch>` can dispatch the *branch's* version. Prove both paths on the fix
  branch before merging:
  - fresh (no override) → succeeds, files nothing;
  - stale (`-f override_ffmpeg_build_tag=autobuild-2026-08-01-13-21`) → **files the issue**, creates the
    `dep-pin-stale` label, and exits non-zero.
  Paste both run links and the resulting issue link into the Work Log. Then close the test issue.
- While in there, add a `bash -n` syntax check of the workflow's `run:` blocks to CI, or at minimum run
  `shellcheck` over them. A syntax error that only manifests on the rarely-taken branch is precisely
  what a static check is for.
- Check the other `run:` blocks in this file for the same shape before declaring it fixed.

## Notes

Filed by the Foreman, 2026-08-19, from the post-merge demonstration of PR #938, minutes after merging it.

Related: **CPE-1763** (the check itself), **CPE-1789** (the pin it watches, due to be pruned around
2026-08-29 — so this needs to work *before* then), **CPE-1767** (the same apostrophe-parsing class).

## Work Log — 2026-08-19, branch CPE-1792-freshness-stale-path-apostrophe

**Fix.** Replaced all three `${var:-prose}` fallbacks in the "Determine current live tags" step with
plain `if [ -z ... ]` assignments, which are immune to punctuation, and left a comment at the site
explaining why they must not be moved back inline.

**Verification — both paths proven live, for the first time in this workflow's existence.** The
pre-merge demonstration was genuinely impossible (GitHub 404s a workflow absent from the default
branch). Now that CPE-1763 has merged, `--ref <branch>` dispatch works, so the fix was proven on its own
branch *before* merging:

| Run | Ref | Input | Result |
|-----|-----|-------|--------|
| [32326252755](https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/32326252755) | `main` | none (real pin) | **success** — fresh path, filed nothing |
| [32326260348](https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/32326260348) | `main` | `override_ffmpeg_build_tag=autobuild-2026-08-01-13-21` | **failure at step 6** — the bug: staleness detected, then the step died on the apostrophe and step 7 was skipped |
| [32326450303](https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/32326450303) | this branch | same override | **step 6 success, step 7 filed the issue** and exited non-zero as designed |

That middle row is the red-proof: same input, same workflow, only the quoting differs, and it is the
difference between "reports nothing" and "reports correctly".

**The notification path is now exercised end-to-end.** Run 32326450303 created the `dep-pin-stale`
label (which had never existed) and filed issue #942 — the first issue ever opened on this repo. Its
body names both failing URLs with their HTTP status, recommends a **month-end anchor** rather than a
soon-pruned daily (`autobuild-2026-07-31-14-10`, version `n7.1.5-12-g1fdbca85aa`), points at the exact
step in `release-sidecar.yml` to edit, cross-references CPE-1789, and tells the reader to re-run via
`workflow_dispatch` to confirm before it blocks a release. Actionable without spelunking.

Issue #942 has been closed as the deliberate test artifact it was.

**Still open, deliberately not done here:** a `bash -n` / shellcheck pass over this file's `run:` blocks
in CI. A syntax error that only manifests on a rarely-taken branch is exactly what a static check is
for, and it is the thing that would have caught this before merge. Worth its own ticket.
