COVERAGE_MIN_LINES ?= 90

all: test lint doc

test:
	cargo test

build:
	cargo build --release

check:
	RUSTFLAGS="-D warnings" cargo check

lint: fmt clippy

fmt:
	cargo fmt

clippy:
	cargo clippy --no-deps --all-targets -- -D warnings

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --all --no-deps

bench:
	cargo test --release -- --ignored

coverage:
	cargo llvm-cov --workspace --all-targets \
		--ignore-filename-regex '(^|/)src/bin/' \
		--fail-under-lines $(COVERAGE_MIN_LINES)

clean:
	cargo clean

.PHONY: all test build check lint fmt clippy doc bench coverage clean
