#!/usr/bin/env bash
# Prints the release notes' Checksums section for the named files, taken from the folder's SHA256SUMS.
# Usage: release_checksums.sh <folder> <file>...
set -euo pipefail

folder="$1"
shift

printf '## Checksums\n\n| File | SHA-256 |\n|---|---|\n'
for file in "$@"; do
  hash=$(awk -v name="$file" '$2 == name { print $1 }' "$folder/SHA256SUMS")
  [ -n "$hash" ] || { echo "$file is not in $folder/SHA256SUMS" >&2; exit 1; }
  printf '| `%s` | `%s` |\n' "$file" "$hash"
done
