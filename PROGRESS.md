# TerritoRide – progress

Handoff between work sessions: read it first, update it last.

## State (2026-11-15)
**Rewrite on the Rust + Svelte stack (pi-infra phase 4): built and tested, not deployed.** Branch `rust-rewrite`,
tracking issue #36. 25 Rust tests (grid/capture against 91 Python-computed tracks, a 12-step scripted game
compared with Django's results, HTTP API against a stand-in Strava), 9 web tests, lint clean, migration 1 equals
the live schema (`just schema-check`). Live is still the Python version (v0.7.0, tag `python-final`).

## Next: preview, then cutover (needs the owner)
1. **Preview** on a copy of the live data: own database and role, `INSTANCE=territoride-preview`, host name
   `territoride-preview.<domain>`. Strava allows one callback domain per app, so logging in through Strava on the
   preview needs that domain changed temporarily (or a session row made by hand); everything else can be checked
   without it.
2. **Owner tests** the preview: map, layers, leaderboard, profile, rules, changelog, GPX upload, PWA install.
3. **Cutover**, like fittracker and dermaquiz: backup (restore-checked) → `just ship` → stop Python → `just baseline`
   → `just deploy` → re-point the Strava webhook if needed (`strava-webhook view`) → players log in again once.
   `.env` on the Pi needs `STRAVA_*`, `STRAVA_WEBHOOK_VERIFY_TOKEN`, `TOKEN_ENCRYPTION_KEY` (same as the Python
   version's), `APP_DOMAIN`, `COOKIE_SECURE=true`.
4. **Rollback**: `just rollback`, or start the Python compose again; Django's tables and sessions are untouched
   (migration 2 only adds `app_sessions`).

Open question for the owner: the Postgres role the app uses is still the shared superuser; a role limited to
this database is a separate, approved step (pi-infra phase 5).

## Session log
| Date | Done | Next |
| --- | --- | --- |
| 2026-11-15 | Scaffold from pi-kit v0.4.1; baseline + `app_sessions` migrations; grid, capture, decay, scoring, Strava, imports, API, CLI, Svelte app; tests against Django | Preview and owner test |
