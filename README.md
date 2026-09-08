# TerritoRide

A location-based strategy game built on real-world sports activities. Connect
Strava, and every ride, run, or hike becomes a move on a living, competitive
map.

A personal hobby project for a small group of friends — built end to end
(including deployment) largely as an experiment in pair-programming with
Claude. Realistically nobody else will ever touch this code, but it's public
in case it's useful or interesting to look at.

## Concept

The world is made of two systems:

- **Territory** — the map is a grid of small hexagonal tiles. Ride through
  one and it's yours; ride a closed loop and you also claim everything
  enclosed inside it.
- **Points of interest** — mountain passes (worldwide, sourced from
  OpenStreetMap) captured by physically riding within range of one.
  Territory ownership never grants a POI - you have to go there yourself,
  and so does anyone who wants to take it from you.

Ownership is always "last rider through wins" - simple, no ambiguity. Nothing
is permanent: every capture decays linearly to zero value over 30 days and
is then released back to unclaimed automatically, so the map keeps moving
even if nobody's actively fighting over it.

See [GAME_DESIGN.md](GAME_DESIGN.md) for the full ruleset and open design
decisions, [ROADMAP.md](ROADMAP.md) for how this got built, and
[CHANGELOG.md](CHANGELOG.md) for what's actually shipped.

## Stack

Django + GeoDjango + PostGIS, MapLibre GL JS, Strava OAuth for login, H3 for
the hex grid, `uv` for dependency management. Deployed via Docker Compose +
Caddy (automatic HTTPS) on a Raspberry Pi, behind DuckDNS for a stable
address. Deliberately minimal - no Redis, no task queue, no build step for
the frontend.

## Local development

```bash
./setup.sh
```

Prompts for Strava API credentials (from
[strava.com/settings/api](https://www.strava.com/settings/api) - set that
app's "Authorization Callback Domain" to `localhost`; leave blank to fill in
later), generates everything else that should be random, enables the
changelog commit hook, and builds/starts the app at http://localhost:8000.

Refuses to run if `.env` already exists, so it's safe to leave lying around -
remove `.env` first if you want to regenerate it.

## Changelog

[CHANGELOG.md](CHANGELOG.md) follows [Keep a Changelog](https://keepachangelog.com/).
A commit hook (`.githooks/post-commit`) automatically appends every commit's
subject line to the `[Unreleased]` section - versions are cut and tagged on
`main` by hand when it feels like a real milestone.

## License

[MIT](LICENSE)
