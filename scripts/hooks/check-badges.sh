#!/usr/bin/env bash
set -euo pipefail

# Validate that README.md badge markers exist and the line after each
# contains a well-formed shields.io badge.
#
# Expected format (two lines):
#   <!-- badge:NAME -->
#   [![Label](https://img.shields.io/badge/ENCODED-VALUE-COLOR)](URL)

readme="README.md"
if [ ! -f "$readme" ]; then
  echo "README.md not found"
  exit 1
fi

errors=0

for tag in coverage tests loc test-loc; do
  # Find the line number of the marker comment
  marker_line=$(grep -n "<!-- badge:${tag} -->" "$readme" | head -1 | cut -d: -f1)
  if [ -z "$marker_line" ]; then
    echo "ERROR: Missing badge marker <!-- badge:${tag} -->"
    errors=$((errors + 1))
    continue
  fi

  # Check the marker line is ONLY the comment (no badge on same line)
  marker_content=$(sed -n "${marker_line}p" "$readme")
  if [ "$marker_content" != "<!-- badge:${tag} -->" ]; then
    echo "ERROR: Badge '${tag}' marker line must contain only the comment"
    echo "  Found: $marker_content"
    errors=$((errors + 1))
    continue
  fi

  # Get the next line (the actual badge)
  badge_line_num=$((marker_line + 1))
  badge_content=$(sed -n "${badge_line_num}p" "$readme")

  # Validate badge format: [![LABEL](https://img.shields.io/badge/...)](URL)
  if ! echo "$badge_content" | grep -qE '^\[!\[[^]]+\]\(https://img\.shields\.io/badge/[^)]+\)\]\([^)]+\)$'; then
    echo "ERROR: Badge '${tag}' (line ${badge_line_num}) has malformed format"
    echo "  Found: $badge_content"
    echo "  Expected: [![Label](https://img.shields.io/badge/...)](URL)"
    errors=$((errors + 1))
  fi
done

if [ "$errors" -gt 0 ]; then
  echo "${errors} badge format error(s) found"
  exit 1
fi
