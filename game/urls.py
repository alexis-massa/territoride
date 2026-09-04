from django.urls import path

from . import views

urlpatterns = [
    path("", views.map_view, name="map"),
    path("grid.geojson", views.grid_geojson_view, name="grid_geojson"),
    path("territory.geojson", views.territory_geojson_view, name="territory_geojson"),
    path("routes.geojson", views.routes_geojson_view, name="routes_geojson"),
    path("pois.geojson", views.pois_geojson_view, name="pois_geojson"),
]
