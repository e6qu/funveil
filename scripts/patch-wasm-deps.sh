#!/usr/bin/env bash
# Download tree-sitter-markdown-fork source and apply WASM compatibility patch.
# This avoids vendoring third-party code while enabling wasm32-wasip1 builds.
#
# Usage: ./scripts/patch-wasm-deps.sh
# The script creates .patched-deps/ (gitignored) and writes a Cargo patch
# override into .cargo/config.toml.

set -euo pipefail

CRATE="tree-sitter-markdown-fork"
VERSION="0.7.3"
PATCH="patches/${CRATE}-wasm.patch"
DEST=".patched-deps/${CRATE}"

cd "$(git rev-parse --show-toplevel)"

if [ ! -f "$PATCH" ]; then
  echo "error: patch file not found: $PATCH" >&2
  exit 1
fi

echo "Preparing patched ${CRATE} v${VERSION} for WASM build..."

# Download crate source from crates.io
rm -rf "$DEST"
mkdir -p "$DEST"
curl -fsSL --retry 3 -H "User-Agent: funveil-build/1.0" \
  "https://crates.io/api/v1/crates/${CRATE}/${VERSION}/download" \
  | tar xz -C "$DEST" --strip-components=1

# Apply patch
patch -d "$DEST" -p1 < "$PATCH"

# Add Cargo patch override (append to .cargo/config.toml)
mkdir -p .cargo
if ! grep -q "patch.crates-io.${CRATE}" .cargo/config.toml 2>/dev/null; then
  cat >> .cargo/config.toml <<EOF

[patch.crates-io.${CRATE}]
path = "${DEST}"
EOF
fi

echo "Done. Patched source in ${DEST}, Cargo override in .cargo/config.toml"
