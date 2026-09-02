from django.contrib import admin
from django.contrib.auth.admin import UserAdmin

from .models import User


@admin.register(User)
class TerritoRideUserAdmin(UserAdmin):  # type: ignore[type-arg]
    list_display = ("username", "athlete_id", "is_staff", "date_joined")
    fieldsets = (
        *UserAdmin.fieldsets,  # type: ignore[misc]  # django-stubs types this as possibly None
        ("Strava", {"fields": ("athlete_id", "strava_token_expires_at")}),
    )
    readonly_fields = ("athlete_id", "strava_token_expires_at")
