---
id: CPE-1702
title: The GUI-smoke session dies after rapid repeated file opens, and nobody knows why GStreamer's decode fails under Xvfb
type: task
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Two loose ends the CPE-1679 worker found while building a stress harness to measure its flake fix. Both
are real, both were correctly kept out of that ticket's scope, and both are recorded here so the
measurements do not evaporate.

### 1. The WebDriver session dies after rapid repeated opens of the same file

Every one of the worker's three stress runs — two before-fix, one after — was eventually cut short by:

```
A sessionId is required for this command
```

It fired after roughly 4–5 rapid re-opens of the same media file on the before-fix runs, and after ~46 on
the after-fix run (which waited less per attempt because the settle check now succeeds immediately). So the
trigger correlates with the *number of rapid re-opens*, not with the settle check.

**This is not a settle-check bug and it does not affect the real suite**, which opens each file once. But it
means the harness cannot be used for long stress loops, which is exactly what anyone measuring a future GUI
flake will reach for first — the second time someone needs a stress harness, they will rediscover this the
hard way.

Likely a WebKitGTK/GStreamer resource leak or handle exhaustion across repeated media-element mounts. Not
root-caused.

### 2. Nobody knows why GStreamer's decode fails under Xvfb in the first place

CPE-1679 established that when decode fails, `MediaPlayer.svelte` correctly renders its graceful fallback,
and that the harness's failure to recognise that state was the bug. It deliberately did **not** investigate
*why* the decode fails intermittently — missing or slow codec plugin initialisation, resource contention,
something else.

That question has a user-facing edge, which is why it is worth a ticket rather than a shrug: **if the decode
failure is not purely a CI-container artefact, real Linux users occasionally see "Can't play this media file"
for files that should play.** The four fixtures are `track.flac`, `track.mp3`, `track.ogg`, `clip.mp4` —
ordinary formats a user expects to work.

## Scope

`gui-smoke/` harness for item 1. For item 2, investigation first — the deliverable is an answer, and a
follow-up ticket if the answer implicates the app rather than the CI container.

## Acceptance criteria

- [ ] **Item 1**: determine what exhausts the session, at least to the level of "which resource and whose".
      Then either fix it or document the ceiling (e.g. "re-open at most N times per session; start a fresh
      session beyond that") somewhere a future stress harness author will actually find — the harness
      README or a comment on the helper itself, not just a ticket.
- [ ] **Item 2**: answer whether the decode failure is a CI-container artefact (no codec plugins installed
      in the runner image) or something that also affects a normally-configured Linux desktop. State the
      evidence and the scope of the check.
- [ ] If item 2 turns out to affect real users, **file a separate ticket for the user-facing bug** rather
      than fixing it here — the two have different risk profiles and different reviewers.
- [ ] Per the Evidence Rules in `Ticketing/wiki.md`: state the exact scope of every negative result. "The
      decode works" is only ever "it works on the configuration I tested — here it is."

## Notes

Filed by the Foreman from the CPE-1679 work (PR #881), 2026-08-13. The worker measured a 78% → 0% settle
failure rate across real CI runs and explicitly listed both of these as unexplored rather than implying its
fix covered them.

Related: **CPE-1679** (the settle-check fix), **CPE-1677** / **CPE-1680** (the ratchet that gates these
cases), **CPE-1148** (the screenshot capture that produced the diagnostic evidence).
