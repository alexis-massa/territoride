# Roadmap

Reworked from the original draft. The main change: **territories move after
the beta checkpoint.** The original roadmap's own closing argument was
right — territory capture is the hardest, riskiest feature to build, and
POI competition is the cheapest way to find out if this game is fun at all.
So the plan below ships a real, playable, competitive game (Strava import →
POI capture → dynamic value → leaderboards) before a single line of loop
detection code gets written, and only builds territories once that loop is
proven to hold people's attention.

A secondary change: a few cross-cutting concerns (Strava API compliance,
anti-cheat, observability) that weren't in the original are called out
explicitly, because they're cheap to build in from the start and expensive
to retrofit.

## Track A — MVP (playable POI game)

### Phase 0 — Game Design ✅
Documented in [GAME_DESIGN.md](GAME_DESIGN.md). Revisit numbers after real
usage data comes in, but don't let open questions block Phase 1.

### Phase 1 — Project Setup
*~1 weekend*

- `app/` monolith to start (backend + frontend together) — split later only
  if it actually becomes a problem
- Docker Compose: PostgreSQL + PostGIS
- Backend: FastAPI, SQLAlchemy, Alembic
- Frontend: **decision point** — NiceGUI is genuinely fast for a solo dev
  and fine for auth screens, profile, leaderboards. For the map itself
  (MapLibre GL JS, marker interaction, live updates) evaluate early whether
  NiceGUI's HTML/JS embedding is enough, or whether the map view alone
  should be a small standalone JS page the rest of the app embeds. Decide
  this in the first weekend, not three months in — a map-heavy game is the
  wrong place to fight the frontend framework.
- Mapping: MapLibre GL JS
- Background jobs: Redis + Dramatiq (needed as soon as webhooks exist —
  don't process Strava webhook payloads synchronously in the request path)
- Goal: app runs locally, empty map renders, DB connected

### Phase 2 — Authentication
*~1-2 days*

- Registration, login, sessions — keep it boring
- `User(id, email, username, created_at)`

### Phase 3 — Strava Integration
*~1 week*

- OAuth ("Connect Strava"), store `access_token`, `refresh_token`,
  `athlete_id` (encrypted at rest)
- **Initial backfill**: on first connect, pull the athlete's existing
  activity history (paginated), not just future ones — otherwise new
  players start with an empty map and no reason to explore what they've
  already ridden
- Webhook subscription → activity processor (queued via Dramatiq, not
  inline)
- `Activity(user_id, strava_activity_id, polyline, distance, elevation,
  sport_type, date)`, polyline → PostGIS `LineString`
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
  value up
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
- Polygon creation via `shapely` + PostGIS
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

**MVP (Track A):** FastAPI, PostgreSQL + PostGIS, MapLibre GL JS, Strava
API, Redis + Dramatiq (needed from Phase 3 onward for webhook processing,
not deferred to beta)

**If it takes off:** caching, CDN, tile optimization/pre-rendering for the
map layer

No microservices. Not until there's a concrete reason.

## The Principle This Roadmap Optimizes For

The biggest risk isn't the tech — it's spending months on territory
mechanics before knowing whether players enjoy competing for passes and
landmarks at all. Strava import → POI capture → dynamic value →
leaderboards is a playable game in a few weeks. Territory is the hard part;
POI competition is where you learn whether the game has legs.
