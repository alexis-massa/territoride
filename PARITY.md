# Feature parity with the Python version

Checklist for the rewrite: every behaviour of `python-final` (v0.7.0, Django), and what proves the new version
has it. `[x]` = done and covered by the test named; `(manual)` = checked by hand in the browser, listed in
PROGRESS.md. `[~]` = deliberately different, see DECISIONS.md.

## Game rules
- [x] H3 resolution-8 cells; cell outlines and centres equal the Python ones — `tests/geo_equivalence.rs::cell_outlines_and_centres_match_python`
- [x] Cells a track passes through are captured — `tests/geo_equivalence.rs::tracks_give_the_same_loops_and_cells_as_python` (91 tracks)
- [x] A loop (ends within 500 m) also captures the enclosed cells — same test (45 loops, 3,391 cells identical)
- [x] A single repeated point captures its one cell — `tests/scenario.rs` (pool swim, treadmill start)
- [x] "Last through wins" by recording time, so a late-synced older ride takes nothing newer — `tests/scenario.rs::the_scripted_game_matches_python`
- [x] POIs within 100 m of a track are claimed, by the same rule — same test
- [x] Visits are recorded per player and cell, keeping the latest — same test (`visits` compared at every step)
- [x] Releasing an activity frees what it held, deletes it and lets others reclaim — same test
- [x] Expiry after 30 days releases cells and POIs and deletes old activities — same test (+20, +28, +40 days)
- [x] Decay is linear by UTC calendar day — `src/decay.rs` test, scenario leaderboards at six dates

## Scoring
- [x] 10 points per cell; multipliers Run/TrailRun 2, Walk/Hike 5, Swim/OpenWaterSwim 8, default 1 — `src/scoring.rs` tests, scenario
- [x] Swims score by distance, held tile or not — scenario (lake swim, pool swim, open-water swim with no GPS)
- [x] POIs score their altitude, decayed — scenario
- [x] Python rounding (halves to even) and the territory/POI split — `src/scoring.rs::rounding_and_split_match_python`
- [x] Leaderboard with per-ride and per-pass detail, ranked; player colours by account order — scenario (`boards` at +0/10/25/29/31/45 days)

## Strava
- [x] Log in only through Strava (state check, code exchange, player created on first login) — `tests/http.rs::strava_login_sync_webhook_and_account_deletion`, `::the_handshake_refuses_a_wrong_state_and_reports_a_denial`
- [x] Tokens stored Fernet-encrypted; tokens the Python version stored stay readable — `src/crypto.rs::reads_what_python_wrote_and_round_trips`
- [x] Expired access token is refreshed before use — `tests/http.rs::an_expired_token_is_refreshed_before_syncing`
- [x] "Sync from Strava": last 30 days, paged, skips known ids, skips non-swims with no location — http test, scenario (odd activities)
- [x] Webhook: challenge, create/update imports, delete releases — http test
- [x] Account deletion revokes Strava access, wipes the player's data, others reclaim — http test
- [x] `strava-webhook create|view|delete`, `sync-all-strava` — `src/commands.rs` (manual, needs live Strava)

## Map and pages
- [x] Territory, routes and POI GeoJSON for the viewport, bad bbox 400, `mine=1` 403 when logged out — `tests/http.rs::map_layers_filter_by_viewport_and_player`
- [x] "Only my activity": own visits (even lost tiles), all own rides, others' POIs shown unclaimed — same test, (manual)
- [x] Map opens on the player's holdings; remembers view and layer choices per browser — `GET /api/me` in the same test, (manual)
- [x] Tooltips, POI clustering, summit/pass icons, recency legend — (manual)
- [x] Leaderboard, profile, rules, changelog pages — `web/src/pages/pages.test.ts`, (manual)
- [x] Welcome dialog once per browser — (manual)
- [x] Connect with Strava in a popup that reports back and closes — (manual, needs live Strava)
- [x] GPX upload (file name fallback, errors for bad file, <2 points, no time) — `tests/http.rs::a_gpx_file_becomes_an_activity`
- [x] Installable PWA (manifest, icons, service worker) — (manual)
- [x] Sessions: cookie, CSRF, logout — `tests/http.rs::rules_changelog_and_logout`
- [~] Players log in again after cutover (Django sessions are not readable) — DECISIONS.md

## Commands (management commands and admin)
- [x] `load-gpx`, `recapture`, `release-expired`, `import-osm-pois`, `import-osm-pois-from-file` — `src/commands.rs`, `src/osm.rs` tests
- [~] Django admin → `list-users`, `list-activities`, `delete-activity`, `delete-user`, `leaderboard` — DECISIONS.md
- [x] Release of expired ownership also runs daily in the server — `src/scheduler.rs` test

## Data
- [x] Migration 1 equals the live Python schema — `just schema-check ~/pi-baseline/schemas/territoride.schema.sql`
- [x] Django's tables are untouched, so rolling back to `python-final` works — migration 2 only adds `app_sessions`
