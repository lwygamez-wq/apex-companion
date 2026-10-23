//! Données de course extraites d'un vrai .ibt : `cargo run -p apex-core --example race_check -- course.ibt`
fn main() {
    for path in std::env::args().skip(1) {
        let s = apex_core::ingest::ibt::import_ibt_file(std::path::Path::new(&path)).unwrap();
        println!("== {} {:?} {} tours", s.meta.track, s.meta.kind, s.laps.len());
        println!("{:#?}", s.race);
    }
}
