# Fix: `dcx exec` lands in `/workspace` instead of original workspace path

## Parts
1. [x] `src/up.rs` — Add RAII TempFile guard, json_escape helper, and override-config JSON
2. [x] `src/exec.rs` — Add `--workspace-folder` to build_exec_args + run_exec
3. [x] `specs/architecture.md` — Fix data flow diagram and steps 13, 8-9
4. [x] `tests/e2e/test_dcx_exec.sh` — Assert pwd == original workspace path
5. [x] Run `make check` to verify all tests pass
