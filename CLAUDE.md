# TerritoRide – instructions for Claude Code

PROGRESS.md is the handoff between sessions; DECISIONS.md records choices.

## Branches
- `main` is the only long-lived branch and the GitHub default. What is on `main` is what runs on the Pi:
  `just deploy` runs from an up-to-date `main` only. Releases are tags `vX.Y.Z` on `main`.
- **Releases (do this properly).** Every change that reaches the Pi is released first; nothing is deployed from an untagged commit, except content-only commits (question bank, content) and docs/CI-only changes, which need no release.
  1. Add the changes under `## Unreleased` in CHANGELOG.md as you work (user-visible features also get their `data/whatsnew.json` note, if the repo has one).
  2. Pick the version: patch = fixes and internal changes, minor = features, major = breaking.
  3. PR "Release vX.Y.Z": bump `Cargo.toml` and the lock file, rename `Unreleased` to `vX.Y.Z (date)`, add an empty `Unreleased`. Merge it.
  4. On up-to-date `main`: `git tag -a vX.Y.Z -m "vX.Y.Z"` then `git push origin vX.Y.Z`. `release.yml` publishes the GitHub Release from the CHANGELOG section.
  5. Check `gh release list` shows vX.Y.Z as Latest and the `release` run is green, then `just deploy` (the image must be named exactly `vX.Y.Z`: `git describe --tags` prints no `-N-g...` suffix).
  Never retag, move or delete a tag or release: fix forward with the next patch. Log the release and deploy in PROGRESS.md if the repo has one.
- One branch per issue: `feature/N-short-slug` from an up-to-date `origin/main`, PR into `main` with
  `Closes #N` (the issue closes on merge). CI runs on every PR and on pushes to `main`.
- Never force-push `main`; never delete a branch with an open PR.
