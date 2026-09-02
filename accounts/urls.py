from django.contrib.auth.views import LogoutView
from django.urls import path

from . import views

app_name = "accounts"

urlpatterns = [
    path("strava/authorize/", views.strava_authorize, name="strava_authorize"),
    path("strava/callback/", views.strava_callback, name="strava_callback"),
    path("logout/", LogoutView.as_view(), name="logout"),
]
