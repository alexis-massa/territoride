from django.contrib import admin
from django.contrib.gis.admin import GISModelAdmin

from .models import POI, Activity, TerritoryCell


@admin.register(Activity)
class ActivityAdmin(GISModelAdmin):  # type: ignore[type-arg]
    list_display = ("name", "user", "recorded_at")


@admin.register(TerritoryCell)
class TerritoryCellAdmin(admin.ModelAdmin):  # type: ignore[type-arg]
    list_display = ("cell_id", "owner", "captured_by", "captured_at")
    list_filter = ("owner",)


@admin.register(POI)
class POIAdmin(GISModelAdmin):  # type: ignore[type-arg]
    list_display = ("name", "altitude_m", "owner", "claimed_at")
    list_filter = ("owner",)
