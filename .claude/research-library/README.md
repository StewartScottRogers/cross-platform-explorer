# Research Library — the crew's accumulating knowledge base

This folder is the **persistent research corpus** for the Sprint (see
`.claude/commands/sprint.md`). Researchers are sub-agents that answer genuinely-hard questions and
return tradeoff-labelled options — but historically their findings **evaporated** when the sub-agent
returned. The Library fixes that: every research deliverable is **filed** here by the **Librarian**, so
the next shift, the Product Manager, and future Researchers can **draw on everything the crew has ever
learned** instead of re-researching it cold.

**Guiding principle:** the more research we keep and the better we index it, the more valuable the
Library becomes. A Library *hit* means a Researcher never had to be dispatched — research reused is cost
avoided (a measurable throughput win, see the ledger note below).

## Files

| Path | Committed? | What it is |
|------|-----------|------------|
| `README.md` | committed | This file — the schema + the Librarian's protocol. |
| `INDEX.md` | committed | One line per entry, loaded at shift start (like `MEMORY.md`). The fast-scan surface the Librarian and PM search first. |
| `entries/<slug>.md` | committed | One research deliverable each — the full findings, with frontmatter for retrieval. |

The **whole Library is committed** (unlike the gitignored `ledger.jsonl`): it must *accumulate* across
shifts and be shared between the CLI and desktop surfaces. Deleting research would defeat the point.

## Entry schema (`entries/<slug>.md`)

```markdown
---
topic:       <short-kebab-case-slug>          # matches the filename (no .md)
title:       <the research question, human-readable>
date:        2026-07-25                        # accession date, absolute (never "today")
researcher:  opus | sonnet | haiku | fable     # model that produced the research
relates:     [CPE-1043, epic-810]              # tickets / epics it served ([] if speculative)
tags:        [streaming, tauri, performance]   # retrieval keywords — be generous
status:      current | superseded              # superseded → name the newer entry in the body
sources:     [in-repo, context7, web, worktree-probe]
---

## Question
<what was actually asked — the decision the research had to inform>

## Findings / Options
<the viable, tradeoff-labelled options the Researcher returned — NOT an essay>

## Recommendation
<the pick and the one-line why, if the research reached one>

## Sources
<files / doc refs / URLs / probes the finding rests on, so it can be re-verified>
```

`INDEX.md` line format (mirror `MEMORY.md` — one line, no frontmatter):

```
- [<title>](entries/<slug>.md) — <date> · <tags> · <one-line finding/recommendation>
```

## The Librarian's protocol

The **Librarian** is a Sprint crew role (played by the assistant + sub-agents). Three duties:

1. **Accession ("put it away").** When a Researcher returns, the Librarian normalises the findings into
   an `entries/<slug>.md` file and appends its `INDEX.md` line. **Dedup like the memory system:** if an
   entry already covers the topic, **update it** rather than create a duplicate; when new research
   overturns an old finding, set the old entry's `status: superseded` and point to the new slug. Keep
   entries to the returned options + recommendation, not raw transcript.

2. **Reference ("fetch it back").** The Library is searched **before** any fresh Researcher is
   dispatched. The Foreman/PM asks the Librarian "do we already know this?"; the Librarian scans
   `INDEX.md` (then the matching entries) and returns what's on file. On a **hit**, reuse the existing
   research instead of paying for a new sub-agent. The Librarian is also the PM's reference desk — when
   the PM weighs epics, the Librarian retrieves the relevant prior research.

3. **Research the Library itself.** The Librarian can answer questions **purely from accumulated
   entries** — cross-referencing, spotting that two tickets hit the same wall, surfacing a
   recommendation made three shifts ago. It also **curates the index**: tight one-line findings,
   generous tags, superseded entries marked, so retrieval stays fast as the corpus grows.

## Ledger note — count the Library's payoff

A Library hit is a **researcher dispatch avoided**. When the Librarian answers from the corpus instead
of a fresh Researcher being spawned, log a `librarian` ledger row (see
`.claude/sprint-metrics/README.md`) with `outcome: "library-hit"` and `elapsed_s` ≈ the lookup time
— its `cost_proxy` against the cost of the Researcher it replaced is the Library's measurable ROI.
