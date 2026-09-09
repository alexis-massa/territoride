# Changelog

All notable changes to this project are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/). New entries are
appended automatically to `[Unreleased]` by a commit hook (see
[README.md](README.md)); versions are cut and tagged on `main` by hand.

## [Unreleased]
- chore: don't generate changelog during rebase and cherry-pick
- feat: release tiles on activity deletion
- feat: score based on whole days + compass on map
- feat: show point calculation breakdown and per-ride detail on leaderboard
- feat: add setup.sh for one-command dev environment setup
- chore: add changelog, MIT license, and changelog commit hook

## [0.1.0] - 2026-09-08

First public-ish version - a friend-group hobby game, playable end to end.

### Added
- Strava-only login (OAuth popup), tokens encrypted at rest
- Territory capture: the world is a grid of small hexagonal tiles, claimed by
  riding through them (or enclosing them with a closed loop)
- Mountain pass POIs, imported worldwide from OpenStreetMap, claimed by
  riding within range of one
- Time-based decay: captures fade to zero value over 30 days, then release
  back to unclaimed automatically
- Leaderboard and per-player profile pages with a score breakdown
- Three ways to get activities in: manual GPX upload, one-click "Sync from
  Strava", and a real-time Strava webhook
- Rules page and a first-visit welcome popup explaining the game and the
  Strava login's security model
- Self-service account deletion, including Strava deauthorization and
  correctly handing captured territory back to whoever else rode it
- Production deployment on a Raspberry Pi: Docker Compose, Caddy for
  automatic HTTPS, DuckDNS for a stable address
