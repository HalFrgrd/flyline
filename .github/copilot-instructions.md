# Flyline – Copilot Instructions

## Project Overview

Flyline is a Bash plugin written in Rust that replaces readline to provide an enhanced line editing experience. It is compiled as a shared library (`libflyline.so`) and loaded into Bash with `enable -f /path/to/libflyline.so flyline`.

Key features: undo/redo, cursor animations, fuzzy history suggestions, fuzzy autocompletions, bash autocomplete integration, mouse support, syntax highlighting, and tooltips.

## Repository Layout

```
src/            Rust library source (cdylib crate)
  lib.rs        Entry point; exports C symbols consumed by Bash
  bash_symbols.rs  FFI declarations for Bash/readline symbols resolved at runtime by Bash
  bash_funcs.rs    Safe-ish wrappers around Bash/readline helpers used by Flyline
  app/          TUI application logic (ratatui-based)
  *.rs          Individual feature modules
tests/          Rust integration tests and shared test helpers
  common/       Shared Rust test support code
docker/         Dockerfiles and helper scripts used by CI
  docker-bake.hcl                Bake file defining all build and test targets
  builder.Dockerfile             Multi-stage build; produces libflyline.so and runs Docker-based lib tests
  bash_integration_test.Dockerfile  Loads the .so into various Bash versions
  specific_bash_version.Dockerfile  Builds the Bash versions used by the integration-test matrix
.github/workflows/ci.yml  CI entrypoint for Docker builds and test matrix jobs
Cargo.toml      Rust manifest (edition 2024, cdylib crate type)
```

## How to Build

For a quick local (host-native) build during development:

```bash
cargo build --release
```

When changing code that is used by the integration tests, also verify that the crate still compiles with the integration-tests feature enabled:

```bash
cargo build --features integration-tests
```

`src/bash_symbols.rs` declares extern Bash/readline symbols that are provided by
Bash when `libflyline.so` is loaded with `enable -f ... flyline`. Those symbols
are not available when Rust links a normal unit-test binary, so test code must
avoid pulling them in directly. `cargo build --features integration-tests`
checks that the non-test code paths used by the integration-test feature still
compile even though `cargo test --lib` uses `#[cfg(test)]` fallbacks in some
places to avoid requiring Bash-owned symbols at unit-test link time.

CI also builds the library inside Docker to target glibc 2.23 (Ubuntu 16.04), ensuring broad host compatibility:

```bash
docker buildx bake -f docker/docker-bake.hcl extract-release-artifact
# Produces docker/build/libflyline.so
```

## How to Run Tests

**Unit/library tests**:

```bash
cargo test --lib
```

Always try `cargo test --lib` for unit-test coverage, and also try
`cargo build --features integration-tests` after changes that touch code shared
with integration tests. The unit-test build alone is not enough because
`#[cfg(test)]` shims can hide linkage-sensitive code paths that depend on Bash
symbols.

**Bash integration tests** (load `libflyline.so` into real Bash builds):

Don't run these unless specified.

```bash
docker buildx bake -f docker/docker-bake.hcl bash-integration-tests
```

Supported `DOCKER_BASH_VERSION` values: `4.4-rc1`, `4.4.18`, `5.0`, `5.1.16`, `5.2`, `5.3`.

CI runs both test suites via `.github/workflows/ci.yml`.

## Coding Conventions

- **Rust edition 2024** — use current idioms (`&raw mut`, `c"..."` literals, etc.).
- The crate is a `cdylib`; all public C symbols must be marked `#[unsafe(no_mangle)]`.
- Feature logic is split into focused single-responsibility modules under `src/`.
- Use `log::` macros (`log::trace!`, `log::debug!`, `log::info!`, `log::warn!`, `log::error!`) for all diagnostic output; never use `println!` for debug messages.
- Prefer `anyhow::Result` for fallible functions.
- Keep `unsafe` blocks as small as possible and document why each one is necessary.
- Do not introduce new dependencies without a clear justification; check the advisory database for known vulnerabilities before adding any.
- Always run `cargo fmt` before committing code.
- In Rust tests, do not add custom failure messages to `assert!` calls (e.g. avoid `assert!(x, "expected …")`). Use plain `assert!(x)` to match the existing test style in this codebase.
