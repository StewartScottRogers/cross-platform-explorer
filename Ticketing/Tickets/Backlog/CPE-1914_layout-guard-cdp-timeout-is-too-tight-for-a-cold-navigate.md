---
id: CPE-1914
title: the layout guard's 15s CDP timeout is too tight for a cold first navigate on a loaded machine
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-26
---

## Summary

`scripts/dev-harness/layout-guard/engine.mjs` applies one fixed `CDP_CALL_TIMEOUT_MS = 15000` to
**every** CDP call. That includes the very first `Page.navigate` against a freshly-launched Chrome and
a cold vite dev server — the single slowest call the harness makes, and the one whose cost the engine's
own code already acknowledges elsewhere by budgeting **40 seconds** for the `readySelector` poll.

On this development machine, CPE-1882's independent UAT could not get a single clean local run:
**7 consecutive attempts, 7 identical failures**, all

    CDP call "Page.navigate" got no response within 15000ms (id=3)

It root-caused it rather than reporting a flake. A raw-CDP probe against the real first-case URL on a
cold server measured `Page.navigate`'s **own acknowledgment** — not the page load — arriving after
**~18.65 s**, past the 15 s ceiling. Raising `CDP_CALL_TIMEOUT_MS` to 60000 locally, changing nothing
else, gave an immediate clean 12/12 pass. Reverted afterwards.

**This is not a CI problem and should not be treated as one.** The job passes on GitHub's runners in
33–38 s total. The UAT identified concrete reasons this machine is slower: a managed/enterprise-policy
Chrome force-installing extensions into every fresh profile (visible as vite HMR "page reload" events
for extension pages on every launch), plus general multi-agent load.

But "a developer machine under load" is the **normal** condition on this project — many worktrees and
agents run concurrently by design — so a harness that only works on an idle machine is a harness
developers will stop running locally, and local is where it is meant to catch things before CI.

## Acceptance criteria

- [ ] Give the initial navigation its own budget rather than sharing the per-call one. The engine
      already treats navigation as a special case for `readySelector` (40 s); make the CDP timeout
      consistent with that judgment instead of contradicting it.
- [ ] Pick the number from a measurement, not a guess, and record what was measured on what hardware.
      The one data point available is ~18.65 s on a loaded Windows machine with policy-managed Chrome.
- [ ] Keep the tight timeout for ordinary calls. `Runtime.evaluate` taking 15 s means something is
      genuinely wrong, and CPE-1882's reviewer relied on exactly that to turn a forced port collision
      into a loud named failure rather than a silent wrong measurement. Do not widen it globally.
- [ ] Make the timeout message name which call and which case, so the next person does not have to
      write a raw-CDP probe to find out. The current message names the call but not the case or URL.
- [ ] Confirm CI timing does not regress — the job should stay in the 30–40 s range it holds today.

## Notes

Filed 2026-08-26 from CPE-1882's independent UAT, which flagged it explicitly as *not* CI-blocking and
deferred the number itself to the reviewer's judgment rather than picking one. Its reviewer separately
got 5 of 5 clean local runs on the same code, which is the clearest evidence this is machine- and
load-dependent rather than a defect in the harness's logic.

Related: **CPE-1882** (the layout guard), **CPE-1910** (the GUI-smoke shard flake — a different harness,
same class of question: how does a real-browser gate behave when the machine underneath it is busy).

Worth noting the shape for whoever picks it up: this is the third time this run that a fixed timeout
tuned on one machine has misbehaved on another — the others being the 600 s tool cap that stalled five
agents (CPE-1880) and the GUI-smoke socket death (CPE-1910). A timeout is a measurement of someone
else's hardware.
