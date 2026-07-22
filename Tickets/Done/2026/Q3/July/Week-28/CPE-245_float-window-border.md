---
id: CPE-245
title: Popped-out preview window looks unfinished — needs a border/frame
type: Defect
status: Done
priority: Low
component: Frontend
estimate: 20m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

The torn-off preview window (FloatPreview, CPE-234) renders its content
edge-to-edge with no border/frame, so it "does not look right" (user, 2026-07-12).
Give the floating content a proper border/frame so it reads as a finished panel.

## Acceptance Criteria
- [ ] The floating preview content has a visible border/frame (not bare to the edges).
- [ ] Tabs and body stay aligned; light/dark themes both look right.
- [ ] `npm run check` passes; verified in the popped-out window.

## Notes
Styling lives in `src/lib/components/FloatPreview.svelte`. Consider a subtle inset
border around the body + consistent padding.

### filled
FloatPreview now renders as a framed card: root has an 8px inset padding on
var(--bg); tabs and body share a bordered rounded frame (tabs rounded top, body
rounded bottom, both var(--border-strong)). No longer bleeds to the window edge.
check 0/0. Ships in the next release.
