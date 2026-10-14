#!/usr/bin/env python3
"""Extract named summits/mountain passes from a Geofabrik .osm.pbf extract into NDJSON.

Runs standalone (no Django/DB) against a locally downloaded extract from
https://download.geofabrik.de/. Load the output on the Pi with
`manage.py import_osm_pois_from_file <poi_type> <path>`.

Usage:
    uv run --group osm-extract python scripts/extract_osm_pois.py summit europe-latest.osm.pbf \\
        > summits.ndjson
"""

import argparse
import json
import sys

import osmium

SUMMIT_MIN_ELEVATION_M = 1000


def _parse_altitude(ele: str | None) -> int | None:
    if ele is None:
        return None
    try:
        return round(float(ele))
    except ValueError:
        return None


class _Handler(osmium.SimpleHandler):
    def __init__(self, poi_type: str, out) -> None:
        super().__init__()
        self.poi_type = poi_type
        self.out = out
        self.matched = 0

    def node(self, n: osmium.osm.Node) -> None:
        name = n.tags.get("name")
        if not name:
            return
        altitude_m = _parse_altitude(n.tags.get("ele"))
        if self.poi_type == "mountain_pass":
            if n.tags.get("mountain_pass") != "yes":
                return
        else:
            if n.tags.get("natural") != "peak" or altitude_m is None:
                return
            significant = (
                altitude_m > SUMMIT_MIN_ELEVATION_M
                or n.tags.get("wikipedia")
                or n.tags.get("prominence")
            )
            if not significant:
                return
        self.out.write(
            json.dumps(
                {
                    "name": name,
                    "poi_type": self.poi_type,
                    "lat": n.location.lat,
                    "lon": n.location.lon,
                    "altitude_m": altitude_m,
                }
            )
            + "\n"
        )
        self.matched += 1


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("poi_type", choices=["summit", "mountain_pass"])
    parser.add_argument("pbf_path")
    args = parser.parse_args()

    handler = _Handler(args.poi_type, sys.stdout)
    handler.apply_file(args.pbf_path)
    print(f"{handler.matched} matched", file=sys.stderr)


if __name__ == "__main__":
    main()
