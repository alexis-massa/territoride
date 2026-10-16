from django.contrib import admin, messages
from django.contrib.gis.admin import GISModelAdmin

from .capture import recapture_all, release_activity
from .models import POI, Activity, TerritoryCell


@admin.action(description="Recapture all routes (replay every activity's capture)")
def recapture_all_action(modeladmin, request, queryset):  # type: ignore[no-untyped-def]
    """Ignore the selection - this always replays every activity, e.g. after POIs are added."""
    cells, pois = recapture_all()
    modeladmin.message_user(
        request, f"Recaptured {cells} cells and {pois} POIs.", level=messages.SUCCESS
    )


@admin.action(description="Release and delete selected activities")
def release_and_delete_action(modeladmin, request, queryset):  # type: ignore[no-untyped-def]
    """Properly release held cells/POIs before deleting, unlike the default delete action."""
    count = queryset.count()
    for activity in queryset:
        release_activity(activity)
    modeladmin.message_user(
        request, f"Released and deleted {count} activities.", level=messages.SUCCESS
    )


@admin.register(Activity)
class ActivityAdmin(GISModelAdmin):  # type: ignore[type-arg]
    list_display = ("name", "user", "recorded_at")
    actions = [recapture_all_action, release_and_delete_action]


@admin.register(TerritoryCell)
class TerritoryCellAdmin(admin.ModelAdmin):  # type: ignore[type-arg]
    list_display = ("cell_id", "owner", "captured_by", "captured_at")
    list_filter = ("owner",)


@admin.register(POI)
class POIAdmin(GISModelAdmin):  # type: ignore[type-arg]
    list_display = ("name", "poi_type", "altitude_m", "owner", "claimed_at")
    list_filter = ("poi_type", "owner")
    actions = [recapture_all_action]
