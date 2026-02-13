#!/usr/bin/env bash
# E2E tests for `dcx doctor`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx doctor ==="

# --- All checks pass in fully provisioned environment ---
echo "--- all checks pass ---"
out=$("$DCX" doctor 2>/dev/null)
code=$?
assert_exit "doctor exits 0 in full environment" 0 "$code"
assert_contains "doctor shows Checking prerequisites" "$out" "Checking prerequisites..."
assert_contains "doctor shows All checks passed" "$out" "All checks passed."

# --- Output contains all expected checks ---
echo "--- output format ---"
assert_contains "doctor checks bindfs" "$out" "bindfs installed"
assert_contains "doctor checks devcontainer" "$out" "devcontainer CLI installed"
assert_contains "doctor checks Docker" "$out" "Docker available"
assert_contains "doctor checks Colima" "$out" "Colima running"
assert_contains "doctor checks unmount tool" "$out" "Unmount tool available"
assert_contains "doctor checks relay exists" "$out" ".colima-mounts exists on host"
assert_contains "doctor checks relay in VM" "$out" ".colima-mounts mounted in VM"

# --- All check marks are ✓ ---
echo "--- check marks ---"
FAIL_COUNT=$(echo "$out" | grep -c "✗" || true)
[ "$FAIL_COUNT" -eq 0 ] && pass "no failing checks (✗) in full environment" || fail "found $FAIL_COUNT failing checks: $out"

# --- Exit code semantics: 0 on pass ---
echo "--- exit code 0 on pass ---"
"$DCX" doctor >/dev/null 2>/dev/null
assert_exit "doctor exits 0" 0 $?

# --- Progress output on stderr ---
echo "--- progress output ---"
stderr_out=$("$DCX" doctor 2>&1 >/dev/null)
assert_contains "doctor shows progress step" "$stderr_out" "→"

summary
