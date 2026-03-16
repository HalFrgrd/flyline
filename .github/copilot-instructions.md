# Flyline – Copilot Instructions

## Project Overview

Flyline is a Bash plugin written in Rust that replaces readline to provide a code-editor-like command-line experience. It is compiled as a shared library (`libflyline.so`) and loaded into Bash with `enable -f /path/to/libflyline.so flyline`.

Key features: undo/redo, cursor animations, fuzzy history suggestions, fuzzy autocompletions, bash autocomplete integration, mouse support, syntax highlighting, and tooltips.

## Repository Layout

```
src/            Rust library source (cdylib crate)
  lib.rs        Entry point; exports C symbols consumed by Bash
  app/          TUI application logic (ratatui-based)
  *.rs          Individual feature modules
tests/          Rust integration tests (run inside Docker)
docker/         Dockerfiles and helper scripts used by CI
  docker-bake.hcl              Bake file defining all build targets
  integration_test_build.Dockerfile  Multi-stage build; produces libflyline.so
  bash_integration_test.Dockerfile   Loads the .so into various Bash versions
Cargo.toml      Rust manifest (edition 2024, cdylib crate type)
```

## How to Build

The library is built with Cargo inside Docker to target glibc 2.23 (Ubuntu 16.04), ensuring broad host compatibility:

```bash
docker buildx bake -f docker/docker-bake.hcl extract-artifact
# Produces docker/build/libflyline.so
```

For a quick local (host-native) build during development:

```bash
cargo build --release
```

## How to Run Tests

**Unit/library tests** (run inside the Ubuntu 16.04 Docker build):

```bash
docker buildx bake -f docker/docker-bake.hcl lib-tests
```

**Bash integration tests** (load `libflyline.so` into real Bash builds):

Don't run these unless specified.

```bash
docker buildx bake -f docker/docker-bake.hcl bash-integration-tests
```

Supported `BASH_VERSION` values: `4.4-rc1`, `4.4.18`, `5.0`, `5.1.16`, `5.2`, `5.3`.

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
