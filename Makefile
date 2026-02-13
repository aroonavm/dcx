.PHONY: check test lint fmt build

check: test lint fmt

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt -- --check

build:
	cargo build
