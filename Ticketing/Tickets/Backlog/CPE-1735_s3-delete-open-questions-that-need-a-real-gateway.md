---
id: CPE-1735
title: S3 delete's three recorded asymmetries and its new start-after dependency, to settle against a real gateway
type: bug
priority: Medium
status: Backlog
tags: resource-blocked-hardware
estimate: M
created: 2026-08-14
closed:
---

Filed from the CPE-1727 (PR #903) review and UAT. Everything here is **measured and recorded** in
`crates/s3/src/provider.rs`; none of it is unknown. What it needs is a decision, and every one of the
decisions turns on behaviour no in-process fixture can supply — which is why it is one ticket and not
four, and why it is gated on a real endpoint (**CPE-1518**, the QNAP; or any MinIO/Ceph/AWS bucket).

## 1. `start-after` is now a hard dependency of deleting an empty folder

CPE-1727's third belt re-lists with `start-after` on the marker-only verdict. `start-after` is an
**optional** `ListObjectsV2` parameter, so a gateway that is fully permissive and entirely honest but
does not implement it now **refuses an empty-folder delete that succeeded before the belt existed**.

Measured: `tests::a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow`
(400 `InvalidArgument`; the marker survives on disk).

Refusing is the conservative call and was kept deliberately — treating a failed confirmation as consent
is how CPE-1723's original bug reads. **The question is empirical:** does any real S3-compatible gateway
we care about reject `start-after`? If none does, this stays as is and the ticket closes as measured. If
one does, the options are (a) fall back to the pre-belt behaviour on a *parameter-rejection* status
specifically, (b) probe support once per connection, or (c) accept the refusal and document it per
backend.

## 2. On a name collision, the more privileged credential loses

Over the keyspace `["photos", "photos/a.jpg", "photos/b.jpg"]`:

| credential | result |
|---|---|
| **without** `s3:ListBucket` | `Ok` — the object `photos` is deleted, the prefix untouched |
| **with** `s3:ListBucket` | `Err` — refused as a directory with content; the object `photos` is **undeletable** |

So granting a permission makes an operation impossible. Both halves pinned
(`an_object_and_a_prefix_sharing_a_name_deletes_only_the_object_the_head_proved`,
`the_credential_that_can_list_is_the_one_that_cannot_delete_the_colliding_object`). On `origin/main`
both were refused, so CPE-1727 created the divergence.

The fix is **not** to apply the HEAD proof to the privileged path: that path renders a listing in which
`photos` is a folder, so deleting the object of the same name would report the row the user clicked as
deleted while the folder it names is untouched — the same confident wrong answer, moved. The real fix is
a caller that can say *which of the two it meant*, which is a `FileSystemProvider`-shaped change (an
explicit "this is an object key, not a prefix" affordance) touching every backend. Needs a real gateway
first only to confirm the collision is reachable in practice; the design work is ours.

## 3. "Nothing was there" answers differently per credential

`delete` of a nonexistent key is `Ok` with `s3:ListBucket` (empty probe, then S3's idempotent 204) and
`Err` without it (the HEAD 404s, nothing proves object-ness). Pinned by
`deleting_a_key_that_does_not_exist_is_ok_with_listbucket_and_refused_without_it`. Defensible, but a
caller that treats `delete` as idempotent needs to know it depends on the policy attached to the
credential. Decide whether the HEAD path should return `Ok` on a 404 — it would restore idempotence at
the cost of the one thing that currently keeps a virtual directory undeletable on that path, so it is
**not** a free change.

## 4. The belt's gate leaves its zero-row sibling open

The belt fires only on `raw_entries > 0`. A server that under-fills to **zero** rows and denies
truncation reaches the ordinary-object verdict, where the belt never runs, and the delete reports
success. Pinned by `a_zero_row_under_filler_reaches_the_object_verdict_where_the_belt_does_not_run`.
Strictly milder — the DELETE goes to `photos`, not `photos/`, so no key that exists is destroyed — and it
predates CPE-1727. Closing it costs a second `ListObjectsV2` on **every ordinary single-file delete**,
which is why the gate is where it is. Revisit only if a real gateway is ever observed under-filling.

## Acceptance criteria

- [ ] Each of the four items is exercised against at least one real S3-compatible endpoint, and the
      measured result recorded in the ticket.
- [ ] Item 1 is either closed as "no real gateway rejects `start-after`" or fixed with the evidence that
      one does.
- [ ] Item 2 has a decision recorded — either the `FileSystemProvider` affordance is specified, or the
      asymmetry is accepted and stays documented in `delete`'s doc and `src/docs/31-network.md`.
- [ ] Any behaviour change carries guard-neutralisation evidence per `Ticketing/wiki.md` Evidence Rules.

## Notes

Blocked on hardware/an endpoint, not on design. Related: **CPE-1727** (which created items 1–3 and
recorded item 4), **CPE-1723**, **CPE-1684**, **CPE-1518** (the QNAP, the first real endpoint),
**CPE-1685** (which makes all of this user-reachable).
