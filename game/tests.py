from datetime import UTC, datetime, timedelta
from importlib import import_module
from typing import Any

import h3
from django.apps import apps as django_apps
from django.contrib.gis.geos import LineString, Point
from django.test import TestCase
from django.urls import reverse

from accounts.models import User

from .capture import capture_territory, release_activity
from .grid import cell_centroid
from .models import POI, Activity, CellVisit, TerritoryCell
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


class GeojsonFixture(TestCase):
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



class GeojsonViewTests(GeojsonFixture):
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


class MineOnlyTests(GeojsonFixture):
    """The "?mine=1" view: only the logged-in player's own data."""

    WORLD = {"min_lon": -180, "min_lat": -90, "max_lon": 180, "max_lat": 90}

    def _mine(self, name: str) -> list[dict[str, Any]]:
        response = self.client.get(reverse(name), {**_bbox(-180, -90, 180, 90), "mine": "1"})
        self.assertEqual(response.status_code, 200)
        features: list[dict[str, Any]] = response.json()["features"]
        return features

    def test_requires_login(self) -> None:
        for name in ("pois_geojson", "territory_geojson", "routes_geojson"):
            response = self.client.get(reverse(name), {**_bbox(-180, -90, 180, 90), "mine": "1"})
            self.assertEqual(response.status_code, 403, name)

    def _visit(self, user: User, cell: str, at: datetime = NOW) -> None:
        ride = Activity.objects.create(user=user, name="Ride", recorded_at=at)
        CellVisit.objects.create(
            user=user, cell_id=cell, centroid=cell_centroid(cell), activity=ride, visited_at=at
        )

    def test_territory_shows_own_visits_including_tiles_since_lost(self) -> None:
        self._visit(self.bob, self.near)  # alice holds this one now
        self._visit(self.bob, self.far)
        self._visit(self.alice, self.near)
        self.client.force_login(self.bob)
        features = self._mine("territory_geojson")
        self.assertEqual(len(features), 2)
        self.assertEqual({f["properties"]["owner"] for f in features}, {"bob"})
        self.assertIn("age_days", features[0]["properties"])
        self.client.force_login(self.alice)
        self.assertEqual(len(self._mine("territory_geojson")), 1)

    def test_territory_ignores_held_cells_never_visited(self) -> None:
        self.client.force_login(self.bob)  # bob holds `far` but has no recorded visit
        self.assertEqual(self._mine("territory_geojson"), [])

    def test_routes_include_own_rides_that_hold_no_cells(self) -> None:
        Activity.objects.create(
            user=self.bob,
            name="Retaken ride",
            track=LineString(TRACK_COORDS, srid=4326),
            recorded_at=NOW,
        )
        self.client.force_login(self.bob)
        features = self._mine("routes_geojson")
        self.assertEqual([f["properties"]["name"] for f in features], ["Retaken ride"])
        self.assertIn("age_days", features[0]["properties"])

    def test_routes_exclude_other_players(self) -> None:
        self.client.force_login(self.bob)
        names = [f["properties"]["name"] for f in self._mine("routes_geojson")]
        self.assertNotIn("Morning ride", names)

    def test_pois_held_by_others_show_as_unclaimed(self) -> None:
        self.client.force_login(self.bob)
        by_name = {f["properties"]["name"]: f["properties"] for f in self._mine("pois_geojson")}
        self.assertEqual(set(by_name), {"Col near", "Col far"})
        self.assertNotIn("owner", by_name["Col near"])  # alice's, hidden from bob
        self.client.force_login(self.alice)
        by_name = {f["properties"]["name"]: f["properties"] for f in self._mine("pois_geojson")}
        self.assertEqual(by_name["Col near"]["owner"], "alice")
        self.assertNotIn("owner", by_name["Col far"])

    def test_default_view_still_shows_everyone(self) -> None:
        self.client.force_login(self.bob)
        features = self._features("territory_geojson", **self.WORLD)
        self.assertEqual(len(features), 2)


class CellVisitTests(TestCase):
    alice: User
    bob: User

    @classmethod
    def setUpTestData(cls) -> None:
        cls.alice = User.objects.create_user("alice", password="x")
        cls.bob = User.objects.create_user("bob", password="x")

    def _ride(self, user: User, days: int) -> Activity:
        activity = Activity.objects.create(
            user=user,
            name=f"{user.username} day {days}",
            track=LineString(TRACK_COORDS, srid=4326),
            recorded_at=NOW + timedelta(days=days),
        )
        capture_territory(activity)
        return activity

    def _visits(self, user: User) -> dict[str, datetime]:
        return dict(CellVisit.objects.filter(user=user).values_list("cell_id", "visited_at"))

    def test_capture_records_a_visit_per_cell(self) -> None:
        self._ride(self.alice, 0)
        held = set(TerritoryCell.objects.values_list("cell_id", flat=True))
        self.assertEqual(set(self._visits(self.alice)), held)
        self.assertIn(ANNECY_CELL, self._visits(self.alice))

    def test_visit_survives_losing_the_tile(self) -> None:
        self._ride(self.alice, 0)
        self._ride(self.bob, 1)
        self.assertEqual(TerritoryCell.objects.get(cell_id=ANNECY_CELL).owner, self.bob)
        self.assertIn(ANNECY_CELL, self._visits(self.alice))
        self.assertIn(ANNECY_CELL, self._visits(self.bob))

    def test_newer_ride_refreshes_the_visit(self) -> None:
        self._ride(self.alice, 0)
        self._ride(self.alice, 5)
        self.assertEqual(self._visits(self.alice)[ANNECY_CELL], NOW + timedelta(days=5))
        self.assertEqual(CellVisit.objects.filter(user=self.alice).count(),
            len(self._visits(self.alice)),
        )

    def test_late_synced_older_ride_does_not_replace_a_newer_visit(self) -> None:
        self._ride(self.alice, 5)
        self._ride(self.alice, 0)
        self.assertEqual(self._visits(self.alice)[ANNECY_CELL], NOW + timedelta(days=5))

    def test_deleting_a_ride_falls_back_to_the_previous_visit(self) -> None:
        self._ride(self.alice, 0)
        newest = self._ride(self.alice, 5)
        release_activity(newest)
        self.assertEqual(self._visits(self.alice)[ANNECY_CELL], NOW)

    def test_backfill_rebuilds_visits_from_existing_activities(self) -> None:
        self._ride(self.alice, 0)
        self._ride(self.alice, 5)
        self._ride(self.bob, 2)
        expected = {u: self._visits(u) for u in (self.alice, self.bob)}
        CellVisit.objects.all().delete()
        backfill = import_module("game.migrations.0011_cellvisit_backfill")
        backfill.backfill_visits(django_apps, None)
        self.assertEqual({u: self._visits(u) for u in (self.alice, self.bob)}, expected)

    def test_deleting_the_only_ride_drops_its_visits(self) -> None:
        release_activity(self._ride(self.alice, 0))
        self.assertFalse(CellVisit.objects.exists())


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
