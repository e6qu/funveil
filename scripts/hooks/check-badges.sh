#!/usr/bin/env bash
set -euo pipefail

# Validate that README.md badge lines match expected format.
# Each dynamic badge must be: <!-- badge:NAME -->[![Label](https://img.shields.io/badge/...)](URL)

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

  # Validate structure: <!-- badge:TAG -->[![LABEL](https://img.shields.io/badge/ENCODED-VALUE-COLOR)](URL)
  if ! echo "$line" | grep -qE "<!-- badge:${tag} -->\[!\[[^]]+\]\(https://img\.shields\.io/badge/[^)]+\)\]\([^)]+\)"; then
    echo "ERROR: Badge '${tag}' has malformed format"
    echo "  Found: $line"
    echo "  Expected: <!-- badge:${tag} -->[![Label](https://img.shields.io/badge/...)](URL)"
    errors=$((errors + 1))
  fi
done

if [ "$errors" -gt 0 ]; then
  echo "${errors} badge format error(s) found"
  exit 1
fi
