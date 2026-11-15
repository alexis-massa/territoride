# TerritoRide

A location-based strategy game built on real-world sports activities. Connect Strava, and every ride, run or
hike becomes a move on a living, competitive map. A personal hobby project for a small group of friends,
public in case it is useful or interesting to look at.

## Concept

- **Territory**: the map is a grid of small hexagonal tiles (H3, resolution 8). Ride through one and it is
  yours; ride a closed loop and you also claim everything enclosed inside it.
- **Points of interest**: mountain passes and summits (from OpenStreetMap), claimed by riding within 100 m.
- Ownership is always "last rider through wins". Every capture decays linearly to zero over 30 days and is then
  released automatically, so the map keeps moving.

See [GAME_DESIGN.md](GAME_DESIGN.md) for the full ruleset, [ROADMAP.md](ROADMAP.md) for how it got built and
[CHANGELOG.md](CHANGELOG.md) for what shipped. The Python/Django version is the git tag `python-final`;
[PARITY.md](PARITY.md) lists every feature it had and what proves the rewrite keeps it.

## Stack

Rust (axum, sqlx, h3o) and a Svelte 5 PWA with MapLibre, embedded in one static arm64 binary and built from the
[pi-kit](https://github.com/alexis-massa/pi-kit) template (shared code pinned by tag in `Cargo.toml` and
`web/package.json`). PostGIS database, Strava OAuth for login. Deployed on a Raspberry Pi behind Caddy.

## Commands

| Command | Does |
| --- | --- |
| `just setup` | once after cloning: tools (mise), dependencies, git hooks |
| `just dev` | API + Vite with hot reload on http://localhost:5173 (starts the local Postgres) |
| `just fmt` / `just lint` | format / check everything (the pre-commit hook runs the checks) |
| `just test` | Rust tests against a fresh local database, then the web tests |
| `just prepare` | refresh `.sqlx/` after changing a query or a migration (commit it) |
| `just build` / `just image` | arm64 binary in `dist/app` / container image |
| `just deploy` / `just rollback` | ship the current commit to the Pi / go back to the previous image |
| `just logs` / `just status` | the app on the Pi |
| `just manage ARGS` | run a binary command on the Pi, e.g. `just manage list-users` |
| `just recapture`, `just sync-all-strava`, `just strava-webhook …` | the common ones, on the Pi |
| `just schema-check FILE` | prove migration 1 equals a live schema dump |
| `just baseline`, `just ship` | cutover steps (PROGRESS.md) |

`INSTANCE=territoride-preview just …` targets the preview instance instead of the live one.

CI (GitHub Actions, `.github/workflows/ci.yml`) runs `just lint`, `just test` and the builds on every push to
`main` and every pull request; it reads pi-kit with the repository secret `PI_KIT_DEPLOY_KEY` (pi-kit README,
"CI in an app"). Nothing is deployed from CI.

The binary (what the Python management commands and the Django admin did):

| Command | Does |
| --- | --- |
| `serve`, `migrate`, `healthcheck`, `baseline` | from pi-kit |
| `load-gpx FILE USER` | load a GPX file as an activity and capture it |
| `recapture` | replay capture for every activity, oldest first (after adding POIs) |
| `release-expired` | release what is past 30 days (the server also does it daily at `DECAY_HOUR_UTC`) |
| `import-osm-pois summit\|mountain-pass` | Overpass import, worldwide; run again to resume |
| `import-osm-pois-from-file FILE` | load the NDJSON made by `scripts/extract_osm_pois.py` |
| `sync-all-strava` | sync every connected player |
| `strava-webhook create\|view\|delete` | manage the Strava webhook subscription |
| `list-users`, `list-activities [--user U]`, `leaderboard` | look around |
| `delete-activity ID`, `delete-user NAME` | remove things; others reclaim what was held |

## Layout

- `src/` the server: `grid.rs`, `capture.rs`, `decay.rs`, `scoring.rs` (the game), `import.rs`, `strava.rs`,
  `sync.rs`, `users.rs`, `crypto.rs` (Strava side), `osm.rs`, `gpx_file.rs`, `routes.rs`, `geojson.rs` (HTTP),
  `commands.rs`, `scheduler.rs`.
- `migrations/` 0001 is the Python version's schema, unchanged (Django's tables stay, so rolling back works);
  later ones are additive.
- `tests/` `geo_equivalence.rs` and `scenario.rs` compare against what the Python version computed
  (`tests/fixtures`, made by `scripts/gen_*.py`); `http.rs` runs the API against a stand-in Strava.
- `web/` the Svelte app (`pages/`, `components/`, `lib/map/` the MapLibre layers).
- Files with a `# pi-kit vX.Y.Z` header are copied from pi-kit; upgrade them with pi-kit's UPGRADING.md.

## Configuration

Environment only (`.env`, never committed; see `.env.example` and pi-infra's `APP_CONTRACT.md`): `DATABASE_URL`,
`APP_DOMAIN`, `BIND_ADDR`, `COOKIE_SECURE`, `SESSION_DAYS`, `RUST_LOG`, `STRAVA_CLIENT_ID`,
`STRAVA_CLIENT_SECRET`, `STRAVA_WEBHOOK_VERIFY_TOKEN`, `TOKEN_ENCRYPTION_KEY` (the Python version's key keeps the
stored tokens readable), `DECAY_HOUR_UTC`. The Strava app's callback domain must be `APP_DOMAIN`.

## First deploy

On the Pi, from pi-infra: `./add-app.sh territoride --env-file ~/territoride/.env`, then add the settings above to
that file. Then here: `just deploy`. See PROGRESS.md for the cutover from the Python version.

## License

[MIT](LICENSE)
