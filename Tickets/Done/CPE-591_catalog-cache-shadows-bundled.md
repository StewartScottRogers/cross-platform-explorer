---
id: CPE-591
title: "Auto-updated agent catalog shadows the newer bundled catalog (stale manifests)"
type: Bug
status: Done
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 2-3h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The sidecar loads a downloaded catalog from `AppData/Roaming/…/ai-console-catalog/` that takes
precedence over the **bundled** catalog shipped in the installer. When the app ships newer agent
manifests than the last published catalog release (as with CPE-583's `swarm` recipe), the stale
download **shadows** them — features silently regress to the older data. Surfaced by [[CPE-590]].

## Fix options
- Version the catalog and prefer whichever of {bundled, downloaded} is newer.
- Invalidate / refresh the downloaded cache on app-version change (installer or first run).
- Merge manifests field-wise (bundled fills gaps the download lacks).

## Acceptance Criteria
- [x] After installing a build whose bundled catalog is newer than the cached download, the sidecar uses
      the newer manifests (e.g. the `swarm` recipe + any new providers), without manual cache deletion.
- [x] A genuinely newer *downloaded* catalog still wins over an older bundled one (auto-update intact).

## Resolution
Chose the **field-wise merge** (option 3). `agents.rs` — when the signed/downloaded source redefines an
agent that the bundled catalog already loaded, `load_signed_source` now **deep-merges** the download onto
the bundled manifest (`merge_manifest`/`deep_merge`) instead of replacing it: the download's fields win
per key, but bundled fields the download **omits survive** (so a stale catalog can't drop the `swarm`
recipe), and `providers` is **unioned** (a set — neither source's providers are lost). A genuinely newer
download still wins on every field it defines, so auto-update is intact. Test
`merge_manifest_keeps_bundled_only_fields_and_unions_providers`; sidecar 289 passed, clippy clean.

(For the immediate incident the stale cache was also cleared during the 0.40.0 install; this fix makes it
self-healing so no manual deletion is needed going forward.)
