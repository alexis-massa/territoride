//! Google encoded polylines (Strava's track format).

use crate::grid::LatLon;

/// Decode an encoded polyline into ordered `(lat, lng)` points.
pub fn decode(encoded: &str) -> Vec<LatLon> {
    let bytes = encoded.as_bytes();
    let (mut index, mut lat, mut lng) = (0usize, 0i64, 0i64);
    let mut points = Vec::new();
    while index < bytes.len() {
        for is_lat in [true, false] {
            let (mut shift, mut result) = (0u32, 0i64);
            loop {
                let Some(&b) = bytes.get(index) else {
                    return points;
                };
                let byte = i64::from(b) - 63;
                index += 1;
                result |= (byte & 0x1f) << shift;
                shift += 5;
                if byte < 0x20 {
                    break;
                }
            }
            let delta = if result & 1 != 0 {
                !(result >> 1)
            } else {
                result >> 1
            };
            if is_lat {
                lat += delta;
            } else {
                lng += delta;
            }
        }
        points.push((lat as f64 / 1e5, lng as f64 / 1e5));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_documented_example() {
        // https://developers.google.com/maps/documentation/utilities/polylinealgorithm
        let points = decode("_p~iF~ps|U_ulLnnqC_mqNvxq`@");
        assert_eq!(
            points,
            [(38.5, -120.2), (40.7, -120.95), (43.252, -126.453)]
        );
    }

    #[test]
    fn truncated_input_gives_the_points_decoded_so_far() {
        assert_eq!(decode("_p~iF~ps|U_ulL"), [(38.5, -120.2)]);
        assert!(decode("").is_empty());
    }
}
