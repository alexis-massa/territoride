# TerritoRide – instructions for Claude Code

PROGRESS.md is the handoff between sessions; DECISIONS.md records choices.

## Branches
- `main` is the only long-lived branch and the GitHub default. What is on `main` is what runs on the Pi:
  `just deploy` runs from an up-to-date `main` only. Releases are tags `vX.Y.Z` on `main`.
- One branch per issue: `feature/N-short-slug` from an up-to-date `origin/main`, PR into `main` with
  `Closes #N` (the issue closes on merge). CI runs on every PR and on pushes to `main`.
- Never force-push `main`; never delete a branch with an open PR.
