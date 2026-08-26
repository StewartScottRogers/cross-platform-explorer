---
id: CPE-1907
title: stall-check flags this app's own "background" vocabulary as hard, so a worker on an Agent Watch ticket gets killed twice for describing the feature
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1880's `scripts/stall-check.mjs` classifies a sub-agent's report as `accept` / `re-invoke` /
`take-over`, to catch an agent that has parked waiting for a signal it can never receive. Its
`backgrounded-watcher` family is **hard**, which means no handoff line excuses it, and two hits
escalate to `take-over` — the Foreman abandons the agent and takes the work over itself.

The pattern fires on this application's own domain vocabulary:

- *"The Agent Watch watcher process runs in the background…"*
- *"Thumbnail generation is a background task in the renderer"*
- *"The index engine's background job continues…"*

All three are ordinary, correct sentences for a worker on an Agent Watch, thumbnail, or indexing
ticket to write. All three trip a hard finding.

**The cost changed when the severity did.** The pattern itself is unchanged from CPE-1880's attempt 1,
and the file documents a deliberate over-flag bias on the grounds that a false positive costs one
re-invoke while a false negative costs a hung agent. That trade was correct when these matched as
*soft*. Promoting two families to hard reprices it: a worker who writes such a sentence in two
successive reports is escalated to `take-over`, and **the Foreman kills a healthy agent mid-task.**

Agent Watch is a headline feature of this app ([AGENT-WATCH.md](../../../AGENT-WATCH.md)), so the
collision is not hypothetical — it is the vocabulary of a whole area of the product.

## Acceptance criteria

- [ ] Exempt `background (task|job|process)` when no first-person parking verb accompanies it. The
      shipped B2 pattern already demonstrates the shape: a watcher noun **and** a background phrase
      **and** a parking verb, all in one sentence. Apply the same three-part requirement to
      `backgrounded-watcher` rather than loosening it wholesale.
- [ ] Add all three sentences above to the benign corpus so the exemption is pinned, and confirm they
      classify `accept` both bare and with the mandated `CI still pending on <SHA>` tail.
- [ ] Re-run CPE-1880's own red-proofs afterwards. All five recorded stalls must still flag hard, all
      four reversed-word-order phrasings must still flag, and the eight existing benign entries must
      stay clean. A fix that buys quiet by letting a real stall through is worse than the noise.
- [ ] Decide and record whether `take-over` should require a hard finding in **two different families**
      rather than two reports — an agent repeating one phrasing is weaker evidence than an agent
      exhibiting two distinct stall shapes.

## Notes

Filed 2026-08-26 by CPE-1880's independent reviewer, round 2, classified **file, don't block** — it
approved the PR and flagged this as the consequence of a severity change rather than a defect in the
change itself.

Related: **CPE-1880** (the stall controls), **CPE-1906** (`ci-poll.mjs` robustness gaps found in the
same review), **CPE-1848** (the dispatch contract whose prescribed command caused the original stalls).

Worth reading CPE-1880's Work Log first: the reason two families were promoted to hard is that the
contract mandates every worker append `CI still pending on <SHA>`, and that exact string was excusing
every soft match — so three of five recorded stalls were classifying `accept` in production while the
tests, which replayed them bare, stayed green. Any change here must not reopen that.
