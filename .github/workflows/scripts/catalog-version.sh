#!/usr/bin/env bash
# CPE-1941: the ONE definition of "what number does a release stamp on every agent-catalog entry".
#
# ## The bug this replaces
#
# release.yml's `catalog` job used to compute the version as `VERSION=$(date +%s)` — a fresh Unix
# timestamp read **at publish time**, stamped uniformly across every entry in catalog-index.json.
# That number therefore records *when the workflow ran*, never *what it published*.
#
# The catalog trust engine (sidecar/host/src/catalog.rs) enforces anti-rollback purely by comparing
# that number: `VersionStanding::refusal()` accepts an entry only when its `version` is strictly
# greater than the installed one. With a publish-time clock as the source, re-running the release
# workflow on an **old tag** republishes that tag's old manifests under a version newer than
# anything installed, and the engine accepts them. The content goes backwards while the number goes
# forwards, and every signature, every content hash, and every schema check still passes — because
# the bundle genuinely is what that tag published. Reachable with **no signing-key compromise**:
# anyone who can trigger an Actions re-run on an old tag.
# Demonstrated end to end in sidecar/host/tests/catalog_republish_downgrade.rs.
#
# ## What replaces it
#
# The **committer timestamp of the tagged commit** (`git log -1 --format=%ct`), read out of the
# checkout the job already has. No network, no new trust dependency — the correction PR #1051's
# Security Auditor made to CPE-1924's framing, which had costed the fix as "catalog-sign must fetch
# and trust the previously published index" and concluded it wasn't worth it.
#
# Why the committer timestamp specifically:
#
#   * It is a property of the **content**, not of the act of publishing. Re-running an old tag ten
#     times produces the identical number ten times, so the second run is `AlreadyCurrent` and any
#     run against a newer installed catalog is `Rollback`. That is the whole fix.
#   * It stays a Unix epoch, so it remains **numerically comparable to the timestamp versions the
#     installed base already holds** — see the floor below — and catalog-freshness-check.sh's
#     `now - version` age arithmetic keeps working unchanged (it now measures the age of the
#     catalog's *content* rather than of its upload, which is what that check wanted anyway).
#   * Committer time (`%ct`), not author time (`%at`): a rebase, cherry-pick, or amend refreshes
#     `%ct` but preserves `%at`, so `%ct` tracks the order commits actually landed on the branch.
#   * A repo-committed counter was the other candidate. Rejected: it would restart the numbering
#     from a small integer while the installed base holds ~1.79 billion, so every future release
#     would be refused as a rollback, permanently. See CATALOG_VERSION_FLOOR.
#
# ## The floor, and the installed-base transition
#
# The one way a version-scheme change bricks updates is by emitting numbers **below** what clients
# already have installed: anti-rollback would then refuse every future release forever.
#
# The real installed base is measurable, and it was measured across all 65 releases carrying a
# catalog index. Two numbers matter, and the difference between them is a trap worth writing down:
#   * `1784894333` (2026-07-24T11:58:53Z, release `v0.57.31-sidecar`) — the highest version on any
#     **published** release. This is the true high-water mark: the one a client can actually be
#     holding, because the default fetch is `releases/latest/download/`.
#   * `1784951108` (2026-07-25T03:45:08Z, release `v0.57.32`) — higher, but `v0.57.32` is a
#     **DRAFT** (`isDraft: true`, `published_at: null`). Draft assets are never served from
#     `latest/download/`, so no client ever fetched this number. Anyone re-measuring the floor by
#     taking a plain max over the API will land on it; that is safe (it errs high) but it is not
#     the installed base.
# The version sequence is monotone across all 65 of those releases, so there is no evidence the
# old-tag republish was ever exercised in the wild.
#
# Every value the old scheme could produce is a `date +%s` taken at some past publish, so all of
# them are below "now". A commit timestamp is the same kind of number, and any commit that can
# *contain this file* is necessarily newer than this file, so the new scheme starts strictly above
# the old one.
#
# CATALOG_VERSION_FLOOR turns that from an argument into an enforced, fatal check: a release whose
# derived version is below it fails the job instead of publishing a bundle the installed base would
# reject. It clears both numbers above — the real one and the draft one — and sits below the commit
# that introduced it, so it can never fire for a legitimately-cut release.
#
# ## What this floor does NOT catch: a release cut from an OLDER commit
#
# The floor is a **static** ratchet, not a monotonic one: it is a fixed constant, and nothing on the
# publish side compares the derived version against the **last published** one. That leaves a real
# gap this scheme introduced and the old one did not have.
#
# Cut a release from a commit older than the previous release's — a hotfix on a maintenance branch,
# a revert branch, or `git tag` on a non-tip commit — and `%ct` comes out BELOW the live catalog's
# version while still clearing the floor by a mile. Everything then passes: floor ok, future-date
# check ok, signatures verify, sha256 binds, the release goes green. And every client returns
# `Rollback`, writes nothing, and says nothing, because from their side that is indistinguishable
# from a stale republish. The failure is invisible until somebody notices clients stopped updating.
# Under `date +%s` this could not happen, since publish order and version order were the same thing.
#
# Mitigating, but not a fix: scripts/release.ps1 pushes `HEAD --tags`, so ordinary releases are cut
# from the tip of main and stay monotone; this only bites a deliberate off-tip tag.
# Filed as a follow-up (CPE-1941 SEC/reviewer finding, both gates found it independently). The
# preferred shape is a publish-time lower-bound check against the currently published index rather
# than a hand-maintained counter — see the PR body for the reasoning; do NOT close it by widening
# this floor, which would only move the same static ratchet.
#
# Bump it only for a deliberate, understood reason (e.g. after an old-tag re-run stamped a large
# `date +%s` on the live catalog — see "Residual" below); never to make a failing release pass.
CATALOG_VERSION_FLOOR=1787000000 # 2026-08-16T21:33:20Z
#
# ## Residual: tags cut BEFORE this change
#
# Re-running a **pre-existing** tag executes that tag's own copy of release.yml, which still says
# `date +%s`. No edit here can reach it, so every tag cut from now on is immune and older ones are
# not.
#
# How much that is actually worth, measured rather than assumed (CPE-1941 SEC review, F3): a re-run
# uploads with `gh release upload "${{ github.ref_name }}"`, i.e. to the **old tag's own release**.
# The default client fetch is `https://github.com/<repo>/releases/latest/download/`
# (`catalog_url()`, src-tauri/src/lib.rs), and `latest` does not resolve to an older tag's release.
# So a stale re-run reaches the default update path in only two situations, neither of which is the
# downgrade this ticket is about:
#   * the re-run tag already IS `latest` — it republishes its own current content, so nothing goes
#     backwards; or
#   * the user drives the CPE-383 rollback picker, which fetches `releases/download/<tag>/`
#     explicitly and passes `allow_downgrade` for the agents named — i.e. the user has already
#     opted into installing an older version on purpose.
# The residual is real but narrow. Do not carry it forward at the wrong size.
#
# Closing it entirely is operational, cheapest first:
#   1. **Restrict who may re-run workflows** (repo setting). Targeted, and it costs installed clients
#      NOTHING. This is the option to reach for.
#   2. If a stale bundle did get published, raise CATALOG_VERSION_FLOOR above the number it stamped
#      and cut a fresh release.
#   3. Rotating CPE_CATALOG_SIGNING_KEY is the **expensive last resort**, not the first move, and the
#      cost is easy to understate: CATALOG_TRUSTED_KEYS (src-tauri/src/lib.rs) is a **compile-time**
#      `&[&str]`, so rotating makes every ALREADY-INSTALLED client reject *every* catalog bundle —
#      legitimate ones included — until it app-updates. The same secret also signs the model snapshot
#      (model-snapshot.yml, CPE-450/451), so rotation breaks that catalog too.
# Also recorded in docs/design/CPE-308-agent-catalog-updates.md and docs/security/threat-model.md.

# catalog_version_validate <candidate> [now_epoch]
#   Echo <candidate> if it is a usable catalog version; otherwise print why on stderr and return
#   non-zero. NEVER echoes a fallback value — a caller that ignores the exit status still gets
#   nothing to publish rather than a wrong number.
#     2 = not a plain decimal integer
#     3 = below CATALOG_VERSION_FLOOR (would be refused by the installed base as a rollback)
#     4 = more than a day in the future relative to [now_epoch] (defaults to `date -u +%s`; a
#         clock-skewed runner or a commit with a fabricated date — publishing it would poison
#         anti-rollback for every subsequent real release, so it is fatal, not a warning)
catalog_version_validate() {
  local v="${1-}" now="${2:-$(date -u +%s)}"
  case "$v" in
    '' | *[!0-9]*)
      printf 'catalog version must be a plain decimal integer, got: %s\n' "${v:-<empty>}" >&2
      return 2
      ;;
    0*)
      # Not merely cosmetic: a leading zero is read base-10 by bash's `[` but base-8 by some other
      # shells' `test`, so it is refused outright rather than compared ambiguously.
      printf 'catalog version must not carry leading zeros, got: %s\n' "$v" >&2
      return 2
      ;;
  esac
  if [ "${#v}" -gt 18 ]; then
    printf 'catalog version is implausibly large (%s digits): %s\n' "${#v}" "$v" >&2
    return 2
  fi
  if [ "$v" -lt "$CATALOG_VERSION_FLOOR" ]; then
    printf 'catalog version %s is BELOW the floor %s — publishing it would be refused as a rollback by every already-installed client. Refusing to sign.\n' \
      "$v" "$CATALOG_VERSION_FLOOR" >&2
    return 3
  fi
  if [ "$v" -gt "$((now + 86400))" ]; then
    printf 'catalog version %s is more than a day ahead of now (%s) — a skewed clock or a fabricated commit date. Publishing it would block every later release. Refusing to sign.\n' \
      "$v" "$now" >&2
    return 4
  fi
  printf '%s\n' "$v"
}

# catalog_version_for_commit [ref] [repo_dir] [now_epoch]
#   The committer timestamp of <ref> (default HEAD) in <repo_dir> (default the cwd), validated as
#   above. Returns 5 if the ref cannot be resolved.
catalog_version_for_commit() {
  local ref="${1:-HEAD}" dir="${2:-.}" now="${3-}"
  local ct
  # `--` so a ref that looks like a path can never be reinterpreted as one.
  if ! ct=$(git -C "$dir" log -1 --format=%ct "$ref" --); then
    printf 'cannot resolve ref %s in %s — no commit to take a version from\n' "$ref" "$dir" >&2
    return 5
  fi
  if [ -n "$now" ]; then
    catalog_version_validate "$ct" "$now"
  else
    catalog_version_validate "$ct"
  fi
}

# Runnable directly as well as sourced, so a release version can be checked locally without a run:
#   bash .github/workflows/scripts/catalog-version.sh [ref] [repo_dir] [now_epoch]
#   bash .github/workflows/scripts/catalog-version.sh --validate <candidate> [now_epoch]
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  set -uo pipefail
  if [ "${1-}" = "--validate" ]; then
    shift
    catalog_version_validate "$@"
    exit $?
  fi
  catalog_version_for_commit "$@"
  exit $?
fi
