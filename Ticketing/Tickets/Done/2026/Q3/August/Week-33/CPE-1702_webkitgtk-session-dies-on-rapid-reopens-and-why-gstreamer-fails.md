---
id: CPE-1702
title: GUI-smoke session dies on rapid re-opens; and CI has no media codecs, so playback is never tested
type: task
priority: Low
status: Done
tags: ready
estimate: M
created: 2026-08-13
closed: 2026-08-13
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

### 2. ANSWERED — CI's runner image has no MP3/AAC/H.264 decoder

The CPE-1679 **UAT** settled this while reviewing PR #881, so this half of the ticket is now a decision,
not an investigation.

The four fixtures are genuine, valid, standard-codec media — not stubs:

- `file(1)`: FLAC 16-bit/44.1 kHz, MPEG-ADTS layer III with ID3v2.3, Ogg/Vorbis, ISO Base Media MP4.
- `ffprobe` parses full stream metadata cleanly; codecs are FLAC, Vorbis, and H.264 + AAC.
- A full `ffmpeg -i … -f null -` decode of all four completed with **zero decode errors**.
- Git history (`703404e3`, CPE-1361 *"make every samples/ fixture real & substantial"*) shows these were
  deliberately rebuilt from tiny stubs into real ffmpeg-encoded media.

And the gap is in the workflow, not the app: `.github/workflows/gui-smoke.yml` installs only
`libwebkit2gtk-4.1-dev` for the Xvfb leg. That pulls in `gstreamer1.0-plugins-base` but **not**
`-good` / `-bad` / `-ugly` / `-libav` — precisely the packages providing the MP3, AAC and H.264 decoders.

So the fallback firing in CI is a **codec-availability artefact of the runner image, not an app defect**,
and there is no user-facing bug hiding here. That closes the worry that motivated this half of the ticket.

**What is left is a choice**, and it is worth making deliberately rather than by default. Today CI exercises
the *graceful-degrade* path for all four media cases and never the *playback* path — so the thing a real
user does most (open an MP3 and hear it) has no CI coverage at all. Installing the plugin packages would
make those four cases test real decode instead.

Note the cost honestly: it adds apt install time to every GUI-smoke run, and it would mean the
graceful-degrade path then loses its only regular exercise. Testing both would need a deliberate split.

## Scope

`gui-smoke/` harness for item 1; `.github/workflows/gui-smoke.yml` for item 2.

## Acceptance criteria

- [ ] **Item 1**: determine what exhausts the session, at least to the level of "which resource and whose".
      Then either fix it or document the ceiling (e.g. "re-open at most N times per session; start a fresh
      session beyond that") somewhere a future stress harness author will actually find — the harness
      README or a comment on the helper itself, not just a ticket.
- [ ] **Item 2 is a decision, not an investigation** — the cause is known (see above). Decide whether to
      install `gstreamer1.0-plugins-{good,bad,ugly}` / `-libav` on the GUI-smoke Linux leg so the four
      media cases exercise real playback, or to leave CI testing the graceful-degrade path. Record the
      reasoning either way.
- [ ] If you install them: the four cases will then assert real playback, so confirm they still pass, and
      say what now covers the graceful-degrade path — it must not silently lose its only coverage. A
      deliberately-corrupt fixture is the obvious candidate.
- [ ] Per the Evidence Rules in `Ticketing/wiki.md`: state the exact scope of every negative result.

## Notes

Filed by the Foreman from the CPE-1679 work (PR #881), 2026-08-13. The worker measured a 78% → 0% settle
failure rate across real CI runs and explicitly listed both of these as unexplored rather than implying its
fix covered them.

Related: **CPE-1679** (the settle-check fix), **CPE-1677** / **CPE-1680** (the ratchet that gates these
cases), **CPE-1148** (the screenshot capture that produced the diagnostic evidence).
