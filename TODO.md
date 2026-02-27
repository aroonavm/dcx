# Network Mode Fix - Implementation Plan

## Part 1: Fix `src/down.rs` (early-exit logic)
- [ ] Replace lines 74-80: Don't exit early if only mount is missing; check for containers too
- [ ] Only print "nothing to do" if BOTH mount AND container are gone
- [ ] Reuse `containers` result from step 6 in step 7

## Part 2: Fix `src/up.rs` (network mode enforcement)
- [ ] Add step 9.5 after the `mounted_fresh` block (before step 10)
- [ ] Check if existing container's network mode matches requested mode
- [ ] Remove mismatched containers before `devcontainer up`

## Part 3: Update `specs/architecture.md`
- [ ] Update `dcx down` step 4 description
- [ ] Add `dcx up` step 9.5 description

## Part 4: Add E2E test
- [ ] Create `tests/e2e/test_network_mode_switch.sh`
- [ ] Test normal network mode switch (restricted → down → open)
- [ ] Test FUSE crash scenario (container survives, dcx down removes it)

## Part 5: Verify
- [ ] Run `make check` to ensure all tests pass
