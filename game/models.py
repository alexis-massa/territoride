from django.conf import settings
from django.contrib.gis.db import models as gis_models
from django.db import models


class Activity(models.Model):
    """A player's recorded GPS track (loaded from GPX for now)."""

    user = models.ForeignKey(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="activities"
    )
    name = models.CharField(max_length=255)
    track = gis_models.LineStringField(srid=4326)
    recorded_at = models.DateTimeField()
    strava_activity_id = models.BigIntegerField(null=True, blank=True, unique=True)

    def __str__(self) -> str:
        """One-line label for admin/debug output.

        Returns:
            The activity name.
        """
        return self.name


class POI(models.Model):
    """A capturable point of interest - mountain passes only for now."""

    name = models.CharField(max_length=255)
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
        """One-line label for admin/debug output.

        Returns:
            The POI's name.
        """
        return self.name


class TerritoryCell(models.Model):
    """An H3 grid cell's current ownership."""

    cell_id = models.CharField(max_length=20, primary_key=True)
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
        """One-line label for admin/debug output.

        Returns:
            The cell id and current owner, if any.
        """
        return f"{self.cell_id} ({self.owner or 'unclaimed'})"
