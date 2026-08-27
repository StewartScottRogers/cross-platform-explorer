#!/usr/bin/env bash
# CPE-1893: the ONE copy of "how old is the published agent catalog, and is that too old".
#
# catalog-sign (sidecar/host/src/bin/catalog_sign.rs, invoked by release.yml's `catalog` job) stamps
# every catalog-index.json entry's `version` field with `date +%s` at sign time (CPE-372's
# anti-rollback counter doubles as a wall-clock publish timestamp for free — no second field to add
# or let drift). That means "how old is the live catalog" is just `now - version`, no extra
# provenance needed. This file is sourced by catalog-freshness.yml so the arithmetic exists in one
# place, is directly runnable for a local dry run, and needs no live GitHub Actions run to verify
# (see catalog-freshness.yml's header comment for the chosen threshold + cadence and why).
#
# catalog_age_days <published_epoch> [now_epoch]
#   Whole days between <published_epoch> and [now_epoch] (defaults to `date +%s`; override only for
#   local/CI testing so this never depends on wall-clock time to be reproducible). CAN be negative
#   — see is_catalog_stale below for why that is never silently treated as healthy.
catalog_age_days() {
  local published="$1"
  local now="${2:-$(date +%s)}"
  echo $(( (now - published) / 86400 ))
}

# is_catalog_stale <published_epoch> <threshold_days> [now_epoch]
#   A 3-way verdict via exit code — 0 = stale, 1 = fresh, 2 = CLOCK SKEW:
#   - stale:  strictly older than <threshold_days>.
#   - fresh:  0 <= age_days <= threshold_days. Strictly-older-than (not >=) so a catalog signed
#             exactly `threshold_days` ago — e.g. by a scheduled run landing a few seconds after the
#             boundary — is not flagged on a technicality.
#   - clock skew (exit 2): age_days is NEGATIVE, i.e. the catalog's stamped `version` is in the
#     FUTURE relative to `now`. Reviewer-caught red-proof (CPE-1893 UAT round 1): a naive
#     `[ age -gt threshold ]` comparison lets a negative age slip through as "fresh" (technically not
#     greater than the threshold) — reporting a nonsensical value as healthy. This is a distinct,
#     third verdict precisely so a caller can never collapse it into "fresh": a future-dated version
#     means either the signing runner's clock was wrong at sign time, or catalog-sign itself
#     mis-stamped the version — either way, a real problem worth surfacing, not something to
#     reassure anyone about. Low severity in practice (the timestamp is CI-stamped `date +%s`, not
#     attacker-influenced), but the arithmetic must still say what happened rather than paper over it.
is_catalog_stale() {
  local published="$1" threshold_days="$2" now="${3:-$(date +%s)}"
  local age_days
  age_days=$(catalog_age_days "$published" "$now")
  if [ "$age_days" -lt 0 ]; then
    return 2
  fi
  [ "$age_days" -gt "$threshold_days" ]
}

# Allow running this file directly too (not just sourcing it), for a quick manual/red-proof check:
#   bash .github/workflows/scripts/catalog-freshness-check.sh <published_epoch> <threshold_days> [now_epoch]
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  if [ $# -lt 2 ]; then
    echo "usage: $0 <published_epoch> <threshold_days> [now_epoch]" >&2
    exit 2
  fi
  published="$1" threshold_days="$2" now="${3:-$(date +%s)}"
  age_days=$(catalog_age_days "$published" "$now")
  set +e
  is_catalog_stale "$published" "$threshold_days" "$now"
  verdict=$?
  set -e
  case "$verdict" in
    2)
      echo "CLOCK SKEW — age ${age_days}d is negative (published epoch is after now epoch: a clock" \
           "skew or a mis-stamped catalog version) -- NOT treated as fresh"
      exit 2
      ;;
    0)
      echo "STALE — age ${age_days}d > threshold ${threshold_days}d"
      exit 1
      ;;
    *)
      echo "fresh — age ${age_days}d <= threshold ${threshold_days}d"
      ;;
  esac
fi
