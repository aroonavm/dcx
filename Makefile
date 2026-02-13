.PHONY: check test lint fmt build e2e

check: test lint fmt

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt -- --check

build:
	cargo build

e2e:
	@for script in tests/e2e/test_*.sh; do \
		echo "--- $$script ---"; \
		bash "$$script"; \
	done
