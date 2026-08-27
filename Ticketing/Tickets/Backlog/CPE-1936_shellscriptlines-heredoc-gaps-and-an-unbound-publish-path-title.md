---
id: CPE-1936
title: `shellScriptLines` mis-parses two heredoc forms, and the publish path's expected run title is bound to nothing
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Four fix-forward findings from PR #1039's round-3 review. All are latent today — no workflow in the
tree contains the shapes — and all were found by **feeding the parser adversarial input**, not by
reading it. The first two are in `src/lib/shellScriptLines.ts`, which now backs **two** guards
(`channelPurityCoverage.test.ts` and `releaseHangHardening.test.ts`), so its blind spots are shared.

**N8 — a heredoc token inside a quoted string swallows the rest of the step. This one has an unsafe
direction.**

    echo "use <<EOF to start a heredoc"
    cargo run … --bin verify-release-artifacts -- --expect-channel sidecar
    echo tail

    lines    -> ["echo \"use <<EOF to start a heredoc\""]     <- two real lines vanished
    channels -> []

For the channel ratchet this is **safe** — it produces a loud red. For `releaseHangHardening.test.ts`'s
*"no `apt`/`curl` invocation left unhardened"* scan it is the **unsafe** direction: a genuinely
unhardened command silently drops out of the scan. That file's count assertions are a partial
backstop. Fix: match `HEREDOC_START` against the out-of-quote skeleton only — the scanner already
tracks quote state.

**N7 — an indented terminator closes a plain `<<EOF` early.** The check is `raw.trim() ===
heredocDelim`, but real bash requires column 0 for `<<`; only `<<-` strips leading tabs. So heredoc
body lines get scanned as live code:

    lines    -> ["cat <<EOF", "cargo run --bin verify-release-artifacts -- --expect-channel sidecar", "EOF"]
    channels -> ["sidecar"]        <- pulled out of a heredoc BODY

`HEREDOC_START` already captures whether it saw the `-`; it simply is not carried through to the
terminator comparison.

**N9 — an unterminated quote leaves a trailing comment unstripped.** `echo "oops # not stripped` comes
back unchanged. Neutralised in practice by the per-line anchored matching added in round 3, and an
unterminated quote is a shell syntax error in its own right. Lowest priority of the four.

**N10 — the publish path's expected run title is bound to nothing.** The literal
`"Release (sidecar) "` now lives in **three** places with no test tying them together:
`.claude/commands/run.md`'s exact-match `select`, `RELEASING.md`, and `release-sidecar.yml:34`'s
`run-name:`. Editing `run-name:` silently breaks the publish-path lookup. It **fails closed** (the
lookup throws rather than publishing an unverified draft), so this is a maintenance hazard rather
than a safety hole — but it is the **provenance-claim shape** CPE-1933 is filed about, and a one-line
assertion tying `run.md`'s expected title to the workflow's real `run-name:` is exactly this repo's
house style.

## Explicitly NOT in scope

The same review found four **false REDs** in `isRealInvocationLine()` — `cd crates && cargo run …`,
`bash -c "cargo run …"`, `$VERIFY --expect-channel sidecar`, and the `--bin=verify-release-artifacts`
equals-form. Each would make the ratchet cry unguarded on a legitimate refactor. They are **loud, not
silent**, which is the correct failure direction, and widening the predicate to accept them risks
re-opening the decoy family that round 3 closed. Leave them unless one actually bites.

## Acceptance criteria

- [ ] Fix N8 by matching `HEREDOC_START` against the out-of-quote skeleton. **Verify against
      `releaseHangHardening.test.ts` specifically**, since that is the guard where this direction is
      dangerous.
- [ ] Fix N7 by carrying the captured `-` through to the terminator comparison.
- [ ] Fix or explicitly document N9.
- [ ] Bind the run title (N10) with a test that reads `run-name:` out of `release-sidecar.yml` and
      asserts `run.md` expects exactly that — a *derivation*, not a restated constant (CPE-1933).
- [ ] **Red-proof each by feeding the parser the adversarial input above**, not by reading it. Every
      one of these was found that way and none would have been found by inspection.
- [ ] While in there: the scanner is hand-rolled and character-by-character, which is the shape that
      has produced three separate bugs in this repo tonight. Consider whether the adversarial cases
      deserve to be a permanent fixture table rather than one-off checks.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's round-3 review, which recommended all four as
fix-forward. **N6 from that same list — `|| true` neutering a real invocation while the ratchet
reports full coverage — was NOT deferred**; it is a false green in a coverage guard and was fixed in
#1039 itself.

Related: **CPE-1908** (the guard), **CPE-1929** (guards that cannot go red), **CPE-1933** (provenance
claims bound to nothing).
