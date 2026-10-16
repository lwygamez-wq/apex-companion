//! Validation sur de vrais fichiers iRacing.
//!
//! Windows : `set APEX_IBT_DIR=C:\Users\<toi>\Documents\iRacing\telemetry` puis
//! `cargo test -p apex-core --test ibt_real -- --nocapture`.
//! Sans la variable, le test est ignoré (rien n'est simulé).

use apex_core::ingest::ibt::import_ibt_file;

#[test]
fn import_real_ibt_files() {
    let Some(dir) = std::env::var_os("APEX_IBT_DIR") else {
        eprintln!("APEX_IBT_DIR non défini : test ignoré");
        return;
    };
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("ibt") {
            continue;
        }
        let s = import_ibt_file(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        println!("{} — {} / {} / {:?} — {} tours", p.display(), s.meta.track, s.meta.car, s.meta.kind, s.laps.len());
        for l in &s.laps {
            l.trace.validate().unwrap();
            println!("  tour {:>3} {:?} {:?} {:?}", l.meta.number, l.meta.kind, l.meta.lap_time_s, l.meta.validity);
        }
        n += 1;
    }
    assert!(n > 0, "aucun .ibt trouvé");
}
