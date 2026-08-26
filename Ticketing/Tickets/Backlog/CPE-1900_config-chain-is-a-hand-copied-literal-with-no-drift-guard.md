---
id: CPE-1900
title: CONFIG_CHAIN is a hand-copied literal with nothing tying it to release-sidecar.yml's real --config args
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

`src/lib/sidecarBundleResources.test.ts` guards the shipped app's merged configuration by walking a
`CONFIG_CHAIN` literal — the list of `--config` overlay files `release-sidecar.yml` layers on top of
`tauri.conf.json`. CPE-1873 leaned on that machinery to pin the updater pubkey and endpoints against
an overlay override, and it works: an attacker key injected into any file in the chain now reds the
suite on all three shipped OSes.

**But `CONFIG_CHAIN` is a hand-maintained copy.** `grep -rn CONFIG_CHAIN` finds only the test itself
and a prose comment asking the reader to "keep this in lockstep" with the workflow. Nothing derives it
from, or checks it against, `release-sidecar.yml`'s actual `args:` line.

So adding a fifth `--config` overlay to the workflow **silently narrows the guard while leaving it
green**. The new overlay ships, is merged into the config the user's app runs on, and no test knows it
exists. A convention is not a mechanism.

This is the same root cause as the bug found in CPE-1873's second audit round: the guard's model of
what ships is a **copy** rather than a **derivation**, so anything that changes what ships without
changing the copy escapes it.

## Acceptance criteria

- [ ] Derive the overlay list from `release-sidecar.yml` itself, or assert that the literal matches the
      workflow's real `--config` arguments. Derivation is better; an assertion that fails loudly on
      drift is acceptable.
- [ ] Red-proof it: add a sixth overlay to the workflow without updating the test and confirm the
      suite goes red naming the missing file. Then add it to both and confirm green. Restore.
- [ ] Cover the ordering too, not just membership. RFC 7396 merge is order-dependent — a chain with the
      right files in the wrong order computes a different merged config than the one that ships, and a
      membership-only check would call that correct.
- [ ] Do not regress CPE-1873's pins while restructuring this. The updater pubkey and endpoint
      assertions over the merged config must still red on an injected attacker key in any chain file,
      on all three shipped OSes — re-run that injection after your change.

## Notes

Filed 2026-08-26 by CPE-1873's independent Security Auditor, which flagged it as a note alongside a
proven bypass rather than a merge block. See also **CPE-1873** (the pin), **CPE-1901** (the
`--skip-pin-check` kill switch found in the same pass).

Whoever picks this up should read the auditor's related finding first: Tauri **also** merges
`tauri.<platform>.conf.json` automatically, with no `--config` flag at all, via
`tauri-utils::config::parse::read_from`. That file class is not — and cannot be — in the workflow's
`args:` line, so a derivation from the workflow alone is still incomplete. The correct model of "what
config does the shipped app actually run on" is the workflow's overlays **plus** Tauri's own automatic
per-platform merge.
