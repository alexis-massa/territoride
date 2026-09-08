from django.contrib.auth.views import LogoutView
from django.urls import path

from . import views

app_name = "accounts"

urlpatterns = [
    path("strava/authorize/", views.strava_authorize, name="strava_authorize"),
    path("strava/callback/", views.strava_callback, name="strava_callback"),
    path("logout/", LogoutView.as_view(), name="logout"),
    path("profile/", views.profile_view, name="profile"),
    path("strava/sync/", views.sync_strava_view, name="strava_sync"),
    path("delete/", views.delete_account_view, name="delete_account"),
    path("strava/webhook/", views.strava_webhook_view, name="strava_webhook"),
]
