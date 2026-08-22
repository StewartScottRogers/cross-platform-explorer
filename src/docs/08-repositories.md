---
title: Repositories
order: 8
category: Development
categoryOrder: 11
---

# Repositories

Repositories is a small overlay app for working with Git hosts: browse a repo's file tree without
cloning it, clone one to disk, and — once it's local — keep it in sync with two-way Pull/Push/Sync tools
that live in the explorer's status bar. Open it from **Repositories** in the left sidebar.

## Browse and clone

The toolbar has a **Provider** dropdown (GitHub, GitLab, Bitbucket, Codeberg, or **Generic Git**), a
**Repository**/**Git URL** field, and an optional **Token** field. For the four named providers, enter
the repo as `owner/name` and click **Browse** (or press **Enter**) to list its tree
in-app: folders drill down, a `..` row goes back up, and a breadcrumb across the top shows where you are
and lets you jump back to the repo root. File rows show a human-readable size (B/KB/MB). A public repo
needs no token; a private one needs a **personal access token** in the Token field, which also unlocks a
higher API rate limit even for public repos.

Click **Clone** at any point to download the whole repository into a folder you pick with the native
folder dialog — cloning doesn't require having browsed first. It clones into
`⟨folder you chose⟩/⟨repo-name⟩`, and the status line tracks progress ("Cloning owner/repo → target…",
then "Cloned to target") or reports a failure in place.

Check **Remember token for ⟨provider⟩** to save the token in your OS keychain so you don't retype it. The
checkbox is the only thing that saves a token — browsing successfully does **not** quietly keep it, so if
you leave the box unchecked the token lives only as long as the dialog does. Note the other direction too:
unticking the box deletes any token you had previously saved for that provider — immediately, and again on
every successful browse while it stays unticked.

## Generic Git and self-hosted

Switch the provider to **Generic Git** to work with **any** HTTPS or SSH remote, including a self-hosted
GitLab/Gitea/etc. This mode trades the in-app tree browser for a single **Clone**-only workflow — there
is no Browse button, and the body explains why: "In-app browse isn't available for a generic remote —
clone, then sync locally." Paste any `https://host/owner/repo.git` or `git@host:owner/repo.git` URL and
click Clone; a token here only ever applies to an `https` URL (an `ssh` remote authenticates through your
own SSH agent/keys instead), and it's remembered per-**host** rather than per-provider.

The first time you clone from a host the app hasn't reached before, cloning pauses and a consent banner
asks: **"Allow this app to connect to ⟨host⟩?"** — clone from a self-hosted or unknown host needs your
explicit **Grant & clone**, which admits **exactly that host** (no wildcard) to a persisted allow-list
before the clone runs. **Cancel** backs out without granting anything. Once a host is admitted, later
clones from it proceed without asking again.

## Two-way sync

Once a repository is local — cloned from here, or any folder that already has a `.git` — navigating the
explorer into it turns on a small **git** indicator at the right of the **status bar**: the current
branch, `↓N`/`↑N` behind/ahead counts, a `●` dot for uncommitted changes, and, only when there's
something to do, **Pull** (shown only when behind), **Push** (shown only when ahead), and always
**Sync…**. This is a general explorer feature, not exclusive to repos cloned through this page — any
folder with a `.git` gets the same status-bar controls.

The indicator describes the folder the explorer is **in**, so it switches off the moment you're looking at
something that isn't that folder: Home, an archive you've opened in place, an open smart folder, and an
open saved search. The Pull/Push/Sync… buttons go with it — the actions never outlive the branch name they
belong to, so there's nothing to click that would act on a repository the status bar has stopped naming.

**Pull** and **Push** run a single safe step directly (a fast-forward-only pull, or a plain push — no
force). **Sync…** opens a dialog that previews the full plan before doing anything: it shows how far
ahead/behind you are, lets you set the **on-divergence policy** — **Merge** (default), **Rebase**, or
**Manual — never auto-reconcile** — and replans live as you change it, listing the exact steps it would
run (e.g. "Pull (merge)", "Push"). If histories have diverged, it warns that the plan may produce merge
conflicts before you confirm. **Run sync** executes each step in order and stops at the first failure,
logging what succeeded and what didn't. **The sync engine never force-pushes** — there's no force action
in it at all — so a real divergence under "Manual" simply surfaces rather than being resolved for you.

If a merge/rebase leaves unmerged files, the status bar's branch chip switches to a **conflicts** label
with a **Resolve…** button (also reachable from the Sync dialog). That opens an in-app three-way
**conflict resolver**: pick a conflicted file, compare **base / ours / theirs** versions, use one of them
as-is or hand-edit the resolution text, then **mark it resolved** (staged) one file at a time. When every
file is resolved, **Continue** finishes the merge/rebase; **Abort** at any point restores the pre-sync
state instead — nothing is lost either way.

**Auto-sync in the background** is an opt-in checkbox in the Sync dialog, **off by default**, per
repository. Turned on, you pick an interval (5/15/30/60/120 minutes) and it runs a fast-forward pull plus
a push on that timer and whenever the window regains focus — but only ever the two safe actions: a
divergence, a possible conflict, or a blocked plan pauses it and surfaces the reason in the Sync dialog
rather than reconciling anything unattended, and a fast-forward pull is withheld if the tree is dirty
(uncommitted changes) even though pushing already-committed work is still allowed.

## Worked example

You want to review a teammate's public repo without committing to a full clone, then later track it.

1. Open **Repositories**, leave the provider on **GitHub**, type `owner/name`, and click **Browse** to
   walk its tree in-app.
2. Decide you want it locally after all — click **Clone**, pick a folder, and wait for "Cloned to
   ⟨target⟩".
3. Navigate the explorer into the cloned folder; the status bar's git indicator appears once it detects
   commits ahead/behind.
4. Make a local commit, then click **Push** for a quick push, or **Sync…** if you also want to pull first
   and see the plan before anything runs.

## Limits / notes

- **Generic Git has no in-app browser.** You can only clone (then work with the files normally, or sync
  from the status bar) — there's no tree view for a self-hosted or arbitrary remote the way there is for
  the four named providers.
- **The Repository field expects a bare `owner/name`**, not a URL, for GitHub/GitLab/Bitbucket/Codeberg —
  unlike Generic Git's URL field. Pasting that provider's own repo URL (e.g. a `gitlab.com/...` URL while
  GitLab is selected) is stripped down to `owner/name` for you automatically; pasting a URL for the wrong
  provider (or one whose host it doesn't recognize) shows the clear "enter owner/name" prompt instead of a
  confusing "not found" (fixed in CPE-1620).
- **Host consent is per-exact-host, not per-URL or wildcard.** Admitting `git.example.com` doesn't admit
  a subdomain or a different port; each new host you reach via Generic Git asks again.
- **The status-bar sync controls apply to any Git folder**, not just ones cloned from this page — cloning
  here is one way to get a local repo, but Pull/Push/Sync work the same on a repo you created or cloned
  by other means.
- **Auto-sync only ever runs the two provably-safe actions** (fast-forward pull, push). It will never
  attempt a merge or rebase unattended, and it never force-pushes under any policy — that's a property of
  the sync engine itself, not just the auto-sync guard.
- **A resolved conflict is staged, not committed**, until you click **Continue** to finish the
  merge/rebase; **Abort** at any time returns you to the pre-sync state rather than leaving a half-merged
  tree.
