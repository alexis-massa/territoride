from datetime import UTC, datetime
from typing import Any

import h3
from django.contrib.gis.geos import LineString, Point
from django.test import TestCase
from django.urls import reverse

from accounts.models import User

from .capture import capture_territory
from .grid import cell_centroid
from .models import POI, Activity, TerritoryCell
from .scoring import PLAYER_COLORS, player_color, player_colors
from .views import _user_territory_bbox

NOW = datetime(2026, 1, 1, tzinfo=UTC)
# A ~1.5 km zigzag near Annecy (~10 m off-axis wiggles, so it only simplifies when zoomed out).
TRACK_COORDS = [
    (6.10 + i * 0.0001, 45.90 + i * 0.00005 + (0.0001 if i % 2 else 0)) for i in range(150)
]
ANNECY_CELL = h3.latlng_to_cell(45.9, 6.1, 8)


def _bbox(min_lon: float, min_lat: float, max_lon: float, max_lat: float) -> dict[str, str]:
    return {"bbox": f"{min_lon},{min_lat},{max_lon},{max_lat}"}


class GeojsonViewTests(TestCase):
    alice: User
    bob: User
    activity: Activity
    near: str
    far: str

    @classmethod
    def setUpTestData(cls) -> None:
        cls.alice = User.objects.create_user("alice", password="x")
        cls.bob = User.objects.create_user("bob", password="x")
        cls.activity = Activity.objects.create(
            user=cls.alice,
            name="Morning ride",
            track=LineString(TRACK_COORDS, srid=4326),
            recorded_at=NOW,
        )
        cls.near = ANNECY_CELL
        cls.far = "88283082a1fffff"
        for cell, owner in ((cls.near, cls.alice), (cls.far, cls.bob)):
            TerritoryCell.objects.create(
                cell_id=cell,
                centroid=cell_centroid(cell),
                owner=owner,
                captured_by=cls.activity,
                captured_at=NOW,
            )
        POI.objects.create(
            name="Col near",
            location=Point(6.1, 45.9, srid=4326),
            owner=cls.alice,
            claimed_at=NOW,
        )
        POI.objects.create(name="Col far", location=Point(-122.4, 37.7, srid=4326))

    def _features(self, name: str, **bbox: float) -> list[dict[str, Any]]:
        response = self.client.get(reverse(name), _bbox(**bbox))
        self.assertEqual(response.status_code, 200)
        features: list[dict[str, Any]] = response.json()["features"]
        return features

    def test_pois_filtered_by_bbox(self) -> None:
        features = self._features("pois_geojson", min_lon=5, min_lat=45, max_lon=7, max_lat=46)
        self.assertEqual([f["properties"]["name"] for f in features], ["Col near"])
        self.assertEqual(features[0]["geometry"], {"type": "Point", "coordinates": [6.1, 45.9]})
        self.assertEqual(features[0]["properties"]["color"], player_color("alice"))

    def test_unclaimed_pois_omit_claim_properties(self) -> None:
        features = self._features(
            "pois_geojson", min_lon=-123, min_lat=37, max_lon=-122, max_lat=38
        )
        self.assertEqual([f["properties"]["name"] for f in features], ["Col far"])
        self.assertEqual(
            set(features[0]["properties"]),
            {"name", "poi_type", "poi_type_label", "altitude_m"},
        )

    def test_territory_filtered_by_bbox(self) -> None:
        lat, lng = cell_centroid(self.near).y, cell_centroid(self.near).x
        features = self._features(
            "territory_geojson",
            min_lon=lng - 0.1,
            min_lat=lat - 0.1,
            max_lon=lng + 0.1,
            max_lat=lat + 0.1,
        )
        self.assertEqual([f["properties"]["owner"] for f in features], ["alice"])
        ring = features[0]["geometry"]["coordinates"][0]
        self.assertEqual(ring[0], ring[-1])

    def test_territory_whole_world(self) -> None:
        features = self._features(
            "territory_geojson", min_lon=-180, min_lat=-90, max_lon=180, max_lat=90
        )
        self.assertEqual(len(features), 2)

    def test_routes_filtered_by_bbox_and_captured(self) -> None:
        Activity.objects.create(
            user=self.bob,
            name="No capture",
            track=LineString(TRACK_COORDS, srid=4326),
            recorded_at=NOW,
        )
        inside = {"min_lon": 5, "min_lat": 45, "max_lon": 7, "max_lat": 46}
        features = self._features("routes_geojson", **inside)
        self.assertEqual([f["properties"]["name"] for f in features], ["Morning ride"])
        self.assertEqual(features[0]["properties"]["owner"], "alice")
        outside = {"min_lon": -123, "min_lat": 37, "max_lon": -122, "max_lat": 38}
        self.assertEqual(self._features("routes_geojson", **outside), [])

    def test_routes_one_feature_even_with_many_captured_cells(self) -> None:
        features = self._features("routes_geojson", min_lon=5, min_lat=45, max_lon=7, max_lat=46)
        self.assertEqual(len(features), 1)

    def test_routes_simplified_when_zoomed_out(self) -> None:
        zoomed_in = self._features(
            "routes_geojson", min_lon=6.10, min_lat=45.90, max_lon=6.102, max_lat=45.902
        )
        zoomed_out = self._features("routes_geojson", min_lon=0, min_lat=40, max_lon=12, max_lat=50)
        self.assertEqual(len(zoomed_in[0]["geometry"]["coordinates"]), len(TRACK_COORDS))
        self.assertLess(len(zoomed_out[0]["geometry"]["coordinates"]), 20)

    def test_bad_bbox_is_rejected(self) -> None:
        for name in ("pois_geojson", "territory_geojson", "routes_geojson"):
            for params in ({}, {"bbox": "1,2,3"}, {"bbox": "a,b,c,d"}, {"bbox": "nan,0,1,1"}):
                response = self.client.get(reverse(name), params)
                self.assertEqual(response.status_code, 400, (name, params))

    def test_geojson_is_gzipped_on_request(self) -> None:
        response = self.client.get(
            reverse("pois_geojson"),
            _bbox(-180, -90, 180, 90),
            headers={"accept-encoding": "gzip"},
        )
        self.assertEqual(response.headers["Content-Encoding"], "gzip")

    def test_query_count_does_not_grow_with_features(self) -> None:
        url, params = reverse("territory_geojson"), _bbox(-180, -90, 180, 90)
        with self.assertNumQueries(2):  # cells (+owner join), usernames
            self.client.get(url, params)


class TerritoryBboxTests(TestCase):
    def test_user_territory_bbox_spans_cells_and_pois(self) -> None:
        user = User.objects.create_user("carol", password="x")
        self.assertIsNone(_user_territory_bbox(user))

        cell = ANNECY_CELL
        TerritoryCell.objects.create(cell_id=cell, centroid=cell_centroid(cell), owner=user)
        point = cell_centroid(cell)
        min_lon, min_lat, max_lon, max_lat = _user_territory_bbox(user)  # type: ignore[misc]
        self.assertEqual((min_lon, min_lat, max_lon, max_lat), (point.x, point.y, point.x, point.y))

        POI.objects.create(name="Col", location=Point(7.0, 46.5, srid=4326), owner=user)
        bbox = _user_territory_bbox(user)
        assert bbox is not None
        self.assertEqual(bbox[2:], (7.0, 46.5))
        self.assertEqual(bbox[:2], (min(point.x, 7.0), min(point.y, 46.5)))


class CaptureCentroidTests(TestCase):
    def test_captured_cells_store_their_centroid(self) -> None:
        user = User.objects.create_user("dave", password="x")
        loop = [(45.90, 6.10), (45.90, 6.14), (45.93, 6.14), (45.93, 6.10), (45.90, 6.10)]
        activity = Activity.objects.create(
            user=user,
            name="Loop",
            track=LineString([(lng, lat) for lat, lng in loop], srid=4326),
            recorded_at=NOW,
        )
        self.assertGreater(capture_territory(activity), 0)
        for cell in TerritoryCell.objects.all():
            self.assertTrue(cell.centroid.equals_exact(cell_centroid(cell.cell_id), 1e-9))


class PlayerColorTests(TestCase):
    def test_colors_are_assigned_by_user_id_order(self) -> None:
        names = [f"user{i}" for i in range(len(PLAYER_COLORS) + 1)]
        for name in names:
            User.objects.create_user(name, password="x")
        colors = player_colors()
        self.assertEqual([colors[n] for n in names[:-1]], PLAYER_COLORS)
        self.assertEqual(colors[names[-1]], PLAYER_COLORS[0])
        self.assertEqual(player_color("user2"), PLAYER_COLORS[2])

    def test_unknown_username_gets_a_stable_fallback(self) -> None:
        self.assertEqual(player_color("ghost"), player_color("ghost"))
        self.assertIn(player_color("ghost"), PLAYER_COLORS)
