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
cp .env.example .env   # fill in Strava API credentials, generate the rest
docker compose up -d
docker compose exec app uv run python manage.py migrate
```

One-time setup for the changelog commit hook (see below) - each clone needs
to opt in, since git hooks aren't tracked by git itself:

```bash
git config core.hooksPath .githooks
```

## Changelog

[CHANGELOG.md](CHANGELOG.md) follows [Keep a Changelog](https://keepachangelog.com/).
A commit hook (`.githooks/post-commit`) automatically appends every commit's
subject line to the `[Unreleased]` section - versions are cut and tagged on
`main` by hand when it feels like a real milestone.

## License

[MIT](LICENSE)
