---
id: CPE-238
title: Support dragging the preview pane header to pop it out (not just the button)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Tear-off (CPE-234) is triggered by a pop-out button (⬈). Users instinctively try
to DRAG the right pane out to read it in its own window and nothing happens.
True cross-window drag isn't possible in a webview, but we can treat a drag
gesture on the preview pane header as the pop-out trigger: press + move past a
small threshold pops the current preview into the floating window (same code as
the button). Clicking the Preview/Details tabs (no movement) is unaffected.

## Acceptance Criteria
- [ ] Dragging the preview pane header (with one file selected) pops the preview
      into the floating window.
- [ ] Clicking the Preview/Details tabs still just switches tabs (no accidental pop-out).
- [ ] The ⬈ button still works; both routes share one code path.
- [ ] `npm run check` passes.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

## Notes
Real drag-into-a-new-OS-window isn't feasible in a webview (told the user at
design time — CPE-234). This is a gesture shortcut for the existing pop-out.

### filled
Added pointer handlers on the preview header (App): a drag past 24px triggers the
existing popOutPreview; plain clicks on the Preview/Details tabs are unaffected.
Shares one code path with the ⬈ button. check 0/0. Ships in 0.10.2.
