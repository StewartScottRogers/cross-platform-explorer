# Deferred

Tickets **we have deliberately postponed** — the work is doable and **not** externally gated, but we
have chosen not to do it now. Two typical reasons:

- **Internal prerequisite** — it waits on other unbuilt work *in this repo* (name the ticket). Its
  own core may already be done; the remaining tail needs a sibling ticket to land first.
- **Deprioritized** — real, unblocked work consciously parked to revisit later.

A ticket here MUST have a Work Log note recording:
- **Deferred on** — why it's postponed (the internal prereq ticket, or the deprioritization reason).
- **Revisit when** — the condition that should bring it back (e.g. "when CPE-432 host connection lands").

## Deferred vs Blocked — don't confuse them

| | **Deferred** (here) | **Blocked** (`../Blocked/`) |
|---|---|---|
| Cause | Our own choice / an internal prereq | An **external** gate outside our control |
| Examples | Waits on another repo ticket; parked for later | Certs, macOS/Linux hardware, a paid plan, a third party, a date |
| Can it be picked up now? | **Yes** — deferral is a choice, `/ticketing-work` may un-defer it anytime | **No** — not until the external gate clears |

Deferred is a side state, not terminal. Do NOT close deferred tickets as Won't Fix — they are
postponed, not declined. When you resume one, move it to `Doing/` (via `/ticketing-work`) or back to
`Backlog/`. Deferred tickets are **shown** by `/ticketing-list` (for visibility) but are **not**
offered in its Work-all/subset options — you pick them up explicitly.
