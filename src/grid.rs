//! The H3 grid (resolution 8): cells, their outlines and centres, loops and distances.
//! Behaves like the Python version's `game/grid.py` (h3-py): polygons cover the cells whose centre is inside.

use std::collections::HashSet;

use geo_types::{Coord, LineString, Polygon};
use h3o::geom::{ContainmentMode, TilerBuilder};
use h3o::{CellIndex, LatLng, Resolution};

pub const RESOLUTION: Resolution = Resolution::Eight;

/// A point as (latitude, longitude), the order the Python version's helpers use.
pub type LatLon = (f64, f64);

fn latlng(&(lat, lng): &LatLon) -> LatLng {
    LatLng::new(lat, lng).expect("track coordinates are finite")
}

/// The cell containing a point.
pub fn cell_at(point: LatLon) -> CellIndex {
    latlng(&point).to_cell(RESOLUTION)
}

/// Closed `(lng, lat)` ring of a cell, ready for GeoJSON.
pub fn cell_boundary(cell: CellIndex) -> Vec<(f64, f64)> {
    let mut ring: Vec<(f64, f64)> = cell.boundary().iter().map(|p| (p.lng(), p.lat())).collect();
    ring.push(ring[0]);
    ring
}

/// Centre of a cell as `(lng, lat)`.
pub fn cell_centroid(cell: CellIndex) -> (f64, f64) {
    let c = LatLng::from(cell);
    (c.lng(), c.lat())
}

/// Great-circle distance in metres.
pub fn distance_m(a: LatLon, b: LatLon) -> f64 {
    latlng(&a).distance_m(latlng(&b))
}

/// Cells whose centre lies inside the closed `(lat, lng)` loop, always including the cell of the loop's
/// centroid (a polygon smaller than one cell would otherwise cover nothing).
pub fn cells_in_loop(points: &[LatLon]) -> HashSet<CellIndex> {
    let ring: LineString = points
        .iter()
        .map(|&(lat, lng)| Coord { x: lng, y: lat })
        .collect();
    let mut cells = HashSet::new();
    let mut tiler = TilerBuilder::new(RESOLUTION)
        .containment_mode(ContainmentMode::ContainsCentroid)
        .build();
    if tiler.add(Polygon::new(ring, Vec::new())).is_ok() {
        cells.extend(tiler.into_coverage());
    }
    let n = points.len() as f64;
    let centroid = (
        points.iter().map(|p| p.0).sum::<f64>() / n,
        points.iter().map(|p| p.1).sum::<f64>() / n,
    );
    cells.insert(cell_at(centroid));
    cells
}
