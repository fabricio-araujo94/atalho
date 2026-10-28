# Atalho task runner

# Default task lists all available tasks
default:
    @just --list

# Build all workspace crates in debug mode
build:
    cargo build --workspace

# Run all tests across the entire workspace
test:
    cargo test --workspace

# Run clippy linter with strict warning denials
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format all workspace code using rustfmt
fmt:
    cargo fmt

# Check code formatting without applying changes
fmt-check:
    cargo fmt --check

# Run the Atalho desktop application
run:
    cargo run -p app
