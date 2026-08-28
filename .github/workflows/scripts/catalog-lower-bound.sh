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
#      fetched content to decide WHAT to publish. This uses the fetch only as a lower bound on
#      whether to publish at all, and the job already runs `gh release upload` against the same
#      host, so it is not a new egress class.
#
#      **What this is NOT safe against, stated precisely — because the sentence that used to sit
#      here was measurably false.** Round 1 of #1091 claimed: *"a hostile or garbage response can
#      cause a false FAILURE, never a false success. It fails closed, so it needs no signature
#      verification to be safe."* Two independent review gates each produced parseable responses
#      that reach **exit 0**, so the claim is withdrawn rather than reworded — it is exactly the
#      CPE-1933 shape, a provenance claim standing next to a green suite, and it is the sentence
#      that licensed shipping an unverified fetch on the release path.
#
#      Routes to exit 0 that are NOT a real bound, all of them by design and none of them fixable
#      by failing closed harder:
#        * the positively-enumerated empty-release branch — an API answer listing a release with no
#          `catalog-index.json` asset yields `none` and no lower bound. That is branch (A) below,
#          it is the state of the world today (#1062), and it must stay reachable.
#        * an index that simply reports a LOWER version than the truth. A bound you fetched is a
#          bound the server chose; nothing here can tell a truthful small number from a forged one.
#      Two more were live until round 2 of #1091 and are FIXED below rather than documented as
#      limits: a bound above 2^63-1 made `[ -le ]` **error** rather than compare, and the fall-
#      through printed "strictly newer" at exit 0 (see `catalog_lb_num_le`); and jq's `max` sorts
#      numbers below strings, so ONE string-typed `version` anywhere in the index masked every
#      numeric one (see the `numbers` filter on the extraction).
#
#      So the correct, narrow claim — and the one the enumerated exit codes below and
#      `src/lib/catalogPublishLowerBound.test.ts` actually assert — is: **every route where the
#      fetch did not produce a usable answer is fatal.** Defeating this guard reverts to
#      pre-CPE-1951 behaviour. It does not forge a catalog: the bundle is still signed with a key
#      this step cannot reach (the `lb` step's env is `GH_TOKEN`/`VERSION`/`REPO` only). Verifying
#      the fetched index's signature would close the second bullet and is deliberately out of scope
#      here — but it is not true that none is needed.
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

# The largest value a catalog entry's `version` can legally hold. `CatalogEntry.version` is a `u64`
# (sidecar/host/src/catalog.rs), so this is `2^64 - 1`. Anything above it is not a version this
# repo's own client type can even hold, so it is a broken index rather than a big one.
# NOT a claim: src/lib/catalogPublishLowerBound.test.ts reads that field's declared Rust type out of
# catalog.rs at run time and asserts this literal is that type's max. Change the field to u32 or
# i64 and the test reds naming both numbers.
CATALOG_LB_U64_MAX='18446744073709551615'

# catalog_lb_log_safe <text>
#   Prints <text> with every line that could be parsed as a GitHub Actions workflow command
#   defanged, and CRs stripped.
#
#   Why: this script echoes REMOTE bytes back into the job log — the `gh api` body on exits 4/5,
#   curl's and jq's stderr, the release tag on the exit-0 permissive path. Actions reads workflow
#   commands out of a step's stdout/stderr, so a forged `tag_name` containing
#   `\n::stop-commands::<token>` DISABLES workflow-command processing for the rest of the job —
#   inside the job whose entire purpose (CPE-1953) is to be loud when it does not publish, and
#   which relies on `::error::`/`::warning::` to be so. Reproduced on #1091 round 2 before this
#   existed: a forged tag emitted `::error::FORGED-ANNOTATION` and `::stop-commands::deadbeef` as
#   real annotations at exit 0.
#
#   Any line CONTAINING `::` is prefixed with `  |`, not merely indented: the runner trims leading
#   whitespace before looking for the `::` prefix, so indentation alone is not a mitigation. `|` is
#   never the start of a workflow command. Pure bash on purpose — no `sed`/`tr` — so the sanitiser
#   itself adds no tool to catalog_lower_bound_tools's list.
#   No trailing newline is added: some callers embed the result mid-sentence (the release tag in the
#   `::warning::`), so the caller owns its own line breaks.
catalog_lb_log_safe() {
  local text="${1-}" line out="" sep=""
  text="${text//$'\r'/}"
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      *::*) out="${out}${sep}  |${line}" ;;
      *) out="${out}${sep}${line}" ;;
    esac
    sep=$'\n'
  done <<< "$text"
  printf '%s' "$out"
}

# catalog_lb_num_le <a> <b>
#   0 when a <= b, 1 when a > b. EXACT for arbitrary-length non-negative decimal integers with no
#   leading zeros, and it performs no integer conversion at all.
#
#   ### Read this before "modernising" the comparison. It is the highest-value note in this file.
#
#   The obvious spelling is `[ "$a" -le "$b" ]`, and that is what round 1 of #1091 shipped. It is
#   fail-OPEN above 2^63-1. Bash's `test` builtin parses `-le` operands with `strtoimax`, and on
#   overflow it prints `integer expected` to stderr and returns **2**. A non-zero `[` is falsy, so
#   the refusal branch is skipped, execution falls through to the success `printf`, and — because
#   this script deliberately runs under `set -uo pipefail` with no `-e` — the step sees exit 0.
#   Measured 2026-08-28 on bash 5.3:
#       $ IDX_BODY='{"entries":[{"version":9223372036854775808}]}' bash catalog-lower-bound.sh \
#             1787200000 owner/repo
#       …: [: 9223372036854775808: integer expected
#       catalog lower-bound: 1787200000 > 9223372036854775808 … — strictly newer …   exit=0
#   At 9223372036854775807 the same input correctly exits 3. `CatalogEntry.version` is a `u64`, so
#   EVERY value in [2^63, 2^64-1] is a legal published version the old line read as "we are newer".
#   **A comparison that ERRORS is not a comparison that is FALSE** — but `[` returns non-zero for
#   both and an `if` cannot tell them apart.
#
#   `[[ a -le b ]]` is NOT the fix and is strictly worse, both halves measured here on bash 5.3:
#     * It does not error on overflow, it WRAPS. `[[ 9223372036854775808 -le 5 ]]` returns **0** —
#       it silently answers "true". The old `[` at least left a message in the log.
#     * `[[ ]]` ARITHMETIC-EVALUATES its operands, and arithmetic evaluation performs command
#       substitution. With `v='a[$(touch PWNED)]'`, `[[ $v -le 1 ]]` returns 0 and CREATES `PWNED`;
#       the same operand under `[ "$v" -le 1 ]` errors with `integer expected` and creates nothing.
#       Both operands here are remote-influenced (the bound comes off the network). Rewriting this
#       comparison as `[[ ]]` would turn a fail-open into command execution.
#   So neither builtin can be trusted with these operands, and this compares by length, then byte
#   by byte — which for equal-length digit strings with no leading zeros IS numeric order, with no
#   locale collation and no subshell. The only `-le`/`-lt` below is on string LENGTHS (≤ 20), which
#   cannot overflow.
catalog_lb_num_le() {
  local a="${1-}" b="${2-}" i=0 ca cb ra rb
  local digits='0123456789'
  if [ "${#a}" -ne "${#b}" ]; then
    [ "${#a}" -lt "${#b}" ]
    return
  fi
  while [ "$i" -lt "${#a}" ]; do
    ca="${a:i:1}"
    cb="${b:i:1}"
    if [ "$ca" != "$cb" ]; then
      # Rank each digit by how much of `digits` precedes it — a length comparison, never an
      # arithmetic one, so a non-digit that slipped past validation cannot be evaluated.
      ra="${digits%%"$ca"*}"
      rb="${digits%%"$cb"*}"
      [ "${#ra}" -lt "${#rb}" ]
      return
    fi
    i=$((i + 1))
  done
  return 0
}

# catalog_lb_plain_u64 <value>
#   0 when <value> is a plain decimal non-negative integer, with no leading zero (except the bare
#   `0`), that fits in the `u64` a CatalogEntry.version actually is. Otherwise 1.
#
#   The leading-zero rule is not pedantry: `[ 010 -eq 8 ]` is FALSE (bash's `test` reads base 10)
#   while `[[ 010 -eq 8 ]]` is TRUE (arithmetic evaluation reads `010` as octal) — both measured
#   here. An input whose value depends on which comparison spelling you picked is refused instead.
catalog_lb_plain_u64() {
  local v="${1-}"
  case "$v" in
    '' | *[!0-9]*) return 1 ;;
    0) return 0 ;;
    0*) return 1 ;;
  esac
  catalog_lb_num_le "$v" "$CATALOG_LB_U64_MAX"
}

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
#     15 = parsed, but carries NO usable entries[].version at all (none numeric / non-negative /
#          integral)
#     16 = a required tool is missing (see catalog_lower_bound_tools)
#     17 = parsed, but the largest usable entries[].version is outside the u64 range a
#          CatalogEntry.version can hold — a broken index, not a big one
catalog_published_lower_bound() {
  local repo="${1-}"
  if [ -z "$repo" ]; then
    printf 'catalog_published_lower_bound needs an owner/repo\n' >&2
    return 2
  fi
  catalog_lower_bound_tools || return $?

  # ── Step 1: enumerate. This is the ONLY thing that can tell the two 404s apart. ────────────────
  # gh's stderr is kept OUT of the body. `2>&1` would splice any gh chatter into the JSON about to
  # be parsed, turning a working call into exit 5; gh suppresses its update notifier under `CI` so
  # that is theoretical, but merging stderr into a payload you are about to parse is a coupling
  # with no upside.
  local api_out api_err gh_err
  api_err=$(mktemp) || return 9
  api_out=$(gh api "repos/${repo}/releases/latest" 2>"$api_err") || {
    gh_err=$(cat "$api_err" 2>/dev/null) || gh_err=""
    rm -f "$api_err"
    [ -n "$gh_err" ] || gh_err="$api_out"
    printf 'catalog lower-bound check: could not resolve the latest published release of %s. `gh api repos/%s/releases/latest` failed:\n%s\nThis is NOT evidence that nothing is published — it is evidence that we do not know. Refusing to publish a catalog version we cannot compare against anything (CPE-1951).\n' \
      "$repo" "$repo" "$(catalog_lb_log_safe "$gh_err")" >&2
    return 4
  }
  rm -f "$api_err"

  local tag assets
  tag=$(printf '%s' "$api_out" | jq -r '.tag_name // empty' 2>/dev/null) || tag=""
  if [ -z "$tag" ]; then
    printf 'catalog lower-bound check: the releases API answered for %s but carried no tag_name — the payload is not a release object this can read. Refusing to guess:\n%s\n' \
      "$repo" "$(catalog_lb_log_safe "$api_out")" >&2
    return 5
  fi
  # `.assets` must be an ARRAY. `// empty` on a missing key would be indistinguishable from a
  # release with no assets, and those are different facts. `.name // "<unnamed>"` so a nameless
  # asset still occupies a line — see the `count` note below.
  if ! assets=$(printf '%s' "$api_out" | jq -er 'if (.assets | type) == "array" then (.assets | map(.name // "<unnamed>") | join("\n")) else error("assets is not an array") end' 2>&1); then
    printf 'catalog lower-bound check: the latest release of %s (%s) has no readable assets[] array, so its contents could not be enumerated. Refusing to read an unenumerable release as "publishes no catalog":\n%s\n' \
      "$repo" "$(catalog_lb_log_safe "$tag")" "$(catalog_lb_log_safe "$assets")" >&2
    return 5
  fi

  # `.assets | length`, NOT a line count over the joined names. Round 1 derived the count from the
  # joined string, so a release whose assets are all nameless objects reported "0 asset(s)
  # enumerated" while the enumeration had in fact found several — a count that lies, printed inside
  # the one message that licenses proceeding with no lower bound.
  local count
  count=$(printf '%s' "$api_out" | jq -r '.assets | length' 2>/dev/null) || count=""
  case "$count" in
    '' | *[!0-9]*) count='an unreportable number of' ;;
  esac
  if ! grep -Fxq 'catalog-index.json' <<< "$assets"; then
    # (A) above, POSITIVELY established: the release exists, its assets were enumerated, and
    # catalog-index.json is not among them. There is nothing published to be newer than.
    # `$tag` is remote-controlled and this line is a workflow command, so the tag goes through the
    # sanitiser: a forged `tag_name` carrying `\n::stop-commands::<token>` would otherwise turn off
    # workflow-command processing for the rest of this job, from inside its ONE exit-0 log line.
    printf '::warning::catalog lower-bound: the latest published release of %s is %s and it carries NO catalog-index.json (%s asset(s) enumerated), so no published catalog version exists to compare against. Proceeding with no lower bound. This is the state CPE-1953/#1062 describes — the last release that published a catalog index was v0.57.33 on 2026-07-25. It is accepted here ONLY because the release was found and its assets were listed; a fetch that merely failed is fatal, not this.\n' \
      "$repo" "$(catalog_lb_log_safe "$tag")" "$count" >&2
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

  # curl's stderr is remote-influenced too (it quotes the host, and on some failures the server's
  # own text), so it goes through the same workflow-command sanitiser as the API body.
  local curl_err
  curl_err=$(cat "$err" 2>/dev/null) || curl_err=""
  curl_err=$(catalog_lb_log_safe "$curl_err")
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

  # `numbers | select(. >= 0 and . == floor)` and NOT a bare `[.entries[]?.version] | max`.
  #
  # jq has a TOTAL ordering across types and it sorts numbers BELOW strings, so `max` over a mixed
  # array returns the string. Round 1 of #1091 took the bare max, and one string-typed `version`
  # anywhere in the index therefore masked every numeric one — the whole index, not just its own
  # entry. Measured 2026-08-28:
  #   $ IDX_BODY='{"entries":[{"version":1787999999999},{"version":"1"}]}' …
  #   catalog lower-bound: 1787200000 > 1 … — strictly newer …   exit=0
  # The real maximum was 1787999999999; the guard bounded against 1 and passed.
  #
  # Filtering rather than erroring on a non-number is deliberate and is the safe direction: a
  # string/null/object/float/negative entry is DISCARDED, so it can only make the bound HIGHER (or
  # leave nothing, which is exit 15) — never lower. `. == floor` drops floats, `. >= 0` drops
  # negatives; a client's `u64` could hold neither, so neither is a version to be newer than.
  local bound jq_err
  jq_err=$(mktemp)
  if ! bound=$(jq -r '[.entries[]?.version | numbers | select(. >= 0 and . == floor)] | max // empty' "$body" 2>"$jq_err"); then
    printf 'catalog lower-bound check: %s returned HTTP 200 but the body is NOT PARSEABLE JSON — corrupt or truncated. jq said: %s\n' \
      "$url" "$(catalog_lb_log_safe "$(cat "$jq_err")")" >&2
    rm -f "$body" "$jq_err"
    return 14
  fi
  rm -f "$body" "$jq_err"

  # Nothing numeric, non-negative and integral anywhere in entries[].version.
  if [ -z "$bound" ]; then
    printf 'catalog lower-bound check: %s parsed, but it carries no usable entries[].version at all — every entry is missing one, or is a string, null, object, float or negative. A published index with no usable version is a BROKEN index, not an absent one, so this is fatal rather than "no lower bound".\n' \
      "$url" >&2
    return 15
  fi

  # `catalog_lb_plain_u64`, not a digits-only `case`. A digits-only test accepts a bound of ANY
  # length, and every value above 2^63-1 then made `[ -le ]` error rather than compare — see the
  # long note on `catalog_lb_num_le`. This also refuses a leading zero, whose meaning differs
  # between `[` and `[[ ]]`, and jq's `1E+20` spelling for a large literal.
  if ! catalog_lb_plain_u64 "$bound"; then
    printf 'catalog lower-bound check: %s parsed, and its largest usable entries[].version is [%s] — outside the range a CatalogEntry.version can hold (a u64: 0 to %s), or not a plain decimal spelling of it. No client could hold this number, so the index is BROKEN rather than merely ahead, and it is refused rather than compared.\n' \
      "$url" "$(catalog_lb_log_safe "$bound")" "$CATALOG_LB_U64_MAX" >&2
    return 17
  fi

  printf '%s\n' "$bound"
}

# catalog_lower_bound_check <candidate> <owner/repo>
#   The fatal gate the release job calls. Returns 0 only when <candidate> is strictly greater than
#   the published bound, or when there is positively no published catalog to be newer than.
#     2 = <candidate> is not a plain decimal integer that fits a u64
#     3 = <candidate> is NOT STRICTLY NEWER than the published catalog version — the bug
#     (every other code is passed straight through from catalog_published_lower_bound)
catalog_lower_bound_check() {
  local candidate="${1-}" repo="${2-}"
  # Same discipline as the bound side, and for the same reason: the candidate operand overflows
  # `[ -le ]` identically. Measured on round 1 of #1091 —
  #   candidate 9223372036854775808 vs bound 1787200000 -> `[: integer expected` and exit 0.
  if ! catalog_lb_plain_u64 "$candidate"; then
    printf 'catalog lower-bound check needs a plain decimal candidate version with no leading zero, no greater than %s (the u64 a CatalogEntry.version is), got: %s\n' \
      "$CATALOG_LB_U64_MAX" "${candidate:-<empty>}" >&2
    return 2
  fi
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

  # `<=`, not `<`. At EQUALITY a client answers `AlreadyCurrent` and writes nothing, so a
  # strictly-less comparison would let a release publish that reaches no user — measured through the
  # real engine in sidecar/host/tests/catalog_offtip_release_lower_bound.rs
  # (`the_clients_acceptance_boundary_is_strictly_greater_than_the_installed_version`).
  # Red-proofed 2026-08-28: changing this call to `catalog_lb_num_le "$bound" "$candidate" ||` (the
  # strictly-less spelling) reds "a version EQUAL to the published one is refused too" in
  # src/lib/catalogPublishLowerBound.test.ts.
  #
  # `catalog_lb_num_le`, NOT `[ "$candidate" -le "$bound" ]` and NOT `[[ ]]`. Read that function's
  # header before touching this line: the `[` spelling is fail-OPEN above 2^63-1 (it ERRORS, and an
  # `if` cannot tell an error from a false), and the `[[ ]]` spelling both wraps silently AND
  # arithmetic-evaluates these remote-influenced operands, which is command execution.
  if catalog_lb_num_le "$candidate" "$bound"; then
    local remedy='This is what a release cut from an OLDER commit looks like: a hotfix off a maintenance branch, a revert branch, or `git tag` on a non-tip commit (CPE-1951). Re-cut the tag from a commit newer than the one already released.'
    if [ "$candidate" = "$bound" ]; then
      # Equality has a second, much more likely cause than an off-tip tag, and the off-tip advice is
      # actively wrong for it: RE-RUNNING the catalog job against a release that has already been
      # published. `latest` then resolves to that same release, so the candidate is comparing
      # against itself and can never be strictly newer. That is exactly the repair path #1062 needs,
      # so it gets its own sentence rather than being told to re-cut a tag that is fine.
      remedy='The two numbers are EQUAL, which most often means this job is being RE-RUN against a release that is already published — `latest` then resolves to that same release, so the candidate is being compared with itself and can never be strictly newer. If you are repairing an upload on an already-published release (the #1062 case), do NOT re-cut the tag: run `catalog-sign` and `gh release upload` against that release directly, or publish the repair as a new release. If this is a genuinely new release, its tag was cut from a commit no newer than the released one — re-cut it from a newer commit.'
    fi
    printf '::error::catalog version %s is NOT NEWER than the version %s already published on %s'"'"'s latest release. Publishing it would be fully green here and then be refused by EVERY client as a rollback (ApplyOutcome::Rollback), silently, forever — nobody'"'"'s agent roster would ever update again and nothing would be logged as a release failure. %s\n' \
      "$candidate" "$bound" "$repo" "$remedy" >&2
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
