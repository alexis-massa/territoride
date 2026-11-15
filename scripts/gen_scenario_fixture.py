"""Record what the Python version (Django) does with a scripted game, for tests/scenario.rs.

The script imports Strava-shaped activities (late-synced ones included), releases and expires them, adds
POIs and replays capture, and after each step stores the cells, POIs, visits and the leaderboard at several
"now"s. Everything is invented; no player's real data is stored in the repository.

Run it inside the python-final image against a throw-away database (see PROGRESS.md):
  docker run --rm --network host --env-file dj.env -v $PWD/scripts:/s territoride-py:python-final uv run python \
    -c "import os,django;os.environ['DJANGO_SETTINGS_MODULE']='config.settings';django.setup();exec(open('/s/gen_scenario_fixture.py').read())" \
    > tests/fixtures/scenario.json
"""
import json
import math
from datetime import datetime, timedelta, timezone as tz

from django.contrib.gis.geos import Point
from django.db import connection
from django.utils import timezone

from accounts.models import User
from game import decay, scoring
from game.capture import recapture_all, release_activity
from game.models import POI, Activity, CellVisit, TerritoryCell
from game.strava_import import import_from_strava

NOW = datetime(2026, 11, 15, 12, 0, tzinfo=tz.utc)
clock = {"now": NOW}
timezone.now = lambda: clock["now"]  # decay and release_expired read the clock through this

with connection.cursor() as cur:
    cur.execute("TRUNCATE game_cellvisit, game_territorycell, game_poi, game_activity, accounts_user CASCADE")


def encode(points):
    """Google polyline encoding of (lat, lng) points."""
    out, prev = [], (0, 0)
    for lat, lng in points:
        cur = (round(lat * 1e5), round(lng * 1e5))
        for d in (cur[0] - prev[0], cur[1] - prev[1]):
            v = ~(d << 1) if d < 0 else d << 1
            while v >= 0x20:
                out.append(chr((0x20 | (v & 0x1F)) + 63))
                v >>= 5
            out.append(chr(v + 63))
        prev = cur
    return "".join(out)


def line(lat, lng, bearing, km, n=40):
    return [
        (
            lat + km * 1000 * i / (n - 1) * math.cos(bearing) / 111_000,
            lng + km * 1000 * i / (n - 1) * math.sin(bearing) / (111_000 * math.cos(math.radians(lat))),
        )
        for i in range(n)
    ]


def loop(lat, lng, radius_m, n=36):
    pts = [
        (
            lat + radius_m * math.sin(2 * math.pi * i / n) / 111_000,
            lng + radius_m * math.cos(2 * math.pi * i / n) / (111_000 * math.cos(math.radians(lat))),
        )
        for i in range(n)
    ]
    return pts + [pts[0]]


def raw(strava_id, name, sport, days_ago, distance, points=None, start=None):
    r = {
        "id": strava_id,
        "name": name,
        "sport_type": sport,
        "start_date": (NOW - timedelta(days=days_ago)).isoformat(),
        "distance": distance,
    }
    if points:
        r["map"] = {"summary_polyline": encode(points)}
    if start:
        r["start_latlng"] = list(start)
    return r


ride_a = line(45.90, 6.10, 0.3, 14)
ride_old = line(45.88, 6.14, 2.2, 12)
loop_b = loop(45.93, 6.15, 2500)
hike_c = line(45.91, 6.12, 5.0, 5, n=25)
swim_lake = loop(45.85, 6.20, 300, n=16)

users = {name: User.objects.create(username=name) for name in ("ann", "bob", "cy")}
midpoint = ride_a[20]
for name, kind, alt, loc in (
    ("Col A", "mountain_pass", 1500, midpoint),
    ("Col near", "mountain_pass", 1200, (midpoint[0] + 0.0006, midpoint[1])),  # ~67 m off the line
    ("Col far", "mountain_pass", 900, (midpoint[0] + 0.004, midpoint[1])),  # ~440 m off the line
    ("Cime B", "summit", 2200, loop_b[9]),
    ("Cime nowhere", "summit", 3100, (46.5, 7.5)),
):
    POI.objects.create(name=name, poi_type=kind, altitude_m=alt, location=Point(loc[1], loc[0], srid=4326))


def iso(d):
    return d.astimezone(tz.utc).isoformat().replace("+00:00", "Z") if d else None


def state():
    name_of = dict(Activity.objects.values_list("id", "name"))
    user_of = dict(User.objects.values_list("id", "username"))
    return {
        "cells": sorted(
            [c.cell_id, user_of.get(c.owner_id), name_of.get(c.captured_by_id), iso(c.captured_at)]
            for c in TerritoryCell.objects.exclude(owner=None)
        ),
        "pois": sorted(
            [p.name, user_of.get(p.owner_id), name_of.get(p.claimed_by_id), iso(p.claimed_at)]
            for p in POI.objects.all()
        ),
        "visits": sorted(
            [user_of[v.user_id], v.cell_id, name_of.get(v.activity_id), iso(v.visited_at)] for v in CellVisit.objects.all()
        ),
        "activities": sorted(Activity.objects.values_list("name", flat=True)),
    }


def jsonable(o):
    if isinstance(o, datetime):
        return iso(o)
    raise TypeError(type(o))


def boards():
    out = []
    for days in (0, 10, 25, 29, 31, 45):
        clock["now"] = NOW + timedelta(days=days)
        rows = scoring.leaderboard(with_detail=True)
        out.append({"days": days, "rows": json.loads(json.dumps(rows, default=jsonable))})
    clock["now"] = NOW
    return out


steps = []


def step(label, **extra):
    steps.append({"label": label, **extra, "state": state(), "boards": boards()})


def do_import(user, *rows):
    return list(import_from_strava(users[user], list(rows)))


step("start")
r = do_import("ann", raw(1001, "A ride", "Ride", 6, 20_000, ride_a))
step("ann rides", result=r)
r = do_import("bob", raw(1002, "B loop", "Run", 3, 4_000, loop_b))
step("bob loops", result=r)
r = do_import("cy", raw(1003, "C hike", "Hike", 3, 5_000, hike_c))
step("cy hikes", result=r)
r = do_import("ann", raw(1004, "A old ride", "Ride", 12, 12_000, ride_old))
step("ann syncs an older ride late", result=r)
r = do_import(
    "cy",
    raw(1005, "Lake swim", "Swim", 2, 1_500, swim_lake),
    raw(1006, "Pool swim", "Swim", 2, 1_000, start=(45.80, 6.30)),
    raw(1007, "Open water, no gps", "OpenWaterSwim", 1, 2_000),
    raw(1008, "Ride, no gps", "Ride", 1, 9_000),
    raw(1009, "Ride, one point", "Ride", 1, 9_000, points=[(45.7, 6.4)]),
    raw(1010, "Run on a treadmill start", "Run", 1, 5_000, start=(45.71, 6.41)),
)
step("cy swims and sends odd activities", result=r)
r = do_import("ann", raw(1001, "A ride", "Ride", 6, 20_000, ride_a))
step("re-importing a known id changes nothing", result=r)

release_activity(Activity.objects.get(name="B loop"))
step("bob's loop released")
POI.objects.create(name="Col late", poi_type="mountain_pass", altitude_m=1800, location=Point(hike_c[10][1], hike_c[10][0], srid=4326))
recapture_all()
step("a POI added then recapture")

for days in (20, 28, 40):
    clock["now"] = NOW + timedelta(days=days)
    n = decay.release_expired()
    clock["now"] = NOW
    step(f"release expired at +{days} days", released=n)

print(json.dumps({"now": iso(NOW), "steps": steps}, indent=None, separators=(",", ":")))
