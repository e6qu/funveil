#!/bin/bash
set -e

echo "=========================================="
echo "Funveil gVisor Smoke Tests"
echo "=========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Image to test (default: ghcr.io/e6qu/funveil:latest)
IMAGE="${1:-ghcr.io/e6qu/funveil:latest}"

# Temp workspace for volume mounts
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

# Helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((TESTS_PASSED++)) || true
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    ((TESTS_FAILED++)) || true
}

info() {
    echo -e "${YELLOW}→${NC} $1"
}

section() {
    echo ""
    echo -e "${BLUE}==========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}==========================================${NC}"
}

run_fv() {
    docker run --runtime=runsc --rm \
        -v "$WORKDIR:/workspace" -w /workspace \
        "$IMAGE" "$@"
}

section "BASIC COMMANDS"

# Test 1: fv --help
info "Test: fv --help"
OUTPUT=$(run_fv --help 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ] && echo "$OUTPUT" | grep -qi "Funveil"; then
    pass "fv --help exits 0 and contains 'Funveil'"
else
    fail "fv --help (rc=$RC, output: $OUTPUT)"
fi

# Test 2: fv --version
info "Test: fv --version"
OUTPUT=$(run_fv --version 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ] && echo "$OUTPUT" | grep -qE '[0-9]+\.[0-9]+'; then
    pass "fv --version exits 0 and contains version string"
else
    fail "fv --version (rc=$RC, output: $OUTPUT)"
fi

# Test 3: fv version
info "Test: fv version"
OUTPUT=$(run_fv version 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ] && echo "$OUTPUT" | grep -qiE 'commit|target|profile'; then
    pass "fv version shows build info"
else
    fail "fv version (rc=$RC, output: $OUTPUT)"
fi

section "WORKFLOW COMMANDS"

# Test 4: fv init --mode blacklist
info "Test: fv init --mode blacklist"
rm -rf "${WORKDIR:?}"/*  "${WORKDIR:?}"/.[!.]* 2>/dev/null || true
git -C "$WORKDIR" init -q
OUTPUT=$(run_fv init --mode blacklist 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ]; then
    pass "fv init --mode blacklist exits 0"
else
    fail "fv init --mode blacklist (rc=$RC, output: $OUTPUT)"
fi

# Test 5: fv status
info "Test: fv status"
OUTPUT=$(run_fv status 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ] && echo "$OUTPUT" | grep -qi "mode"; then
    pass "fv status shows mode"
else
    fail "fv status (rc=$RC, output: $OUTPUT)"
fi

# Test 6: fv veil <file>
info "Test: fv veil <file>"
echo "SECRET_KEY=abc123" > "$WORKDIR/secrets.env"
OUTPUT=$(run_fv veil secrets.env 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ]; then
    pass "fv veil secrets.env exits 0"
else
    fail "fv veil secrets.env (rc=$RC, output: $OUTPUT)"
fi

# Test 7: fv unveil <file>
info "Test: fv unveil <file>"
OUTPUT=$(run_fv unveil secrets.env 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ]; then
    pass "fv unveil secrets.env exits 0"
else
    fail "fv unveil secrets.env (rc=$RC, output: $OUTPUT)"
fi

# Test 8: fv parse <file>
info "Test: fv parse <file>"
cat > "$WORKDIR/main.rs" << 'RUST'
fn main() {
    println!("Hello");
}

fn helper() {
    println!("Help");
}
RUST
OUTPUT=$(run_fv parse main.rs 2>&1) && RC=$? || RC=$?
if [ "$RC" -eq 0 ] && echo "$OUTPUT" | grep -qi "Functions\|main\|helper"; then
    pass "fv parse main.rs shows parsed output"
else
    fail "fv parse main.rs (rc=$RC, output: $OUTPUT)"
fi

# Results
echo ""
echo "=========================================="
echo "gVisor Smoke Test Results"
echo "=========================================="
echo -e "Image: ${BLUE}${IMAGE}${NC}"
echo -e "${GREEN}Passed${NC}: $TESTS_PASSED"
echo -e "${RED}Failed${NC}: $TESTS_FAILED"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All gVisor smoke tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some gVisor smoke tests failed!${NC}"
    exit 1
fi
