import h3

RESOLUTION = 8


def cell_boundary(cell: str) -> list[tuple[float, float]]:
    """Closed (lng, lat) ring for a cell, ready for GeoJSON.

    Returns:
        Polygon ring coordinates, first point repeated last.
    """
    ring = [(lng, lat) for lat, lng in h3.cell_to_boundary(cell)]
    return [*ring, ring[0]]


def cells_in_bbox(min_lon: float, min_lat: float, max_lon: float, max_lat: float) -> set[str]:
    """H3 cells covering a lon/lat bounding box.

    Returns:
        Cell indexes.
    """
    corners = [(min_lat, min_lon), (min_lat, max_lon), (max_lat, max_lon), (max_lat, min_lon)]
    return _cells_covering(corners)


def cells_in_loop(points: list[tuple[float, float]]) -> set[str]:
    """H3 cells enclosed by a closed (lat, lng) loop.

    Args:
        points: Ordered (lat, lng) points forming the loop.

    Returns:
        Cell indexes.
    """
    return _cells_covering(points)


def _cells_covering(points: list[tuple[float, float]]) -> set[str]:
    """H3 cells whose center falls inside a (lat, lng) polygon ring.

    Args:
        points: Ordered (lat, lng) points forming the ring.

    Returns:
        Cell indexes, always including the ring's own centroid cell -
        polygon_to_cells alone misses polygons smaller than one cell.
    """
    cells = set(h3.polygon_to_cells(h3.LatLngPoly(points), RESOLUTION))
    centroid_lat = sum(lat for lat, _ in points) / len(points)
    centroid_lng = sum(lng for _, lng in points) / len(points)
    cells.add(h3.latlng_to_cell(centroid_lat, centroid_lng, RESOLUTION))
    return cells
