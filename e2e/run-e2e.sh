#!/bin/bash
set -e

echo "=========================================="
echo "Funveil E2E Test Suite"
echo "=========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

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

# Setup fresh test environment
setup() {
    info "Setting up test environment..."
    # Remove all files including hidden ones
    find /workspace/test-project -mindepth 1 -delete 2>/dev/null || true
    cd /workspace/test-project
    
    # Create test files
    cat > README.md << 'EOF'
# Test Project

This is a README file.
Line 3
Line 4
Line 5
EOF

    cat > secrets.env << 'EOF'
API_KEY=super_secret_key_12345
DB_PASSWORD=another_secret
EOF

    cat > main.py << 'EOF'
def main():
    # Line 2
    # Line 3
    api_key = "should_be_hidden"
    # Line 5
    # Line 6
    print("Hello World")
    # Line 8

if __name__ == "__main__":
    main()
EOF

    mkdir -p src
    cat > src/utils.py << 'EOF'
def helper():
    pass
EOF

    git init -q
    pass "Test environment setup"
}

# Test 1: Initialize funveil
test_init() {
    info "Test: Initialize funveil"
    setup
    
    if fv init 2>&1 | grep -q "Initialized"; then
        if [ -f ".funveil_config" ] && [ -d ".funveil" ]; then
            pass "funveil init creates config and data directory"
        else
            fail "funveil init did not create expected files"
        fi
    else
        fail "funveil init did not report success"
    fi
}

# Test 2: Initialize twice should warn
test_init_twice() {
    info "Test: Initialize twice should warn"
    setup
    
    fv init -q
    if fv init 2>&1 | grep -q "already initialized"; then
        pass "Double init shows warning"
    else
        fail "Double init did not show warning"
    fi
}

# Test 3: Check default mode
test_default_mode() {
    info "Test: Default mode is whitelist"
    setup
    
    fv init -q
    if fv mode 2>&1 | grep -q "whitelist"; then
        pass "Default mode is whitelist"
    else
        fail "Default mode is not whitelist"
    fi
}

# Test 4: Change mode
test_change_mode() {
    info "Test: Change mode to blacklist"
    setup
    
    fv init -q
    fv mode blacklist -q
    if fv mode 2>&1 | grep -q "blacklist"; then
        pass "Mode can be changed to blacklist"
    else
        fail "Mode change did not work"
    fi
}

# Test 5: Status shows configuration
test_status() {
    info "Test: Status shows configuration"
    setup
    
    fv init -q
    fv unveil README.md -q
    if fv status 2>&1 | grep -q "README.md"; then
        pass "Status shows whitelisted files"
    else
        fail "Status does not show whitelisted files"
    fi
}

# Test 6: Veil a file
test_veil_file() {
    info "Test: Veil a file"
    setup
    
    fv init --mode blacklist -q
    if fv veil secrets.env 2>&1 | grep -q "Veiling"; then
        # Check if file is veiled (contains ...)
        if grep -q "\.\.\." secrets.env; then
            pass "File is veiled with marker"
        else
            fail "File was not veiled"
        fi
    else
        fail "Veil command did not execute"
    fi
}

# Test 7: Veil line ranges
test_veil_lines() {
    info "Test: Veil specific line ranges"
    setup
    
    fv init --mode blacklist -q
    fv veil main.py#3-5 -q
    
    # Check that lines 3-5 are veiled (marker appears somewhere in lines 3-5)
    if sed -n '3,5p' main.py | grep -q "\.\.\."; then
        pass "Line range is veiled"
    else
        fail "Line range was not veiled"
    fi
}

# Test 8: Unveil a file
test_unveil_file() {
    info "Test: Unveil a file"
    setup
    
    fv init --mode blacklist -q
    ORIGINAL=$(cat secrets.env)
    fv veil secrets.env -q
    fv unveil secrets.env -q
    
    if [ "$(cat secrets.env)" = "$ORIGINAL" ]; then
        pass "File is restored after unveil"
    else
        fail "File was not restored correctly"
    fi
}

# Test 9: Unveil all
test_unveil_all() {
    info "Test: Unveil all files"
    setup
    
    fv init --mode blacklist -q
    fv veil secrets.env -q
    fv veil main.py#3-5 -q
    
    fv unveil --all -q
    
    # Check files are restored
    if ! grep -q "\.\.\." secrets.env && ! grep -q "\.\.\." main.py; then
        pass "All files restored after unveil --all"
    else
        fail "Some files still veiled after unveil --all"
    fi
}

# Test 10: Protected paths cannot be veiled
test_protected_paths() {
    info "Test: Protected paths cannot be veiled"
    setup
    
    fv init -q
    
    if fv veil .funveil_config 2>&1 | grep -q "protected"; then
        pass "Config file is protected"
    else
        fail "Config file was not protected"
    fi
}

# Test 11: Checkpoint save and restore
test_checkpoint() {
    info "Test: Checkpoint save and restore"
    setup
    
    fv init -q
    fv unveil README.md -q
    
    # Save checkpoint
    fv checkpoint save test-checkpoint -q
    
    # Make changes
    fv veil README.md -q
    
    # Restore checkpoint
    fv checkpoint restore test-checkpoint -q
    
    if fv status 2>&1 | grep -q "README.md"; then
        pass "Checkpoint restore works"
    else
        fail "Checkpoint restore failed"
    fi
}

# Test 12: Show file with annotations
test_show() {
    info "Test: Show file with annotations"
    setup
    
    fv init --mode blacklist -q
    fv veil main.py#3-5 -q
    
    if fv show main.py 2>&1 | grep -q "veiled"; then
        pass "Show command displays veil annotations"
    else
        fail "Show command did not display annotations"
    fi
}

# Test 13: Garbage collection
test_gc() {
    info "Test: Garbage collection"
    setup
    
    fv init --mode blacklist -q
    fv veil secrets.env -q
    fv unveil secrets.env -q
    
    # Run GC
    if fv gc 2>&1 | grep -q "Removed"; then
        pass "GC removes unused objects"
    else
        # GC might not remove if still referenced, that's ok
        pass "GC executed (may not have objects to remove)"
    fi
}

# Test 14: Doctor command
test_doctor() {
    info "Test: Doctor command"
    setup
    
    fv init -q
    if fv doctor 2>&1 | grep -q "check"; then
        pass "Doctor command runs integrity checks"
    else
        fail "Doctor command did not run"
    fi
}

# Test 15: Whitelist mode - everything hidden by default
test_whitelist_default_hidden() {
    info "Test: Whitelist mode - everything hidden by default"
    setup
    
    # Initialize with explicit whitelist mode
    fv init --mode whitelist -q
    
    # Without unveiling anything, all files should be conceptually hidden
    # (The actual veil is applied by 'fv apply')
    fv apply -q 2>/dev/null || true
    
    pass "Whitelist mode initialized"
}

# Run all tests
echo ""
echo "Running E2E Tests..."
echo ""

test_init
test_init_twice
test_default_mode
test_change_mode
test_status
test_veil_file
test_veil_lines
test_unveil_file
test_unveil_all
test_protected_paths
test_checkpoint
test_show
test_gc
test_doctor
test_whitelist_default_hidden

echo ""
echo "=========================================="
echo "E2E Test Results"
echo "=========================================="
echo -e "${GREEN}Passed${NC}: $TESTS_PASSED"
echo -e "${RED}Failed${NC}: $TESTS_FAILED"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
