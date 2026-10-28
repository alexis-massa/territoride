from django.urls import path

from . import views

urlpatterns = [
    path("", views.map_view, name="map"),
    path("manifest.webmanifest", views.manifest_view, name="manifest"),
    path("sw.js", views.service_worker_view, name="service_worker"),
    path("territory.geojson", views.territory_geojson_view, name="territory_geojson"),
    path("routes.geojson", views.routes_geojson_view, name="routes_geojson"),
    path("pois.geojson", views.pois_geojson_view, name="pois_geojson"),
    path("leaderboard/", views.leaderboard_view, name="leaderboard"),
    path("rules/", views.rules_view, name="rules"),
    path("changelog/", views.changelog_view, name="changelog"),
    path("import-activity/", views.import_activity_view, name="import_activity"),
]
