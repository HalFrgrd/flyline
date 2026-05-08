# Docker building and testing

The goal is to allow the same builds and tests to run locally, with `cargo`, and in GitHub Actions.

For instance, we can easily build the library locally targeting an old glibc version with: `docker buildx bake -f docker/docker-bake.hcl extract-release-artifact`.

Tab completion tests run as plain Rust library tests (no docker required):

- `cargo test --lib tab_completion_tests`

The fixtures the tests cd into live under `tests/example_fs/` and `tests/example_braces_fs/`.
