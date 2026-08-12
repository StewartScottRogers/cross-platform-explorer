---
id: CPE-1679
title: The four media-preview GUI-smoke cases flip on unchanged code, and did so invisibly until case-level ratcheting exposed them
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the CPE-1677 work — and found **by the new gate, on its first run**, which is the best evidence
that ticket could have produced for itself.

Four GUI-smoke cases flip between pass and fail on **unchanged code**: `samples/audio/track.ogg`,
`samples/audio/track.mp3`, `samples/video/clip.mp4`, and their sibling — exactly the set whose settle check
is `.mp-media`. Per-case logs across eight real runs show one or two of them failing per run, at random.

Crucially that includes run `31617196015` **on `main`**, where the old whole-file ratchet still printed
`OK`. So this has been happening on the default branch, invisibly, for as long as the flake has existed.

## Why it wasn't fixed in CPE-1677

Case-level ratcheting turns a flaky case into a **coin-flip gate**: the "unlisted case failed" clause reds
the runs where it fails, and the "listed case passed" clause reds the runs where it passes. Something had
to give, and the worker deliberately chose to *name and expose* rather than fix blind — raising a timeout
or widening a selector without understanding the cause could mask a real defect, which is the failure mode
this whole family of tickets exists to stop.

The four are marked `"intermittent": true` in `known-failing.json`, which exempts them in **both**
directions, but: they must still exist (a rename still fails the gate), their reason must cite the runs
proving the flip, and **every intermittent entry prints its observed status on every run**. They are as
unguarded as they were before — but now they are named, visible, and counted.

**That marker is a hole by construction, and this ticket is what closes it.** It should have exactly these
four users and then none.

## Hypothesis — unverified, do not treat as diagnosis

GStreamer/WebKitGTK media initialisation under Xvfb occasionally exceeds the 20-second settle window. That
is the worker's guess from the shape of the failure, not a measurement. **Establish the real cause before
changing anything**, per the standing rule that a fix which merely makes a test quiet is worse than the
flake.

## Scope

1. **Measure it.** Instrument the settle path for the media cases and find out what is actually slow, or
   actually never arriving. Run the job enough times to characterise the distribution rather than
   reasoning from one failure.
2. **Fix the cause.** If it is genuinely media-stack initialisation, the fix is probably to wait on a real
   readiness signal from the player rather than on a wall-clock window — the same lesson CPE-1667 learned
   about duration bounds versus ordering.
3. **Remove the `intermittent` markers** and let the four cases be guarded like everything else.
4. If a marker must survive, say why in the ticket — but the default outcome is that
   `known-failing.json` contains no `intermittent` entries at all.

## Acceptance criteria

- [ ] The real cause is measured and stated, not hypothesised.
- [ ] The four media cases pass on ten consecutive real GUI-smoke runs.
- [ ] All four `"intermittent": true` markers are removed, and the gate stays green.
- [ ] The fix is not a raised timeout unless the measurement shows a raised timeout is genuinely the right
      answer — and if it is, the number is justified by the measured distribution.
- [ ] Deliberately breaking one of the four now reds the job (it is a real guard again).

## Notes

Filed by the Foreman from the CPE-1677 work, 2026-08-12. Reference runs: `31634362037` (green),
`31638003195` (deliberate break, red), `31641032217` (reverted, green), and `31617196015` — the `main` run
where the old gate said OK while a media case was failing.

Related: **CPE-1677** (the case-level ratchet, which exposed this) and **CPE-1639**, whose deliberate-break
experiment first proved the old gate could not see a case regress inside a known-failing file.
