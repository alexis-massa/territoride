# Game Design — Phase 0 Decisions

These are working defaults, not permanent rules. Ship the MVP with these
numbers, then tune them once real activity data shows how they behave.
Everything here should live in config, not hardcoded, so balancing doesn't
require a redeploy.

## World Model

**Activity**
- Imported from Strava (polyline → PostGIS `LineString`)
- Fields: `user_id`, `strava_activity_id`, `polyline`, `distance`,
  `elevation_gain`, `sport_type`, `started_at`

**Territory**
- A closed loop enclosing an area, owned by one player
- Polygon stored in PostGIS

**POI (Point of Interest)**
- A named point (`pass`, `summit`, `viewpoint`, `lake`, `castle`,
  `monument`, `landmark`) with a location and a current `value`

## Territories

| Question | Default |
|---|---|
| Loop closing tolerance | Start and end of the activity must be within **500m** of each other for the route to be considered a loop candidate — Strava trims ~200m off each end of a public activity to hide home addresses, so a real loop can show up to ~400m "open"; 500m covers that plus GPS noise |
| Minimum enclosed area | **1 hectare (0.01 km²)** — filters out GPS noise loops (e.g. a parking lot circle) |
| Maximum territory size | **50 km²** per single capture — forces players toward multiple smaller, contestable territories instead of one mega-loop claiming a whole valley |
| Self-intersecting routes | Reject for v1 (GEOS `is_valid` check via GeoDjango's geometry API). Figure-eight → largest simple sub-loop is a good v2 improvement, not MVP scope |
| Overlap | Territories cannot overlap. A new loop's polygon is clipped against existing owned polygons (`ST_Difference`); the overlapping portion is a **contest**, resolved by the conflict rules in Phase 8/9, not an automatic overwrite |

## Points of Interest

| Question | Default |
|---|---|
| Capture radius | **100m** for mountain passes (the only POI type so far) — covers typical GPS drift without a route that merely passes nearby falsely claiming it |
| Capture method | Distance from the track to the POI, via PostGIS (`Distance` annotation + geodetic filter) — proximity to the whole track, not just start/end point. Applies the same way whether the route is a loop or not — a pass is only ever claimed by proximity, never by loop enclosure |
| Must physically pass through | Yes — this is the whole point of a POI vs. a territory |
| Ownership rule | Last visitor owns it (simple, no ambiguity, easy to explain to players) |

## Scoring

| Question | Default |
|---|---|
| When is score awarded | Immediately, on activity import/webhook processing |
| Score lifetime | Two tracks: a **permanent career score** (prestige, never resets) and a **seasonal score** (resets each season, see Phase 11) — this is what most leaderboards should show by default |
| Value formula | `territory_value + poi_value` (territory: flat 10/cell; POIs: altitude in meters — a bigger pass is worth more). No exploration bonus yet — needs capture history, which isn't tracked |
| Value decay | Time-based, not frequency-based (simpler, needs no capture history): value decays **linearly to 0 over 30 days** since capture, at which point ownership itself releases back to unowned — computed live for scoring, actually released by a periodic sweep (`release_expired_ownership`, meant to run daily via cron). 30 days is a first guess, tune once there's real play data |
