---
id: CPE-1792
title: the freshness check's stale path dies on an apostrophe before it can report anything
type: bug
priority: High
status: Backlog
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
