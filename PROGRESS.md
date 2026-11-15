# TerritoRide – progress

Handoff between work sessions: read it first, update it last.

## State (2026-11-15)
**Rewrite on the Rust + Svelte stack (pi-infra phase 4): built and tested, not deployed.** Branch `rust-rewrite`,
tracking issue #36. 25 Rust tests (grid/capture against 91 Python-computed tracks, a 12-step scripted game
compared with Django's results, HTTP API against a stand-in Strava), 9 web tests, lint clean, migration 1 equals
the live schema (`just schema-check`). Live is still the Python version (v0.7.0, tag `python-final`).

## Preview (2026-11-15, running)
https://territoride-preview.territoride.duckdns.org: a restored copy of the live database (dump of 14:34 UTC:
6 players, 36 activities, 3,702 cells, 209,115 POIs), its own database and role `territoride_preview`, Pi folder
`~/territoride-preview/`, `DECAY_HOUR_UTC=-1`, **no Strava credentials on purpose** (a token refresh from the
preview could invalidate the refresh token the live app holds). So Strava login, sync and webhook are untested
live; they are covered by `tests/http.rs`.
- Healthy in 14 s. Leaderboard identical to the live Python one (5 players, same totals). Map shows the real
  data; service worker active, manifest with 3 icons, nothing from `/api` in the cache; CSP and HSTS set.
- Found on the preview and fixed: the `FROM scratch` image has no CA certificates (reqwest failed to build its
  client at start) → bundled roots (`src/http.rs`). Also fixed in pi-infra: `restore.sh` choked on the
  `postgis_topology` schema comment and sequence (pi-infra PR 11).
- To log in without Strava, an `app_sessions` row was made for `alexis-massa` (token only in the owner's hands).

## Next: owner tests, then cutover
1. **Owner tests** the preview: map and layers ("only my activity" needs a login), leaderboard, profile, rules,
   changelog, GPX upload, PWA install. Ask for a session cookie command.
2. **Cutover**, like fittracker and dermaquiz: backup (restore-checked) → `just ship` → stop Python (`~/territoride`
   compose) → `.env` for the live folder (`STRAVA_*`, `STRAVA_WEBHOOK_VERIFY_TOKEN`, `TOKEN_ENCRYPTION_KEY` from the
   old `.env`, `APP_DOMAIN`, `COOKIE_SECURE=true`, `DATABASE_URL`) → `just baseline` → `./deploy.sh` → remove the
   preview. Players log in again once through Strava; the Strava webhook path is unchanged.
3. **Rollback**: `just rollback`, or start the Python compose again; Django's tables are untouched.

Open: the app's Postgres role is still the shared superuser; a role limited to this database is a separate,
approved step (pi-infra phase 5).

## Session log
| Date | Done | Next |
| --- | --- | --- |
| 2026-11-15 | Preview on a copy of the live data (see above); CA-roots fix |  Owner test, then cutover |
| 2026-11-15 | Scaffold from pi-kit v0.4.1; baseline + `app_sessions` migrations; grid, capture, decay, scoring, Strava, imports, API, CLI, Svelte app; tests against Django | Preview and owner test |
