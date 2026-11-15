"""Record what the Python version (Django, h3-py) computes for synthetic tracks, for tests/geo_equivalence.rs.

Run inside the python-final image (see tests/README in PROGRESS.md):
  docker run --rm --network host --env-file dj.env -v $PWD/scripts:/s territoride-py:python-final \
    sh -c 'uv run python manage.py shell < /s/gen_geo_fixtures.py' > tests/fixtures/geo.json
Tracks are invented (no player's real GPS data is stored in the repository).
"""
import json
import math
import random

import h3
from game.capture import captured_cells, is_loop, touched_cells
from game.grid import cell_boundary, cell_centroid, cells_in_loop
from game.strava_import import decode_polyline

rng = random.Random(20261115)
tracks = {}


def jitter(points, metres=15):
    d = metres / 111_000
    return [(lat + rng.uniform(-d, d), lng + rng.uniform(-d, d)) for lat, lng in points]


def loop(lat, lng, radius_m, n=40, wobble=0.25):
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        r = radius_m * (1 + rng.uniform(-wobble, wobble))
        pts.append((lat + r * math.sin(a) / 111_000, lng + r * math.cos(a) / (111_000 * math.cos(math.radians(lat)))))
    return pts + [pts[0]]


def ride(lat, lng, bearing, km, n=60):
    pts = []
    for i in range(n):
        d = km * 1000 * i / (n - 1)
        pts.append((lat + d * math.cos(bearing) / 111_000, lng + d * math.sin(bearing) / (111_000 * math.cos(math.radians(lat)))))
    return jitter(pts)


tracks["short_ride"] = ride(45.9, 6.1, 0.4, 3)
tracks["long_ride"] = ride(45.5, 5.9, 2.0, 40)
tracks["tiny_loop"] = loop(45.9, 6.12, 60, n=12)  # smaller than one cell
tracks["small_loop"] = loop(45.9, 6.12, 400)
tracks["big_loop"] = loop(45.4, 5.8, 4000, n=90)
tracks["huge_loop"] = loop(45.1, 6.3, 15000, n=120)
tracks["almost_loop"] = loop(45.3, 5.7, 1500)[:-2]  # endpoints a few hundred metres apart
tracks["not_loop"] = ride(45.6, 6.0, 1.0, 12)[:-1]
tracks["figure_eight"] = loop(45.7, 6.2, 800, n=30)[:-1] + loop(45.7035, 6.2, 800, n=30)[:-1] + [loop(45.7, 6.2, 800, n=30)[0]]
tracks["single_point"] = [(45.8, 6.15), (45.8, 6.15)]
tracks["two_points"] = [(45.8, 6.15), (45.8004, 6.1503)]
for i in range(40):
    lat, lng = 44.5 + rng.random() * 2, 5 + rng.random() * 2
    tracks[f"random_loop_{i}"] = loop(lat, lng, rng.choice([150, 300, 800, 2000, 5000]), n=rng.choice([16, 40, 70]), wobble=0.4)
    tracks[f"random_ride_{i}"] = ride(lat, lng, rng.random() * 6.28, rng.choice([1, 5, 20]))

out = {"tracks": {}, "cells": {}}
for name, pts in tracks.items():
    pts = [(round(a, 6), round(b, 6)) for a, b in pts]
    entry = {"points": pts, "is_loop": is_loop(pts), "touched": sorted(touched_cells(pts))}
    try:  # the Python version raises ValueError for a loop of fewer than 3 points (e.g. two close points)
        entry["captured"] = sorted(captured_cells(pts))
        if entry["is_loop"]:
            entry["in_loop"] = sorted(cells_in_loop(pts))
    except ValueError as exc:
        entry["python_error"] = str(exc)
    out["tracks"][name] = entry
    for c in entry.get("captured", entry["touched"])[:3]:
        b = cell_boundary(c)
        pt = cell_centroid(c)
        out["cells"][c] = {"boundary": [list(p) for p in b], "centroid": [pt.x, pt.y]}

# encoded polylines: points -> encoded by the standard algorithm, decoded by the Python version
def encode(points):
    def enc(v):
        v = ~(v << 1) if v < 0 else v << 1
        s = ""
        while v >= 0x20:
            s += chr((0x20 | (v & 0x1F)) + 63)
            v >>= 5
        return s + chr(v + 63)
    out_s, plat, plng = "", 0, 0
    for lat, lng in points:
        la, ln = round(lat * 1e5), round(lng * 1e5)
        out_s += enc(la - plat) + enc(ln - plng)
        plat, plng = la, ln
    return out_s

polys = []
for name in ["short_ride", "big_loop", "random_ride_3", "random_loop_7"]:
    enc = encode(tracks[name])
    polys.append({"encoded": enc, "decoded": [list(p) for p in decode_polyline(enc)]})
out["polylines"] = polys
out["great_circle"] = [
    {"a": [45.0, 6.0], "b": [45.0, 6.01], "m": h3.great_circle_distance((45.0, 6.0), (45.0, 6.01), unit="m")},
    {"a": [45.9, 6.1], "b": [46.2, 6.9], "m": h3.great_circle_distance((45.9, 6.1), (46.2, 6.9), unit="m")},
]
print(json.dumps(out))
