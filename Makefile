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

clean:
	cargo clean

.PHONY: all test build check lint fmt clippy doc bench clean
