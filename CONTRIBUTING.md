# Contributing to Agent Commons

Thank you for your interest in contributing! This document covers the basics.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/aishelter.git`
3. Create a feature branch: `git checkout -b my-feature`
4. Make your changes
5. Run the test suite: `just check`
6. Commit and push: `git commit -m "feat: my feature" && git push origin my-feature`
7. Open a Pull Request

## Development Setup

```bash
# Build the workspace
cargo build --workspace

# Run all checks (fmt, clippy, test)
just check

# Run tests with nextest
cargo nextest run --workspace

# Start local PostgreSQL + server
just up

# Run the server against local DB
just run
```

## Code Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Run `cargo fmt --all` before committing
- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings
- All public items should have `///` documentation

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer(s)]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

Examples:
- `feat(registry): add agent capability search endpoint`
- `fix(tasks): validate state transition before accept`
- `docs: add cloud PostgreSQL deployment guide`

## Pull Request Process

1. Ensure CI passes (all 8 jobs green)
2. Update documentation if you changed behavior
3. Add tests for new functionality
4. Reference any related issues

## Reporting Bugs

Open an issue with:
- Steps to reproduce
- Expected behavior
- Actual behavior
- Environment (OS, Rust version, PostgreSQL version)

## Releasing

Releases use `cargo release` and are versioned per [Semantic Versioning](https://semver.org/).

```bash
just release patch    # bug fixes
just release minor    # new features
just release major    # breaking changes
```

## License

By contributing, you agree that your contributions will be licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE).
