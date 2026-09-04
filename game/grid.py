import h3

RESOLUTION = 7


def cell_boundary(cell: str) -> list[tuple[float, float]]:
    """Closed (lng, lat) ring for a cell, ready for GeoJSON.

    Returns:
        Polygon ring coordinates, first point repeated last.
    """
    ring = [(lng, lat) for lat, lng in h3.cell_to_boundary(cell)]
    return [*ring, ring[0]]


def cells_in_bbox(min_lon: float, min_lat: float, max_lon: float, max_lat: float) -> list[str]:
    """H3 cell indexes covering a lon/lat bounding box.

    Returns:
        Cell indexes.
    """
    corners = [(min_lat, min_lon), (min_lat, max_lon), (max_lat, max_lon), (max_lat, min_lon)]
    cells = set(h3.polygon_to_cells(h3.LatLngPoly(corners), RESOLUTION))
    center = h3.latlng_to_cell((min_lat + max_lat) / 2, (min_lon + max_lon) / 2, RESOLUTION)
    cells.add(center)  # covers the bbox-smaller-than-one-cell case polygon_to_cells misses
    return list(cells)
