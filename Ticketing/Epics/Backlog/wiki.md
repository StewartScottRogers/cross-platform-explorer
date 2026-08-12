# Backlog — proposed epics

Umbrella trackers too large for one unit of work. **Separate from `Tickets/Backlog/`** — an epic here
is a one-page brief (goal, rough scope, open questions) and is **not** researched, planned, or
sub-ticketed until it is **activated** (`/ticketing-epic activate CPE-NNN`).

- Frontmatter: `status: Proposed`, `tags: [epic]`.
- Activating an epic moves its file to [`../Doing/`](../Doing/wiki.md) and rewrites `status: In Progress`.

## The Epics queue's shape (CPE-1676)

`Ticketing/Epics/` has the **same five status folders** as `Ticketing/Tickets/`, and — exactly as
there — **the folder is the authoritative status**, mirrored in the file's `status:` frontmatter:

| Folder | Epic `status:` | Meaning |
|--------|----------------|---------|
| `Backlog/` (here) | `Proposed` | Dormant brief. No children, no research. |
| [`Doing/`](../Doing/wiki.md) | `In Progress` | Activated; children live in `Tickets/Backlog/` with `epic: CPE-NNN`. |
| [`Blocked/`](../Blocked/wiki.md) | `Blocked` | Gated by something **external** we cannot clear by working. |
| [`Deferred/`](../Deferred/wiki.md) | `Deferred` | Parked by **our** choice / an internal prereq. Still pickable. |
| [`Done/`](../Done/wiki.md) | `Done` | Closed — every child Done and the Definition of Done holds. |

Managed by the `ticketing-epic` skill; see `Ticketing/wiki.md` → "Epics" for the full lifecycle.
