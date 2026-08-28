#!/usr/bin/env bash
# CPE-1951: the MONOTONIC half of the catalog anti-rollback rule.
#
# ## The bug this closes
#
# CPE-1941 made each catalog entry's `version` the **committer timestamp of the tagged commit**
# (.github/workflows/scripts/catalog-version.sh) instead of a publish-time `date +%s`. That is the
# right number, and it introduced a consequence CPE-1941 did not close: the version now tracks
# **commit order, not release order**.
#
# Cut a release from a commit OLDER than the last released one — a hotfix off a maintenance branch,
# a revert branch, `git tag` on a non-tip commit — and the derived version is LOWER than the one
# already live. Everything on the publish side still passes: CATALOG_VERSION_FLOOR is cleared by a
# mile, the future-date check passes, the index and per-manifest signatures verify, every sha256
# binds, `gh release upload` succeeds. **The job is green.** And every client then returns
# `ApplyOutcome::Rollback` (sidecar/host/src/catalog.rs), writes nothing, and says nothing — because
# from a client's side a lower version is indistinguishable from a stale republish.
#
# Demonstrated end to end, on the CLIENT and on DISK rather than on a verdict enum, in
# `sidecar/host/tests/catalog_offtip_release_lower_bound.rs`. The publish side of the same story —
# a real git fixture, the real `catalog-version.sh`, and this script — is in
# `src/lib/catalogPublishLowerBound.test.ts`.
#
# ## Why a fetched lower bound and not a committed counter
#
# The two candidate shapes were costed on CPE-1951 by CPE-1941's own author:
#
#   1. A committed `LAST_PUBLISHED_VERSION` must be bumped by something. Auto-bumping means the
#      release job commits back to the repo from a detached tag checkout; manual bumping rots. And a
#      stale counter degrades into **exactly the static ratchet CATALOG_VERSION_FLOOR already is** —
#      i.e. into this bug.
#   2. This is NOT the trust dependency CPE-1924 rejected. That objection was about *trusting*
#      fetched content to decide what to publish. This uses the fetch only as a **lower bound that
#      fails the build**: a hostile or garbage response can cause a false FAILURE, never a false
#      success. It fails closed, so it needs no signature verification to be safe, and the job
#      already runs `gh release upload` against the same host, so it is not a new egress class.
#   3. It also closes the legacy window forward: if a pre-CPE-1941 tag's re-run ever stamps a large
#      `date +%s` on the live catalog, the next real release fails LOUDLY here instead of being
#      silently refused on every client.
#
# CATALOG_VERSION_FLOOR STAYS. The two answer different questions and both are wanted: the floor
# says "not below what the installed base already holds" (a fact about clients that no fetch can
# see, since a client may be pinned to an old catalog for months); this says "strictly above what is
# published right now". Neither implies the other.
#
# ## The 404 decision — read this before changing the `none` branch
#
# A 404 on the live index URL is the state of the world **today**, not a hypothetical:
# `/releases/latest/` resolves to `v0.57.69-sidecar`, only the plain channel runs the `catalog` job,
# and the last release that actually published a catalog index was **v0.57.33 on 2026-07-25**
# (CPE-1953, issue #1062). So the tempting shape is "404 ⇒ skip the check", because otherwise no
# release can publish again.
#
# That bare skip is refused here, and this is the reason. **Two entirely different facts both
# surface as a failed fetch**, and only one of them is safely read as "no lower bound exists":
#
#   (A) The latest published release EXISTS and simply carries no `catalog-index.json` asset.
#       Nothing is published, so there is genuinely nothing to be newer than. Safe to proceed.
#   (B) The fetch did not happen, or did not complete: DNS failure, connection refused, a timeout, a
#       500, a truncated body, a missing tool. We learned NOTHING about what is published. Treating
#       this as "no lower bound" is the `npm audit` defect in CLAUDE.md — a wrapper that cannot tell
#       *"ran and found nothing"* from *"did not run"*, and reports the second as the first.
#
# So this script ESTABLISHES (A) POSITIVELY before it will accept it: it asks the GitHub API for the
# latest published release, requires that call to succeed, requires a `tag_name`, and enumerates the
# release's asset names. Only an enumeration that succeeded and did not contain `catalog-index.json`
# yields the `none` verdict. Every other route out of this script is a non-zero exit with its own
# message. In particular a 404 on the index URL AFTER the asset list said the asset is there is a
# **contradiction**, not an absence, and is fatal (exit 10).
#
# One consequence, named rather than hedged: if this repository ever legitimately has NO published
# non-draft release at all, `gh api repos/<repo>/releases/latest` 404s and this script exits 4 —
# a release cannot be cut until someone looks. That is deliberate. There are 65+ published releases
# here, so the state is not reachable by accident; and a fork has no CPE_CATALOG_SIGNING_KEY, so the
# whole catalog job's steps are gated off there anyway. Do not add a bootstrap escape hatch to get
# past it — an escape hatch is a skip wearing a coat, and it is what this comment exists to refuse.
#
# ## Drafts, and why `latest` is the right thing to resolve
#
# `gh api repos/<repo>/releases/latest` returns the most recent **non-draft, non-prerelease**
# release, which is exactly what `https://github.com/<repo>/releases/latest/download/...` redirects
# to — and exactly what a default client fetches (`catalog_url()`, src-tauri/src/lib.rs). Draft
# releases are invisible to both. That matters twice:
#   * tauri-action creates the release being cut as a DRAFT, so during a release run `latest` is
#     still the PREVIOUS release — which is precisely the bound wanted. The run does not race
#     itself.
#   * `v0.57.32` carries a higher catalog version than any published release but is a draft no
#     client ever fetched (CPE-1941/CPE-1953). Resolving `latest` rather than taking a max over the
#     releases API is what keeps that number out of the bound.
#
# The bound is the MAX over `entries[].version`. Clients compare per entry
# (`VersionStanding::refusal`), so a candidate above the max is above every entry.

# No top-level `set` on purpose: this file is sourceable, and a sourced script that flips the
# caller's shell options is its own bug class. Every construct below is instead written to be safe
# under a caller's `set -euo pipefail` — in particular every command substitution whose command may
# legitimately fail uses `x=$(…) || rc=$?` (or sits in an `if` condition), because a BARE failing
# assignment under `set -e` aborts the step *before* the diagnostic can be printed. That exact
# failure was found and fixed twice already in this job (CPE-1893 UAT round 1, and again in
# release.yml's bundle-verify step), so it is written down rather than rediscovered.
#
# `|| return $?` and not `if ! cmd; then return $?; fi`: after `! cmd`, bash reports the NEGATED
# status, so `$?` in the then-branch is 0 and every distinct exit code below would collapse into
# one. Distinct codes are the whole point of this file.

# The fetch budget. Deliberately finite and deliberately fatal on expiry: an unbounded fetch on the
# release path is a hang, and a hang here reads as a stuck release rather than as a refusal.
CATALOG_LB_CONNECT_TIMEOUT="${CATALOG_LB_CONNECT_TIMEOUT:-15}"
CATALOG_LB_MAX_TIME="${CATALOG_LB_MAX_TIME:-60}"

# catalog_lower_bound_url <owner/repo>
#   The EXACT URL a default client fetches. Kept as a function with no override hook so nothing can
#   quietly point this check at a different origin than the one clients read. Pinned against
#   `catalog_url()` in src-tauri/src/lib.rs by src/lib/catalogPublishLowerBound.test.ts, which reads
#   that Rust source at run time (comments stripped) rather than asserting the two agree.
catalog_lower_bound_url() {
  printf 'https://github.com/%s/releases/latest/download/catalog-index.json\n' "$1"
}

# catalog_lower_bound_tools
#   A missing tool is "did not run", never "found nothing". Refused up front with its own code so it
#   can never be mistaken for a clean result further down.
#     16 = a required tool is absent
catalog_lower_bound_tools() {
  local missing="" t
  for t in gh curl jq; do
    command -v "$t" >/dev/null 2>&1 || missing="${missing} ${t}"
  done
  if [ -n "$missing" ]; then
    printf 'catalog lower-bound check cannot run: missing required tool(s):%s. Refusing to treat a missing tool as a passing check — that is the difference between "ran and found nothing" and "did not run".\n' \
      "$missing" >&2
    return 16
  fi
}

# catalog_published_lower_bound <owner/repo>
#   stdout: the lower bound as a plain decimal integer, OR the literal `none` when it has POSITIVELY
#           established that the latest published release carries no catalog index.
#   return: 0 for either of those; otherwise one of the codes below, each with its own message.
#     4  = could not resolve the latest published release (gh failed / no published release at all)
#     5  = the releases API answered with a payload this cannot read
#     6  = the index fetch TIMED OUT
#     7  = the index fetch could not reach the host (DNS / connect / TLS)
#     8  = the index fetch was TRUNCATED in transit (curl partial transfer)
#     9  = the index fetch failed for some other transport reason
#     10 = HTTP 404 on the index URL, contradicting the asset list that said it is there
#     11 = HTTP 5xx on the index URL
#     12 = an unexpected HTTP status on the index URL
#     13 = HTTP 200 with an EMPTY body
#     14 = HTTP 200 with a body that is not parseable JSON (corrupt or truncated)
#     15 = parsed, but carries no usable entries[].version
#     16 = a required tool is missing (see catalog_lower_bound_tools)
catalog_published_lower_bound() {
  local repo="${1-}"
  if [ -z "$repo" ]; then
    printf 'catalog_published_lower_bound needs an owner/repo\n' >&2
    return 2
  fi
  catalog_lower_bound_tools || return $?

  # ── Step 1: enumerate. This is the ONLY thing that can tell the two 404s apart. ────────────────
  local api_out
  api_out=$(gh api "repos/${repo}/releases/latest" 2>&1) || {
    printf 'catalog lower-bound check: could not resolve the latest published release of %s. `gh api repos/%s/releases/latest` failed:\n%s\nThis is NOT evidence that nothing is published — it is evidence that we do not know. Refusing to publish a catalog version we cannot compare against anything (CPE-1951).\n' \
      "$repo" "$repo" "$api_out" >&2
    return 4
  }

  local tag assets
  tag=$(printf '%s' "$api_out" | jq -r '.tag_name // empty' 2>/dev/null) || tag=""
  if [ -z "$tag" ]; then
    printf 'catalog lower-bound check: the releases API answered for %s but carried no tag_name — the payload is not a release object this can read. Refusing to guess:\n%s\n' \
      "$repo" "$api_out" >&2
    return 5
  fi
  # `.assets` must be an ARRAY. `// empty` on a missing key would be indistinguishable from a
  # release with no assets, and those are different facts.
  if ! assets=$(printf '%s' "$api_out" | jq -er 'if (.assets | type) == "array" then (.assets | map(.name) | join("\n")) else error("assets is not an array") end' 2>&1); then
    printf 'catalog lower-bound check: the latest release of %s (%s) has no readable assets[] array, so its contents could not be enumerated. Refusing to read an unenumerable release as "publishes no catalog":\n%s\n' \
      "$repo" "$tag" "$assets" >&2
    return 5
  fi

  local count=0
  if [ -n "$assets" ]; then
    count=$(printf '%s\n' "$assets" | grep -c '') || count=0
  fi
  if ! grep -Fxq 'catalog-index.json' <<< "$assets"; then
    # (A) above, POSITIVELY established: the release exists, its assets were enumerated, and
    # catalog-index.json is not among them. There is nothing published to be newer than.
    printf '::warning::catalog lower-bound: the latest published release of %s is %s and it carries NO catalog-index.json (%s asset(s) enumerated), so no published catalog version exists to compare against. Proceeding with no lower bound. This is the state CPE-1953/#1062 describes — the last release that published a catalog index was v0.57.33 on 2026-07-25. It is accepted here ONLY because the release was found and its assets were listed; a fetch that merely failed is fatal, not this.\n' \
      "$repo" "$tag" "$count" >&2
    printf 'none\n'
    return 0
  fi

  # ── Step 2: fetch what a client fetches. From here a 404 is a contradiction, not an absence. ───
  local url body err http rc
  url=$(catalog_lower_bound_url "$repo")
  body=$(mktemp) || return 9
  err=$(mktemp) || { rm -f "$body"; return 9; }
  # --retry covers transient 5xx/connection hiccups; it never retries a 404 into a pass and never
  # manufactures a 200 (the same argument catalog-freshness.yml's fetch step makes).
  rc=0
  http=$(curl -sSL \
    --connect-timeout "$CATALOG_LB_CONNECT_TIMEOUT" --max-time "$CATALOG_LB_MAX_TIME" \
    --retry 3 --retry-max-time 20 --retry-delay 2 --retry-connrefused \
    -o "$body" -w '%{http_code}' "$url" 2>"$err") || rc=$?

  local curl_err
  curl_err=$(cat "$err" 2>/dev/null) || curl_err=""
  rm -f "$err"

  if [ "$rc" -ne 0 ]; then
    rm -f "$body"
    case "$rc" in
      28)
        printf 'catalog lower-bound check: the fetch of %s TIMED OUT (curl exit 28, budget %ss connect / %ss total). The published catalog version is unknown, so there is nothing to compare against. Fatal on purpose — a timeout is "did not run", not "nothing is published" (CPE-1951). curl said: %s\n' \
          "$url" "$CATALOG_LB_CONNECT_TIMEOUT" "$CATALOG_LB_MAX_TIME" "$curl_err" >&2
        return 6
        ;;
      5 | 6 | 7 | 35 | 60)
        printf 'catalog lower-bound check: could NOT REACH the host serving %s (curl exit %s — DNS, proxy, connection or TLS). The release asset list said catalog-index.json is published, so this is a broken network path, not an absent catalog. curl said: %s\n' \
          "$url" "$rc" "$curl_err" >&2
        return 7
        ;;
      18)
        printf 'catalog lower-bound check: the body of %s was TRUNCATED in transit (curl exit 18, partial transfer). A partial index cannot yield a trustworthy lower bound. curl said: %s\n' \
          "$url" "$curl_err" >&2
        return 8
        ;;
      *)
        printf 'catalog lower-bound check: the fetch of %s failed (curl exit %s). Refusing to publish without a lower bound. curl said: %s\n' \
          "$url" "$rc" "$curl_err" >&2
        return 9
        ;;
    esac
  fi

  case "$http" in
    200) ;;
    404)
      rm -f "$body"
      printf 'catalog lower-bound check: %s returned HTTP 404, but the latest release (%s) DOES list catalog-index.json among its assets. That is a contradiction — an asset that is listed but not served — not an absence of a published catalog, and it is refused rather than read as "no lower bound" (CPE-1951).\n' \
        "$url" "$tag" >&2
      return 10
      ;;
    5??)
      rm -f "$body"
      printf 'catalog lower-bound check: %s returned HTTP %s — a SERVER ERROR, after retries. The published catalog version is unknown. Fatal on purpose: a 5xx tells us the fetch did not run to completion, never that nothing is published (CPE-1951).\n' \
        "$url" "$http" >&2
      return 11
      ;;
    *)
      rm -f "$body"
      printf 'catalog lower-bound check: %s returned an unexpected HTTP status %s. Refusing to derive a lower bound from a response this does not understand.\n' \
        "$url" "$http" >&2
      return 12
      ;;
  esac

  if [ ! -s "$body" ]; then
    rm -f "$body"
    printf 'catalog lower-bound check: %s returned HTTP 200 with an EMPTY body. An empty index is not "no catalog published" — it is a fetch that produced nothing usable.\n' \
      "$url" >&2
    return 13
  fi

  local bound jq_err
  jq_err=$(mktemp)
  if ! bound=$(jq -r '[.entries[]?.version] | max // empty' "$body" 2>"$jq_err"); then
    printf 'catalog lower-bound check: %s returned HTTP 200 but the body is NOT PARSEABLE JSON — corrupt or truncated. jq said: %s\n' \
      "$url" "$(cat "$jq_err")" >&2
    rm -f "$body" "$jq_err"
    return 14
  fi
  rm -f "$body" "$jq_err"

  case "$bound" in
    '' | *[!0-9]*)
      printf 'catalog lower-bound check: %s parsed, but [.entries[].version] | max yielded [%s], which is not a plain non-negative integer. A published index with no usable version is a broken index, not an absent one.\n' \
        "$url" "${bound:-<empty>}" >&2
      return 15
      ;;
  esac

  printf '%s\n' "$bound"
}

# catalog_lower_bound_check <candidate> <owner/repo>
#   The fatal gate the release job calls. Returns 0 only when <candidate> is strictly greater than
#   the published bound, or when there is positively no published catalog to be newer than.
#     2 = <candidate> is not a plain decimal integer
#     3 = <candidate> is NOT STRICTLY NEWER than the published catalog version — the bug
#     (every other code is passed straight through from catalog_published_lower_bound)
catalog_lower_bound_check() {
  local candidate="${1-}" repo="${2-}"
  case "$candidate" in
    '' | *[!0-9]*)
      printf 'catalog lower-bound check needs a plain decimal candidate version, got: %s\n' \
        "${candidate:-<empty>}" >&2
      return 2
      ;;
  esac
  if [ -z "$repo" ]; then
    printf 'catalog lower-bound check needs an owner/repo\n' >&2
    return 2
  fi

  local bound
  bound=$(catalog_published_lower_bound "$repo") || return $?

  if [ "$bound" = none ]; then
    printf 'catalog lower-bound: no published catalog index to compare against; %s accepted with no lower bound.\n' \
      "$candidate"
    return 0
  fi

  # `-le`, not `-lt`. At EQUALITY a client answers `AlreadyCurrent` and writes nothing, so a `-lt`
  # comparison would let a release publish that reaches no user — measured through the real engine in
  # sidecar/host/tests/catalog_offtip_release_lower_bound.rs
  # (`the_clients_acceptance_boundary_is_strictly_greater_than_the_installed_version`).
  # Red-proofed 2026-08-28: switching this to `-lt` reds "a version EQUAL to the published one is
  # refused too" in src/lib/catalogPublishLowerBound.test.ts.
  if [ "$candidate" -le "$bound" ]; then
    printf '::error::catalog version %s is NOT NEWER than the version %s already published on %s'"'"'s latest release. Publishing it would be fully green here and then be refused by EVERY client as a rollback (ApplyOutcome::Rollback), silently, forever — nobody'"'"'s agent roster would ever update again and nothing would be logged as a release failure. This is what a release cut from an OLDER commit looks like: a hotfix off a maintenance branch, a revert branch, or `git tag` on a non-tip commit (CPE-1951). Re-cut the tag from a commit newer than the one already released.\n' \
      "$candidate" "$bound" "$repo" >&2
    return 3
  fi

  printf 'catalog lower-bound: %s > %s (the version on the latest published release) — strictly newer, so every client will accept it.\n' \
    "$candidate" "$bound"
}

# Runnable directly as well as sourced, so a candidate can be checked locally without a run:
#   bash .github/workflows/scripts/catalog-lower-bound.sh <candidate> <owner/repo>
#   bash .github/workflows/scripts/catalog-lower-bound.sh --bound <owner/repo>
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  set -uo pipefail
  if [ "${1-}" = "--bound" ]; then
    shift
    catalog_published_lower_bound "$@"
    exit $?
  fi
  catalog_lower_bound_check "$@"
  exit $?
fi
