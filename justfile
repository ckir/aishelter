# Justfile for Agent Commons (aishelter)

# Default: run all checks
default:
    just check

# Build the project
build:
    cargo build --workspace

# Run all tests with nextest
test:
    cargo nextest run --workspace

# Run tests with verbose output
test-verbose:
    cargo nextest run --workspace --no-fail-fast

# Run clippy lints
clippy:
    cargo clippy --workspace -- -D warnings

# Format check
fmt-check:
    cargo fmt --check

# Format all code
fmt:
    cargo fmt --all

# Full check: fmt + clippy + test
check: fmt-check clippy test

# Run the server locally (requires PostgreSQL)
run:
    cargo run -p ac-server --bin agent-commons

# Initialize the database (run migrations)
init-db:
    sqlx migrate run --source migrations

# Generate changelog
changelog:
    git-cliff --output CHANGELOG.md

# Start docker-compose (PostgreSQL + Commons)
up:
    docker compose up -d

# Stop docker-compose
down:
    docker compose down

# Reset docker volumes
reset:
    docker compose down -v
    docker compose up -d

# Run the end-to-end demo test
demo:
    cargo nextest run --workspace e2e

# Clean build artifacts
clean:
    cargo clean

# Release: bump version, tag, and commit all crates in lockstep
# Usage: just release <patch|minor|major>
release VERSION_BUMP:
    cargo release {{VERSION_BUMP}} --workspace --execute

# Check for typos across all files
typos:
    typos
