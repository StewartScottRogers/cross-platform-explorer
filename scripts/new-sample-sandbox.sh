#!/usr/bin/env bash
# Copy the pristine samples/ tree into a fresh git-ignored .sandbox/<timestamp>/ directory (CPE-1042),
# so you can edit copies (e.g. save tag changes in the Metadata Studio) without touching the baseline.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
src="$repo/samples"
[ -d "$src" ] || { echo "samples/ not found at $src" >&2; exit 1; }

stamp="$(date +%Y%m%d-%H%M%S)"
dest="$repo/.sandbox/$stamp"
mkdir -p "$dest"
cp -R "$src/." "$dest/"
# This copy is meant to be edited — drop the baseline's "pristine" README.
rm -f "$dest/README.md"

echo "Sandbox ready (edit freely — it's git-ignored):"
echo "  $dest"
