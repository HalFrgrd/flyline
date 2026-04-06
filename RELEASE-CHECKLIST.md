# Release checklist
1. Make sure the Update Settings Documentation action finishes and merge the PR.
2. Make sure the Copilot Documentation Check action finishes and merge the PR.
3. Make sure the CI action finishes successfully.
4. Make sure the Generate GIF action finishes successfully.
5. Update Cargo.toml to new version. Build locally so Cargo.lock updates.
6. Tag the commit.
7. Push all so the tag and commit are on the remote master.
8. Wait for all actions to have finished.
9. Once the release action has finished, the new release will be marked as pre-release. Manually change it from pre-release to release.
10. If it fails, delete the tag, fix the problem, retag and repeat.