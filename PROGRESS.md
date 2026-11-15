# TerritoRide – progress

Handoff between work sessions: read it first, update it last.

## State (2026-11-15)
**Rewrite on the Rust + Svelte stack (pi-infra phase 4): live since 2026-11-15 (v0.8.0; v0.8.1 for the role change).** 25 Rust tests (grid/capture
against 91 Python-computed tracks, a 12-step scripted game compared with Django's results, HTTP API against a
stand-in Strava), 9 web tests, lint clean, migration 1 equals the live schema. Tracking issue #36.

## Cutover (2026-11-15, 14:47-14:50 UTC)
Done with the owner's go: restore-checked backup (`~/backups/territoride/territoride-20261115-144704.dump`), `just ship`, Python compose `down`, `.env` extended (the Python keys stay in
it and in `.env.python`, so the Python version can start again), `baseline`, `./deploy.sh`: healthy in 14 s.
- Checked live: leaderboard identical to the Python one before the switch (5 players, same totals); Strava
  webhook subscription (`/accounts/strava/webhook/`) answers the challenge and refuses a wrong token; `strava-webhook
  view` works (credentials and TLS roots fine); `sync-all-strava` read and refreshed all 6 players' stored tokens.
- Preview removed (final dump in `~/backups/territoride-preview/`).
- Players log in again through Strava once (Python sessions are not read).
- **Clean-up (2026-11-15, after the cutover):** the Python image, checkout and `.env.python` are gone from the Pi
  (pi-infra REPORT.md lists what was verified first); `.env` holds only the Rust keys. Going back to Python would
  mean rebuilding from the `python-final` tag; Django's tables are still in the database. `just rollback` still
  goes back to the previous Rust image.
- **Cron (done 2026-11-15, pi-infra REPORT.md):** the old 04:00 `release_expired_ownership` line is gone (the server
  releases daily at `DECAY_HOUR_UTC`, default 4); `backup.sh territoride` runs nightly at 03:25.
- **CI:** GitHub Actions runs `just lint`, `just test` and the builds on every push and pull request
  (`.github/workflows/ci.yml`, secret `PI_KIT_DEPLOY_KEY`); Dependabot bumps the actions monthly. `main` is ahead
  of the deployed v0.8.1 by CI, tests, docs, pi-kit v0.4.2 (no code change) and patch-level dependency updates.
- Nothing technical is open: new work is features (ROADMAP.md, GAME_DESIGN.md).

**Database login (2026-11-15):** the app connects as `territoride`, an ordinary role that owns only its database. The Postgres superuser, formerly also called `territoride`, was renamed `pi_admin`; the unused tiger/topology extensions were dropped (v0.8.1, migration 3).

## Session log
| Date | Done | Next |
| --- | --- | --- |
| 2026-11-15 | Tidy-up: CI and Dependabot (pi-kit v0.4.2), tests drop their databases, annotated tags and GitHub release v0.8.1, docs | Features only |
| 2026-11-15 | v0.8.1 (database role change, unused extensions dropped) deployed; Python leftovers removed (pi-infra phase 5) | |
| 2026-11-15 | Cutover to v0.8.0; preview removed | Owner: log in once, delete the old cron line |
| 2026-11-15 | Preview on a copy of the live data (see above); CA-roots fix |  Owner test, then cutover |
| 2026-11-15 | Scaffold from pi-kit v0.4.1; baseline + `app_sessions` migrations; grid, capture, decay, scoring, Strava, imports, API, CLI, Svelte app; tests against Django | Preview and owner test |
