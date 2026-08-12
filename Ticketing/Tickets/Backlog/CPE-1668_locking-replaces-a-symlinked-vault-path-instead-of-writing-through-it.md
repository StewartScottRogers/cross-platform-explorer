---
id: CPE-1668
title: "Locking replaces a symlinked .cpevault path instead of writing through it, unlike create_vault"
type: bug
priority: Low
status: Backlog
component: Backend
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent reviewer of PR #847 (CPE-1645), nit 7.

The re-seal on lock finishes with `std::fs::rename(&staging, blob_path)`. If `blob_path` is itself a
**symlink** to a vault stored elsewhere (a user keeping vaults on another volume and linking them into a
folder, say), `rename` replaces the *link* with a real file: the link is gone, the real vault at the far
end is left holding the pre-lock contents, and the user's next unlock quietly opens the wrong copy.

`create_vault` behaves the opposite way — `std::fs::write` follows the symlink and updates the target.
So the two halves of the same feature disagree about what a symlinked vault path means.

Nothing is destroyed and no data is lost (both files still exist and both are decryptable), which is why
this is Low. The rename direction is arguably the *safer* of the two — a re-seal can never be redirected
into writing a vault somewhere the user did not choose — so this may well be resolved as "keep the
behaviour, make it deliberate and documented" rather than as a code change.

## Fix — decide, then make both ends agree

Options:
- **Keep the rename, document it** (and ideally warn once when the destination is a link).
- **Resolve the link first** and rename onto its target — matching `create_vault`, but re-opening the
  question of whether a link may point outside anywhere we would otherwise refuse to write.
- **Refuse** a symlinked vault path on lock, with a clear message.

Whichever is chosen, `create_vault` and the re-seal must agree, and `docs/design/VAULT-SECURITY.md` §5
should state it.

## Acceptance criteria

- [ ] `create_vault` and the lock-time re-seal treat a symlinked `.cpevault` path the same way.
- [ ] A test covers a symlinked vault path end to end (seal → unlock → edit → lock → unlock), skipping
      loudly where the OS will not create a symlink.
- [ ] The chosen behaviour is stated in `docs/design/VAULT-SECURITY.md` §5.

## Notes

- Source: independent review of PR #847, nit 7, 2026-08-12. Filed rather than changed under a security
  fix, so the behaviour is not altered without a decision.
- Related: [[CPE-1645]] locking a vault re-seals, [[CPE-1248]] vault lifecycle.
- The current behaviour is marked with an explanatory comment at the `rename` call site.

## Work Log

- 2026-08-12 — Filed from the PR #847 review.
