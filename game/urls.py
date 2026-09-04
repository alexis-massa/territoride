from django.urls import path

from . import views

urlpatterns = [
    path("", views.map_view, name="map"),
    path("grid.geojson", views.grid_geojson_view, name="grid_geojson"),
]
