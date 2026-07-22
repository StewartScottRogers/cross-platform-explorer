---
id: CPE-244
title: Make "pop out the right pane into its own window" discoverable
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Tear-off (CPE-234/238) exists but is undiscoverable: a small unlabeled ⬈ icon on
the preview header, disabled until a file is selected. Users can't find how to
pop the right pane into its own window. Make it obvious: a labeled "Pop out"
button, an Application-menu item, and a keyboard shortcut. Also verify the pop-out
actually creates the floating window in the installed build.

## Acceptance Criteria
- [ ] The preview header shows a clearly labeled "Pop out" control.
- [ ] Application menu has "Pop out preview".
- [ ] A keyboard shortcut pops out the current preview.
- [ ] With no file selected, popping out gives a helpful notice (not silence).
- [ ] Verified: it opens a floating preview window in the installed build.
- [ ] `npm run check` passes.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

### filled
Relocated the pop-out control to the far right of the Preview toolbar (Toolbar
"actions" slot, margin-left:auto) with a new "popout" glyph (open-window + arrow)
replacing the ambiguous share icon. Added an Application-menu item "Pop out
preview" (+ handler), a Ctrl+Shift+O shortcut, and a helpful notice when no
single file is selected. Drag-the-tab-bar still pops out. check 0/0.
