#!/usr/bin/env bash
set -euo pipefail

# Validate that README.md contains well-formed shields.io badges with trailing markers.
#
# Expected format (single line):
#   [![Label](https://img.shields.io/badge/ENCODED-VALUE-COLOR)](URL) <!-- badge:NAME -->

readme="README.md"
if [ ! -f "$readme" ]; then
  echo "README.md not found"
  exit 1
fi

errors=0

for tag in coverage tests loc test-loc; do
  line=$(grep "<!-- badge:${tag} -->" "$readme" || true)
  if [ -z "$line" ]; then
    echo "ERROR: Missing badge marker <!-- badge:${tag} -->"
    errors=$((errors + 1))
    continue
  fi

  # Validate: [![LABEL](https://img.shields.io/badge/...)](URL) <!-- badge:TAG -->
  if ! echo "$line" | grep -qE '^\[!\[[^]]+\]\(https://img\.shields\.io/badge/[^)]+\)\]\([^)]+\) <!-- badge:'"${tag}"' -->$'; then
    echo "ERROR: Badge '${tag}' has malformed format"
    echo "  Found: $line"
    echo "  Expected: [![Label](https://img.shields.io/badge/...)](URL) <!-- badge:${tag} -->"
    errors=$((errors + 1))
  fi
done

if [ "$errors" -gt 0 ]; then
  echo "${errors} badge format error(s) found"
  exit 1
fi
