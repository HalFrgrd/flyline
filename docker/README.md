# Docker building and testing

The approach is to allow the same builds and tests to be run locally, with `cargo` integration tests, and in GitHub Actions.

For instance, we can easily build the library locally targeting an old glibc version with: `docker buildx bake -f docker/docker-bake.hcl extract-artifact`.

Or we can run the tab completion test:
- locally with `docker buildx bake -f docker/docker-bake.hcl tab-completion-tests`
- locally via `cargo` with `cargo test --test tab_completions_tests` or simply `cargo test`
- or in GitHub Actions (see `.github/workflows/ci.yml`)
