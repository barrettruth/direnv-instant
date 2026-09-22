# Contributing to direnv-instant

Thank you for your interest in contributing to direnv-instant! This guide will help you get started with building, testing, and contributing to the project.

## Prerequisites

### Required
- **Rust toolchain** (1.84+): Install via [rustup](https://rustup.rs/)
- **Direnv**: Required for testing functionality

### Optional but Recommended
- **Nix with flakes enabled**: For reproducible builds and running integration tests
- **Tmux or Zellij**: For testing multiplexer integration features

## Building the Project

### Using Cargo (Standard Rust Build)

```bash
# Debug build
cargo build

# Release build
cargo build --release

# The binary will be in target/debug/direnv-instant or target/release/direnv-instant
```

### Using Nix (Reproducible Build)

```bash
# Build the package
nix build

# The result will be symlinked as ./result/bin/direnv-instant

# Enter development shell with all dependencies
nix develop
```

## Running Tests

### Integration Tests

The project uses Python-based integration tests with pytest. These tests verify the end-to-end behavior of direnv-instant.

#### Using Nix (Recommended)

```bash
# Run all tests
nix build .#checks.$(nix eval --raw --impure --expr builtins.currentSystem).tests

# Or run tests individually in the dev shell
nix develop
pytest tests/ -v

# Run tests in parallel
pytest tests/ -v -n auto
```

#### Using pytest directly

```bash
# Install test dependencies
pip install pytest pytest-xdist

# Set the binary path
export DIRENV_INSTANT_BIN="$(pwd)/target/debug/direnv-instant"

# Build the project first
cargo build

# Run all tests
pytest tests/ -v

# Run specific test file
pytest tests/test_slow_direnv_exports_via_tmux.py -v

# Run tests in parallel
pytest tests/ -v -n auto
```

**Note**: Some tests require tmux or zellij to be installed and will be skipped if not available.

### Running All Checks

Using Nix, you can run all checks (build, tests, formatting) at once:

```bash
nix flake check
```

## Code Quality

### Formatting

The project uses multiple formatters managed via treefmt:

```bash
# Using Nix (runs rustfmt, nixfmt, and ruff)
nix fmt

# Or manually:
cargo fmt              # Format Rust code
ruff format tests/     # Format Python tests
```

### Linting

```bash
# Check Rust code with clippy
cargo clippy -- -D warnings

# Check Python tests with ruff
ruff check tests/
```

## Development Workflow

1. **Fork and clone the repository**
   ```bash
   git clone https://github.com/your-username/direnv-instant.git
   cd direnv-instant
   ```

2. **Create a feature branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

3. **Make your changes**
   - Write code following the existing style
   - Add tests for new functionality
   - Ensure all tests pass

4. **Format and lint your code**
   ```bash
   nix fmt              # Or cargo fmt
   cargo clippy
   ```

5. **Run tests**
   ```bash
   cargo build
   pytest tests/ -v
   ```

6. **Commit your changes**
   ```bash
   git add .
   git commit -m "Brief description of changes"
   ```

7. **Push and create a pull request**
   ```bash
   git push origin feature/your-feature-name
   ```

## Debugging

### Enable Debug Logging

Set the `DIRENV_INSTANT_DEBUG_LOG` environment variable to redirect daemon output to a log file:

```bash
export DIRENV_INSTANT_DEBUG_LOG="/tmp/direnv-instant-debug.log"

# Then run your shell and check the log
tail -f /tmp/direnv-instant-debug.log
```

## Adding New Features

When adding new features:

1. **Consider the user experience**: direnv-instant should be fast and non-intrusive
2. **Add tests**: Integration tests should cover the new functionality
3. **Update documentation**: Modify README.md if user-facing changes are made
4. **Handle errors gracefully**: Use appropriate error messages with the `direnv-instant:` prefix
5. **Consider multiplexer support**: If relevant, ensure features work with both tmux and zellij

## Common Issues

### Unix Socket Path Too Long

On macOS, Unix socket paths have a 104-byte limit. Tests automatically use `/tmp` to avoid this issue. If you encounter socket path errors during development:

```bash
export TMPDIR=/tmp
```

## Questions or Need Help?

- Open an issue on GitHub for bugs or feature requests
- Check existing issues and pull requests for similar discussions
- For security issues, please report privately to the maintainers

## License

By contributing to direnv-instant, you agree that your contributions will be licensed under GPL-3.0.
