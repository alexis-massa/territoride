//! The Rust grid and capture rules against the Python version (h3-py, Django): 91 synthetic tracks recorded
//! with `scripts/gen_geo_fixtures.py` must give the same loops, cells and outlines. No database needed.

use std::collections::BTreeSet;

use h3o::CellIndex;
use serde_json::Value;
use territoride::capture::{captured_cells, is_loop, touched_cells};
use territoride::grid::{self, LatLon};
use territoride::polyline;

const FIXTURE: &str = include_str!("fixtures/geo.json");

fn points(v: &Value) -> Vec<LatLon> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
        .collect()
}

fn hex(cells: impl IntoIterator<Item = CellIndex>) -> BTreeSet<String> {
    cells.into_iter().map(|c| c.to_string()).collect()
}

fn wanted(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn tracks_give_the_same_loops_and_cells_as_python() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let (mut loops, mut cells_checked, mut different) = (0, 0, Vec::new());
    for (name, t) in fixture["tracks"].as_object().unwrap() {
        let pts = points(&t["points"]);
        assert_eq!(
            is_loop(&pts),
            t["is_loop"].as_bool().unwrap(),
            "is_loop, {name}"
        );
        assert_eq!(
            hex(touched_cells(&pts)),
            wanted(&t["touched"]),
            "touched cells, {name}"
        );
        if t.get("python_error").is_some() {
            // Python raises ValueError here; Rust falls back to the touched cells plus the centroid cell.
            assert!(
                hex(captured_cells(&pts)).is_superset(&wanted(&t["touched"])),
                "{name}"
            );
            continue;
        }
        let got = hex(captured_cells(&pts));
        if got != wanted(&t["captured"]) {
            different.push(format!(
                "{name}: {} vs {}",
                got.len(),
                wanted(&t["captured"]).len()
            ));
        }
        if t["is_loop"].as_bool().unwrap() {
            loops += 1;
            if hex(grid::cells_in_loop(&pts)) != wanted(&t["in_loop"]) {
                different.push(format!("{name} (loop fill)"));
            }
        }
        cells_checked += got.len();
    }
    println!("{loops} loops, {cells_checked} captured cells compared");
    assert!(
        different.is_empty(),
        "tracks that differ from Python: {different:?}"
    );
}

#[test]
fn cell_outlines_and_centres_match_python() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for (id, c) in fixture["cells"].as_object().unwrap() {
        let cell: CellIndex = id.parse().unwrap();
        let boundary = grid::cell_boundary(cell);
        let want = c["boundary"].as_array().unwrap();
        assert_eq!(boundary.len(), want.len(), "{id}");
        for (got, want) in boundary.iter().zip(want) {
            assert!(
                (got.0 - want[0].as_f64().unwrap()).abs() < 1e-9
                    && (got.1 - want[1].as_f64().unwrap()).abs() < 1e-9,
                "{id}"
            );
        }
        let (lng, lat) = grid::cell_centroid(cell);
        assert!(
            (lng - c["centroid"][0].as_f64().unwrap()).abs() < 1e-9
                && (lat - c["centroid"][1].as_f64().unwrap()).abs() < 1e-9,
            "{id}"
        );
    }
}

#[test]
fn polylines_and_distances_match_python() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for p in fixture["polylines"].as_array().unwrap() {
        assert_eq!(
            polyline::decode(p["encoded"].as_str().unwrap()),
            points(&p["decoded"])
        );
    }
    for d in fixture["great_circle"].as_array().unwrap() {
        let (a, b) = (
            points(&Value::Array(vec![d["a"].clone()]))[0],
            points(&Value::Array(vec![d["b"].clone()]))[0],
        );
        let want = d["m"].as_f64().unwrap();
        assert!(
            (grid::distance_m(a, b) - want).abs() / want < 1e-9,
            "{a:?} {b:?}"
        );
    }
}
