# TerritoRide – instructions for Claude Code

PROGRESS.md is the handoff between sessions; DECISIONS.md records choices.

## Branches (gitflow)
- `main`: GitHub default branch, released code only, what runs on the Pi. Every merge into it is tagged
  `vX.Y.Z`. Never commit or open a feature PR against it. `just deploy` runs from an up-to-date `main`
  only. CI runs on every PR (any base) and on pushes to `main`.
- `develop`: integration branch. Issues close when their `Closes #N` reaches `main` with a release.
- `feature/N-short-slug` (N = issue number): from `origin/develop`, PR into `develop`.
- `release/vX.Y.0`: **(human)** the owner decides when; cut from `develop`, only fixes and CHANGELOG on it,
  then PR into `main` whose body has one `Closes #N` line per issue merged since the last release (feature
  PR bodies merged into `develop` do not close anything), tag, and merge `main` back into `develop`.
- `hotfix/vX.Y.Z`: from `main` for a bug on the Pi; PR into `main`, tag, then merge into `develop`.
- Never force-push `main` or `develop`; never delete a branch with an open PR.
