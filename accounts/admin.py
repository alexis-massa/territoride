from django.contrib import admin
from django.contrib.auth.admin import UserAdmin

from .models import User


@admin.register(User)
class TerritoRideUserAdmin(UserAdmin):  # type: ignore[type-arg]
    list_display = (
        "username",
        "athlete_id",
        "is_allowed",
        "is_staff",
        "date_joined",
    )
    list_filter = ("is_staff", "is_superuser", "is_active", "is_allowed", "groups")
    fieldsets = (
        (None, {"fields": ("username", "password")}),
        ("Personal info", {"fields": ("first_name", "last_name", "email")}),
        (
            "Permissions",
            {
                "fields": (
                    "is_active",
                    "is_staff",
                    "is_superuser",
                    "groups",
                    "user_permissions",
                )
            },
        ),
        ("Important dates", {"fields": ("last_login", "date_joined")}),
        (
            "Strava",
            {
                "fields": (
                    "athlete_id",
                    "is_allowed",
                    "strava_token_expires_at",
                )
            },
        ),
    )
    readonly_fields = ("athlete_id", "strava_token_expires_at")
