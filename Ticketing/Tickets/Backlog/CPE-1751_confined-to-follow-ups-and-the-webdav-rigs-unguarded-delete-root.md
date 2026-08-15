---
id: CPE-1751
title: confined_to follow-ups, and the WebDAV rig's DELETE / still removes the served root
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Four small items from the PR #909 (CPE-1730) review, all explicitly non-blocking there, plus one asymmetry
worth recording as a ticket rather than only as a code comment.

### 1. `confined_to` contradicts its own documented failure policy in one branch

`crates/server/src/fsutil.rs:1063`. The doc says *"every unresolved case here fails closed"*, but the branch
is `Err(_) => { drop a component and continue }` with the comment *"It genuinely is not there"* — that arm
swallows **any** `symlink_metadata` error, not only `NotFound`.

The reviewer could not construct a reachable input (the preceding `canonicalize` arm catches every
non-`NotFound` error first, and a denied-directory probe refused correctly), so this is a doc/code mismatch
rather than a demonstrated hole. But this primitive is explicitly billed as the reference every other guard
in the repo will be compared against, so it should mean exactly what it says:

```rust
Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* drop a component, continue */ }
Err(_) => return false,
```

### 2. `make_dir_link_inner` has a `None == None` assertion

`fsutil.rs:1699`: `std::fs::canonicalize(link).ok() == std::fs::canonicalize(target).ok()` is satisfied when
**both** fail. Harmless for today's callers (the target is always an existing directory), but it is a
verification step that can pass while verifying nothing — the same shape this sprint has been removing
elsewhere.

### 3. `contained_under`'s doc has no forward pointer

`fsutil.rs:718` still advises *"A create-side check needs to canonicalise the target's parent instead"*. A
reader who lands there is told to roll their own instead of being sent to `confined_to`. One line.

### 4. CI's CPE-1717 sabotage step is stale

`.github/workflows/ci.yml:412` says *"One leg per staging mechanism"* and enumerates four; `make_dir_link` is
a fifth with no leg. The gate is shared, so coverage is not actually lost — but the comment now under-counts,
and an under-counting comment on a sabotage gate is exactly the kind of thing that gets trusted later.

### 5. The WebDAV rig's `DELETE /` removes the served root and answers `204`

Recorded at the call site by CPE-1730 rather than changed, and that was the right call for that PR: WebDAV
`DELETE` on a collection is defined to delete the collection, it destroys only the rig's own per-test temp
root, and containment genuinely cannot speak to it (the root is contained in itself).

The asymmetry is that `MOVE`'s **source** did get a `409` guard in that same PR, from the same three lines of
reasoning. So one verb refuses to operate on the served root and another does not, and the only record of
that decision is a code comment. CPE-1731's comment-recorded source gap became a ticket for exactly this
reason; this one should too. Decide: guard it, or write the reason somewhere a reader will find it without
reading that function.

## Acceptance criteria

- [ ] `confined_to`'s not-found branch matches its documented fail-closed policy, and a non-`NotFound` error
      returns `false`.
- [ ] `make_dir_link_inner`'s verification cannot pass when both canonicalisations fail.
- [ ] `contained_under`'s doc points at `confined_to` for the create-side case.
- [ ] The CI sabotage-step comment counts the mechanisms that actually exist.
- [ ] `DELETE /` on the WebDAV rig is either guarded like `MOVE`'s source, or its exemption is recorded with
      the reason outside the function body.

## Notes

Related: CPE-1730 (PR #909), CPE-1731, CPE-1717, CPE-1750 (the production-side sibling, filed High).
