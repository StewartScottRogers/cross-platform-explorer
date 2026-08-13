---
id: CPE-1688
title: The network form's scheme-switch auth coercion is correct but unguarded — two humans checked it, nothing will
type: task
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-12
closed: 2026-08-12
---

## Problem

CPE-1686 made the connection form's authentication choices **scheme-scoped**: pick `s3` and the only option
is Access key; pick `ftp` and the options are Password and Key file. Switching the protocol has to re-coerce
the selected auth kind, or you can save a profile that could only ever fail at connect.

It works. It was verified twice, independently — by the PR #866 reviewer and by the UAT, each driving the
real Svelte component and observing the correct coercion in both directions:

```
BEFORE scheme change -> radios: [ 'password', 'key' ] checked: [ 'password' ]
AFTER  scheme change -> radios: [ 'access_key' ]      checked: [ 'access_key' ]
BACK to sftp         -> radios: [ 'password', 'key' ] checked: [ 'password' ]
```

**But neither of those is standing evidence.** All the automated coverage that shipped sits at the pure
`network.ts` layer. The part that carries the feature to a user — `on:change` firing `coerceAuthKind`
*after* `bind:value` has updated `form.scheme`, the relabelled fields, the conditional Access-key-ID input
— is unproven by the test suite.

## Why it is worth a ticket rather than a shrug

The listener-ordering question was a genuine coin-flip: if `on:change` ran before `bind:value`, the
coercion would read the *previous* scheme and the form would settle one step behind. It happens to be
right. Nothing stops a future edit from reordering those, or from replacing `bind:value` with a manual
handler, and silently reintroducing it — and the next person to touch the form has no way to know two
people once checked this by hand.

The gap is not *"is it correct"*, which has been answered twice. It is *"will it stay correct"*.

Deliberately **not** added to CPE-1685's scope: that ticket is backend routing, a Svelte component test
does not belong in it, and it would be the first thing dropped if that ticket ran long.

## Scope

`src/lib/components/NetworkConnectionForm.svelte` — one small component test, following the existing
precedent (`TabBar.test.ts`, `Toolbar.test.ts`) rather than inventing a pattern. Mount it, drive the real
`<select>`, and assert what the two manual checks asserted.

## Acceptance criteria

- [ ] Switching the protocol to `s3` leaves exactly one auth option (Access key), selected.
- [ ] Switching back to a file-server scheme restores Password/Key file with Password selected.
- [ ] The field **labels** change with the scheme (Endpoint / Region / Bucket and prefix vs Host / User /
      Remote path) — this is what a user actually sees.
- [ ] Reordering or removing the coercion turns the test red. Prove it by doing so, and paste the real
      failure output, per the guard-neutralisation rule in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #866 review, 2026-08-12, on the reviewer's explicit recommendation after
the Foreman declined to write the test inside a PR it was also merging.

Related: **CPE-1686** (which introduced the coercion) and the Evidence Rules in `Ticketing/wiki.md` — this
is the distinction between *verified once* and *guarded*.
