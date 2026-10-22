//! Parseurs de fichiers de télémétrie.

pub mod garage61;
pub mod ibt;

/// Origine du repère métrique : coin de la maille de 0,1° contenant le point.
/// Commune à tous les tours d'un même circuit (et à toutes les sources : .ibt, Garage 61),
/// ce qui rend les trajectoires directement superposables.
pub(crate) fn grid_origin(lat: f64, lon: f64) -> (f64, f64) {
    ((lat * 10.0).floor() / 10.0, (lon * 10.0).floor() / 10.0)
}

/// Projection locale (équirectangulaire) d'un point GPS en mètres : x vers l'est, y vers le nord.
pub(crate) fn latlon_to_xy(lat: f64, lon: f64, origin: (f64, f64)) -> (f32, f32) {
    let k = origin.0.to_radians().cos();
    (((lon - origin.1) * 111_320.0 * k) as f32, ((lat - origin.0) * 110_540.0) as f32)
}
