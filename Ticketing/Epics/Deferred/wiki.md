# Deferred — epics we deliberately postponed

Epics that are doable and **not** externally gated, but that we have chosen not to pursue now —
either they wait on an internal prerequisite epic/ticket in this repo, or they are consciously
deprioritized. Same meaning as `Tickets/Deferred/`, applied to the epic queue.

- Frontmatter: `status: Deferred`, `tags: [epic]`.
- An epic here MUST record **Deferred on** (the prereq or the deprioritization reason) and
  **Revisit when** (what should bring it back) in its Notes.
- Deferral is a choice, so an epic here stays pickable: move it to
  [`../Backlog/`](../Backlog/wiki.md) or [`../Doing/`](../Doing/wiki.md) at any time.

See [`../Blocked/wiki.md`](../Blocked/wiki.md) for the Deferred-vs-Blocked test.

This folder is normally empty; it exists so the Epics queue is genuinely the same shape as
`Tickets/` (CPE-1676). This `wiki.md` is also what keeps the folder in git — git does not track
empty directories.
