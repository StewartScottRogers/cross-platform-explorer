# workshifts_remote_checkpoint — Remote Progress Sync

Run `/workshift` in repeated batches, **POSTing cumulative progress to a remote URL** after each batch, so
progress is visible somewhere other than this machine.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Before the first sync — this leaves the machine

This is the only skill in the family that **sends data off this machine**, so it is gated. Announce it and
confirm before the first POST, every run ([[announce-offsite-and-actions]]):

1. **The default `sync_url` is a placeholder.** `https://example.com/sync` is IANA's reserved example
   domain — it is not a real endpoint and posting there accomplishes nothing. If `sync_url` is the default,
   **do not POST**: say so and ask for the real endpoint.
2. **Confirm the destination with the user before the first sync**, naming the host. Approval covers this
   run, not future ones.
3. **Send counters only.** The payload is `{"completed": <n>}` plus, at most, batch counts and timestamps.
   Never ticket titles, file paths, diffs, source, branch names, tokens, or credentials. Anything sent to an
   external service may be logged, cached, or indexed, and cannot be recalled.
4. **HTTPS only.** Refuse a plain-`http://` endpoint; the payload is progress telemetry about a private repo.

If the user declines, run `/workshifts_checkpoint` instead — same loop, local persistence, no egress.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `sync_url` | `https://example.com/sync` | Sync endpoint — **placeholder; must be replaced** |

## Procedure

1. Run the gate above. `completed = 0`.
2. Loop until a family stop condition fires:
   1. `run_workshift_batch(batch_size)`.
   2. `completed += batch_size`.
   3. `send_checkpoint_to_remote(sync_url, {"completed": completed})`.
   4. Report `Synced checkpoint. Restarting...` plus a plain-language line on what shipped.

## Notes

- **A failed sync must not kill the shift.** Network failure gets bounded exponential-backoff retry
  ([[circuit-breaker-for-retryable-errors]]); after the cap, log the failure, keep working, and report the
  gap at the wrap. Merged work is not lost because a status POST 500'd.
- Persist `completed` locally as well. A remote-only counter is unrecoverable if the endpoint is down when
  the session resets.
- Like `/workshifts_until`, `completed` advances by `batch_size`, not by tickets actually merged — it is a
  progress signal, not an accounting record.
