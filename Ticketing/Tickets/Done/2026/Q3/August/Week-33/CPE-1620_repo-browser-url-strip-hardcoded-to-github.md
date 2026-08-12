---
id: CPE-1620
title: "Repositories: pasting a repo URL only strips the host for GitHub — other providers get a bogus \"not found\""
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
---

## Why
Found writing the docs depth pass on `src/docs/08-repositories.md` (CPE-1619, epic CPE-1569) — verifying
the "enter a repository" claim against the real component before documenting it, per the epic's
verify-against-code rule.

`src/lib/components/RepoBrowser.svelte`'s `browse()` (lines 28-31) reads:

```ts
async function browse(toPath = ""): Promise<void> {
  const r = repo.trim().replace(/^https?:\/\/github\.com\//i, "").replace(/\.git$/, "");
  if (!r.includes("/")) { error = "Enter a repository as owner/name."; return; }
  repo = r;
```

The URL-stripping regex is **hardcoded to `github.com`**, unconditionally — it runs the same way no
matter which provider is selected in the dropdown.

## The gap
The **Provider** dropdown offers GitHub, GitLab, Bitbucket, and Codeberg, all sharing one **Repository**
field with the same placeholder (`owner/name (e.g. tauri-apps/tauri)`) and the same "paste and Browse"
workflow. Nothing in the UI tells the user that only a *GitHub* URL gets its host stripped.

Concretely:
- Provider = **GitHub**, paste `https://github.com/owner/name` → stripped correctly to `owner/name`,
  browse works.
- Provider = **GitLab** (or Bitbucket/Codeberg), paste `https://gitlab.com/owner/name` → the regex doesn't
  match a non-github.com host, so `r` stays the full URL. `r.includes("/")` is still true (a URL is full
  of slashes), so the friendly `"Enter a repository as owner/name."` guard **doesn't fire** — the raw URL
  is sent straight to `forgeBrowse(provider, r, ...)` and on to the backend's `browse_path()`
  (`src-tauri/src/lib.rs` `forge_browse_impl`), which builds an API path assuming `repo` is a bare
  `owner/name`. The result is a confusing failure (a malformed API path → a `404`-shaped
  `"Repo '⟨whole URL⟩' not found (or private — add a token)."`) instead of the clear, already-written
  "enter owner/name" guidance the same field gives for a bad GitHub-style input.

So the same "paste the repo URL" instinct works for one of the four providers and silently misfires for
the other three, with an error message that blames the (nonexistent) repo rather than the input format.

## Fix
Either:
1. Generalize the strip to whichever host the selected provider actually uses (`github.com` /
   `gitlab.com` / `bitbucket.org` / `codeberg.org` — the same hosts `clone_host()` in
   `src-tauri/src/lib.rs` already knows), so pasting that provider's own URL works for all four, or
2. At minimum, detect "this still looks like a URL, not `owner/name`" after the strip attempt (e.g. it
   still contains `://` or a bare host) and keep the user on the friendly `"Enter a repository as
   owner/name."` message instead of forwarding it to the backend as a fake repo name.
Add a unit test per provider (paste that provider's own repo URL and a foreign one) so the four don't
silently regress independently again.

**Conflict surface:** `src/lib/components/RepoBrowser.svelte` (and its test file, if one exists —
otherwise add one).

## Acceptance criteria
- Pasting a GitLab/Bitbucket/Codeberg URL while that provider is selected either browses correctly or
  fails with the same clear "enter owner/name" guidance GitHub already gets — never a bogus "not found."
- A regression test covers all four named providers.
- `npm run check` and the relevant vitest suite pass.

## Notes
Non-destructive UX papercut, not data loss — Medium rather than High. Small, self-contained fix once
picked up. Model: sonnet (or haiku — mechanical once the fix approach is chosen).

## Work Log
2026-08-11 — Claim confirmed against `RepoBrowser.svelte` (lines 28-31 as described). Fixed: `browse()`
now strips whichever host the *selected* provider actually uses (`PROVIDER_HOSTS` map mirroring
`clone_host()` in `src-tauri/src/lib.rs`), and detects "this still looks like a URL" after the strip
(wrong provider, or an unrecognized host) to fall back to the existing "Enter a repository as
owner/name." guidance instead of forwarding the raw URL to `forge_browse`. Added unit tests for
`stripRepoUrl`/`looksLikeUrl` covering all four named providers (github/gitlab/bitbucket/codeberg) plus
a foreign-host negative control, and component-level tests pasting each provider's own URL (browses
correctly) and a foreign-provider URL (shows the friendly guidance, never hits `forge_browse`).
`npm run check` clean; `npx vitest run` 287 files / 3640 tests green. Updated
`src/docs/08-repositories.md`'s Limits/notes bullet to describe the fixed behavior. Batched with
CPE-1622 and CPE-1639 into PR #837 (branch `cpe-1620-1622-1639-small-fixes`).
