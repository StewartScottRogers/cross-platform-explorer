---
id: CPE-407
title: Status bar shows "X of Y items" when a filter is active
type: feature
priority: low
estimate: XS
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, explorer]
---

## Problem / value
When search or the type-filter narrows the list, the status bar only said "(filtered)" — you
couldn't tell how many of the folder's items matched. Now it reads "3 of 12 items".

## Done
- StatusBar: "X of Y items" when totalCount > itemCount, else "X items" (dropped the vaguer
  "(filtered)"); pure `isFiltered` derivation.
- App: passes totalCount = the folder's pre-filter count (shown.length).
- New StatusBar.test.ts (count readouts + free-space readout).

- [x] Filtered readout shows matched/total
- [x] Plain count + singular when unfiltered; tested
