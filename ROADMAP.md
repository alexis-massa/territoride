# Roadmap

Reworked from the original draft. The main change: **territories move after
the beta checkpoint.** The original roadmap's own closing argument was
right — territory capture is the hardest, riskiest feature to build, and
POI competition is the cheapest way to find out if this game is fun at all.
So the plan below ships a real, playable, competitive game (Strava import →
POI capture → dynamic value → leaderboards) before a single line of loop
detection code gets written, and only builds territories once that loop is
proven to hold people's attention.

## Track A — MVP (playable POI game)

### Phase 0 — Game Design ✅
Documented in [GAME_DESIGN.md](GAME_DESIGN.md). Revisit numbers after real
usage data comes in, but don't let open questions block Phase 1.

### Phase 1 — Project Setup
*~1 weekend*

- Single Django project (GeoDjango enabled), monolith — split later only if
  it actually becomes a problem
- `uv` for dependency/venv management, Python 3.13
- `ruff` (lint + format) and `mypy` (+ `django-stubs`) from day one
- Docker Compose: just two services, `app` (Django) and `db`
  (PostGIS-flavored Postgres image) — no Redis, no worker
- Frontend: Django templates + HTMX for standard pages; a single page with
  vanilla MapLibre GL JS fed by a Django view returning GeoJSON, for the
  map
- Goal: app runs locally, empty map renders, DB connected, `ruff check` and
  `mypy` both pass in CI (or a pre-commit hook) from the first commit

### Phase 2 — Authentication (Strava-only)
*~2-3 days*

- No local passwords/registration. "Connect with Strava" is the only login
  — a custom `User` model keyed on `athlete_id`, session created on
  successful OAuth callback
- Store `access_token`, `refresh_token`, `athlete_id` (encrypted at rest),
  handle token refresh
- No access gate — anyone who completes Strava OAuth is logged in
  immediately, no admin approval step. The game isn't publicly advertised,
  so the URL itself is the only barrier; revisit if that stops being enough
- `User(id, athlete_id, username, created_at)`

### Phase 3 — Strava Activity Import
*~1 week*

- **Initial backfill**: on first connect, pull the athlete's existing
  activity history (paginated), not just future ones — otherwise new
  players start with an empty map and no reason to explore what they've
  already ridden
- Webhook subscription → activity processor. At friend-group volume this
  can run **synchronously in the webhook view** — a geo query plus a few
  writes is fast enough not to need a queue. Revisit only if it starts
  blocking requests
- `Activity(user_id, strava_activity_id, polyline, distance, elevation,
  sport_type, date)`, polyline → PostGIS `LineString` via GeoDjango's
  `LineStringField`
- **Strava API compliance**: respect their rate limits (100 req/15min,
  1000/day by default), follow the Strava brand guidelines for the
  "Connect with Strava" button, and handle athlete deauthorization (delete
  their data on webhook `deauthorize` event) — this is a hard requirement
  of Strava's API agreement, not optional polish
- Milestone: user sees all their activities on the map

### Phase 4 — Map Foundation
*~1 week*

- Render activities, users, POIs on the world map
- Time filters: last week / last month / all time
- Click a route → distance, elevation, date
- Milestone: the map is already useful before any game mechanics exist

### Phase 5 — POI System
*~1-2 weeks — this is where the game starts*

- Seed POI database — start with **French mountain passes only**, it's
  enough to validate the mechanic
- `POI(id, name, type, location, value)`
- Capture: on activity import, `ST_DWithin` check against nearby POIs
  (radius per [GAME_DESIGN.md](GAME_DESIGN.md))
- `POICapture(poi_id, owner_id, captured_at)`, last visitor owns it
- Milestone: players can steal passes from each other

### Phase 6 — Dynamic Value System
*~1 week*

- `capture_frequency`, `last_capture`, `value` on POIs
- Daily recalculation job: frequently visited → value down, ignored →
  value up. Just a Django management command (`recalc_values`) fired by a
  systemd timer or cron — no in-process scheduler needed
- Milestone: the game starts rewarding exploration over repetition

### Phase 7 — Leaderboards & Scoring
*~1 week*

- Global, regional (country/region/department), POI-specialist (top pass
  collectors), combined score
- This is also where the permanent-vs-seasonal score split from
  [GAME_DESIGN.md](GAME_DESIGN.md) gets surfaced in the UI

### Phase 7.5 — Observability & Basic Anti-abuse
*~2-3 days, don't skip before inviting real users*

- Structured logging on the webhook → import → capture pipeline (this is
  the part that fails silently and confuses players — "why didn't my ride
  count?")
- The `is_suspicious` flag and speed-sanity checks from
  [GAME_DESIGN.md](GAME_DESIGN.md)
- Basic error alerting on the background job queue

---

**🚦 MVP checkpoint.** Invite 50-100 athletes to the POI + leaderboard game,
no territories yet. Watch what gets farmed, what gets ignored, whether the
decay curve feels right, whether people actually detour for a forgotten
pass. This is the checkpoint that tells you whether Track B is worth
building at all.

---

## Track B — Territory Expansion

Only start this once Track A has real players and real signal. Territory
capture is the technically hardest part of the game — don't pay that cost
before you know people want it.

### Phase 8 — Territory Prototype
*~2-4 weeks*

- Loop detection: is a route a valid closed loop? (thresholds in
  [GAME_DESIGN.md](GAME_DESIGN.md))
- Polygon creation via GEOS (through GeoDjango's geometry API, no separate
  `shapely` dependency needed since GeoDjango already wraps GEOS)
- `Territory(polygon, owner, created_at)`
- Milestone: a player creates their first territory

### Phase 9 — Territory Conflict
*~2 weeks, expect a lot of iteration*

- Overlap / containment / replacement rules using `ST_Intersection`,
  `ST_Contains`, `ST_Difference`
- Milestone: territories can be stolen

### Phase 10 — Notifications
*~1 week*

- In-app only for now ("You captured Col du Galibier", "Your territory was
  stolen", "Mont Ventoux is now worth 800 points")
- No push notifications yet

### Phase 11 — Seasons
*~1 week*

- Season resets every 1-3 months so new players can compete and no one
  dominates forever
- Career score (from Phase 7) persists across seasons regardless

### Phase 12 — Public Beta
- Pick one focused region (e.g. France / Rhône-Alpes / the Alps), don't go
  global
- Populate passes, summits, landmarks for that region
- Invite 50-100 athletes (a fresh cohort, or the same Track A group with
  territories now switched on — the latter gives a cleaner before/after
  read)
- Watch the same questions as the MVP checkpoint, now with territories in
  the mix: which assets get farmed, how often territories change hands,
  are loops too easy to draw

## Technical Growth Path

This project is explicitly not trying to scale past a friend group, so the
stack optimizes for minimal ops surface over raw capability:

**Stack:** Django + GeoDjango, PostgreSQL + PostGIS, Django templates +
HTMX, vanilla MapLibre GL JS on the map page, Strava-only auth, `uv` +
Python 3.13, `ruff` + `mypy` for code quality. Two containers
(`app`, `db`) — no Redis, no task queue, no separate frontend build.
Webhooks processed synchronously in the view; the daily value-decay job is
a management command run by cron/systemd timer.

**If it ever actually needs to scale:** that's the point to introduce
Celery/Redis for background jobs, caching, a CDN, and tile
optimization/pre-rendering for the map layer — none of it needed now.

No microservices. Not until there's a concrete reason.

## The Principle This Roadmap Optimizes For

The biggest risk isn't the tech — it's spending months on territory
mechanics before knowing whether players enjoy competing for passes and
landmarks at all. Strava import → POI capture → dynamic value →
leaderboards is a playable game in a few weeks. Territory is the hard part;
POI competition is where you learn whether the game has legs.
