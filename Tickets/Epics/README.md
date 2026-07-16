# Epics queue

Umbrella trackers too large for one unit of work. **Separate from `Backlog/`** — an epic here is a
one-page brief (goal, rough scope, open questions) and is **not** researched, planned, or
sub-ticketed until it is **activated** (`/ticketing-epic activate CPE-NNN`).

- `status: Proposed` — dormant brief, no children yet.
- `status: In Progress` — activated; children live in `Backlog/` (each with `epic: CPE-NNN`).
- Closes to `Done/` when all children are Done and the epic's Definition of Done holds.

Managed by the `ticketing-epic` skill; see `Tickets/wiki.md` → "Epics".
