#!/usr/bin/env bash
# CPE-1796: the ONE copy of "what counts as a month-end ffmpeg anchor tag".
#
# BtbN/FFmpeg-Builds (see ffmpeg-pin-freshness.yml's cadence note) keeps two retention classes: ~14
# rolling DAILY autobuilds it prunes on a ~14-day window, and one MONTH-END anchor per month it
# retains indefinitely (verified: an anchor ~23 months old still returns 200 -- CPE-1789's Work
# Log). A tag is a month-end anchor iff the calendar day AFTER its embedded date is the 1st of the
# next month.
#
# This rule used to exist as one inline copy, in ffmpeg-pin-freshness.yml's "Determine current live
# tags to recommend" step. CPE-1796 gave it a second call site -- a CI guard (ci.yml, "ffmpeg pin is
# a month-end anchor") that asserts release-sidecar.yml's CURRENT pin is an anchor, not just
# recommends one after the fact -- and pulled the rule out here rather than typing it twice. A
# second hand-typed copy of a date rule is exactly how this repo got CPE-1795 (a hardcoded filename
# suffix that drifted from the value it was supposed to track): both call sites now `source` this
# file, so there is structurally nothing left to retype or let drift.
#
# is_ffmpeg_anchor_tag <tag>
#   <tag> looks like "autobuild-YYYY-MM-DD-HH-MM" (a BtbN/FFmpeg-Builds release tag). Returns 0 if
#   <tag>'s embedded date is a month-end anchor, 1 if it's a rolling daily -- or if <tag> doesn't
#   match the expected shape at all, which is treated as "not an anchor" rather than an error, since
#   what a non-match MEANS is the caller's call (freshness-check skips it and tries the next live
#   tag; the CI guard fails the pin outright).
is_ffmpeg_anchor_tag() {
  local tag="$1"
  local d next_day
  # Anchored at BOTH ends (review finding): `^autobuild-\K\d{4}-\d{2}-\d{2}` alone only anchors the
  # start, so "autobuild-2026-07-31" (missing the -HH-MM suffix) and
  # "autobuild-2026-07-31-14-10-extra" (an unexpected trailing suffix) both matched and were
  # accepted as valid anchors -- contradicting this function's own docstring, which says a tag not
  # matching the expected shape is "not an anchor". The `(?=-\d{2}-\d{2}$)` lookahead requires the
  # HH-MM suffix to be present and the match to reach the end of the string.
  d=$(printf '%s' "$tag" | grep -oP '^autobuild-\K\d{4}-\d{2}-\d{2}(?=-\d{2}-\d{2}$)' || true)
  [ -z "$d" ] && return 1
  next_day=$(date -u -d "${d} +1 day" +%d 2>/dev/null || true)
  [ "$next_day" = "01" ]
}

# Allow running this file directly too (not just sourcing it), for a quick manual check:
#   bash .github/workflows/scripts/ffmpeg-anchor-check.sh autobuild-2026-07-31-14-10
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  if [ $# -ne 1 ]; then
    echo "usage: $0 <ffmpeg-autobuild-tag>" >&2
    exit 2
  fi
  if is_ffmpeg_anchor_tag "$1"; then
    echo "anchor"
  else
    echo "daily (or unrecognised tag shape)"
    exit 1
  fi
fi
