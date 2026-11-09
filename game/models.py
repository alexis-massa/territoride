from django.conf import settings
from django.contrib.gis.db import models as gis_models
from django.db import models


class Activity(models.Model):
    """A player's recorded GPS track (loaded from GPX for now)."""

    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="activities"
    )
    name = models.CharField(max_length=255)
    track = gis_models.LineStringField(srid=4326, null=True, blank=True)
    recorded_at = models.DateTimeField()
    strava_activity_id = models.BigIntegerField(null=True, blank=True, unique=True)
    sport_type = models.CharField(max_length=30, blank=True, default="")
    distance_m = models.IntegerField(null=True, blank=True)

    def __str__(self) -> str:
        """Admin/debug label."""
        return self.name


class POI(models.Model):
    """A capturable point of interest - a mountain pass, summit, etc."""

    class POIType(models.TextChoices):
        MOUNTAIN_PASS = "mountain_pass", "Mountain pass"
        SUMMIT = "summit", "Summit"

    name = models.CharField(max_length=255)
    poi_type = models.CharField(
        max_length=30, choices=POIType.choices, default=POIType.MOUNTAIN_PASS
    )
    location = gis_models.PointField(srid=4326)
    altitude_m = models.IntegerField(null=True, blank=True)
    owner = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.SET_NULL, null=True, related_name="claimed_pois"
    )
    claimed_by = models.ForeignKey(
        Activity, on_delete=models.SET_NULL, null=True, related_name="claimed_pois"
    )
    claimed_at = models.DateTimeField(null=True)

    def __str__(self) -> str:
        """Admin/debug label."""
        return self.name


class TerritoryCell(models.Model):
    """An H3 grid cell's current ownership."""

    cell_id = models.CharField(max_length=20, primary_key=True)
    centroid = gis_models.PointField(srid=4326)
    owner = models.ForeignKey(
        settings.AUTH_USER_MODEL,
        on_delete=models.SET_NULL,
        null=True,
        related_name="territory_cells",
    )
    captured_by = models.ForeignKey(
        Activity, on_delete=models.SET_NULL, null=True, related_name="captured_cells"
    )
    captured_at = models.DateTimeField(null=True)

    def __str__(self) -> str:
        """Admin/debug label."""
        return f"{self.cell_id} ({self.owner or 'unclaimed'})"


class CellVisit(models.Model):
    """A player's latest visit to an H3 cell, kept after someone else takes the tile.

    Territory ownership only remembers the current holder, so this is what lets a
    player see everywhere they went recently, including tiles since lost.
    """

    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="cell_visits"
    )
    cell_id = models.CharField(max_length=20)
    centroid = gis_models.PointField(srid=4326)
    # Cascades with the activity, so deleting or expiring a ride also drops its visits.
    activity = models.ForeignKey(Activity, on_delete=models.CASCADE, related_name="cell_visits")
    visited_at = models.DateTimeField()

    class Meta:
        constraints = [
            models.UniqueConstraint(fields=["user", "cell_id"], name="unique_visit_per_user_cell")
        ]

    def __str__(self) -> str:
        """Admin/debug label."""
        return f"{self.cell_id} ({self.user})"
