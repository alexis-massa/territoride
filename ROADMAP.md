# Roadmap

The game is built in two tracks: a playable POI game first, territories after
a checkpoint with real players. The app is now Rust (axum, sqlx, PostGIS) with
a Svelte + MapLibre frontend, on the shared pi-kit library. Game rules live in
[GAME_DESIGN.md](GAME_DESIGN.md); what shipped, release by release, in
[CHANGELOG.md](CHANGELOG.md).

## Track A — MVP (playable POI game)

- **Phase 0 — Game design** ✅ [GAME_DESIGN.md](GAME_DESIGN.md).
- **Phase 1 — Project setup** ✅ Rust + Svelte app, PostGIS, CI (lint, tests, builds).
- **Phase 2 — Authentication** ✅ Strava-only login, tokens encrypted at rest.
- **Phase 3 — Strava activity import** ✅ Manual sync, webhook with deauthorisation handling, GPX upload.
- **Phase 4 — Map foundation** ✅ Routes, territory and passes on a MapLibre map, viewport-filtered.
- **Phase 5 — POI system** ✅ Mountain passes and summits from OpenStreetMap, captured within a radius of the route; last visitor owns it.
- **Phase 6 — Dynamic value** ✅ Captures expire after a number of days and are released by a daily task.
- **Phase 7 — Leaderboards and scoring** ✅ Global leaderboard, per-sport multipliers, profile with capture breakdown. Open: regional boards, POI-specialist boards, permanent-vs-seasonal split in the UI.
- **Phase 7.5 — Observability and anti-abuse** Open: structured logging along webhook → import → capture so a missing ride is explainable; the `is_suspicious` flag and speed-sanity checks from GAME_DESIGN.md.

**MVP checkpoint.** Invite 50-100 athletes to the POI and leaderboard game and
watch what gets farmed, what gets ignored, and whether the decay curve feels
right. This decides whether Track B is worth building.

## Track B — Territory expansion

Territory cells (H3) with loop detection are already in the app. What remains:

- **Phase 9 — Territory conflict.** Overlap and replacement rules refined with real data.
- **Phase 10 — Notifications.** In-app only: "you captured X", "your territory was stolen", "X is now worth N points". No push.
- **Phase 11 — Seasons.** Reset every 1-3 months so no one dominates forever; career score persists.
- **Phase 12 — Public beta.** One focused region (e.g. the Alps), populated with passes, summits and landmarks; 50-100 athletes; watch which assets get farmed and whether loops are too easy to draw.

## Principle

The biggest risk is not the tech but building territory mechanics before
knowing whether players enjoy competing for passes at all. Keep the ops
surface small: this is meant for a friend group, not for scale.
