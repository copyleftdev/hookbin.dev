.PHONY: build test check fmt clippy run clean release

# Build debug binary
build:
	cargo build

# Build release binary
release:
	cargo build --release

# Run all tests
test:
	cargo test -- --nocapture

# Format code
fmt:
	cargo fmt --all

# Check formatting
fmt-check:
	cargo fmt --all -- --check

# Run clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# Run the server (debug)
run:
	cargo run -- serve --port 8080

# Full pre-commit check
check: fmt-check clippy test
	cargo build

# Clean build artifacts
clean:
	cargo clean

# Binary size check
size:
	@cargo build --release 2>/dev/null
	@ls -lh target/release/hookbin | awk '{print "Binary size:", $$5}'

# Run with sample data directory
dev:
	mkdir -p ./data
	cargo run -- serve --port 8080 --data ./data
