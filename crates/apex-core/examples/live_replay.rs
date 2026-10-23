//! Rejoue un vrai .ibt comme une session en direct et affiche les messages radio du coach.
//! `cargo run -p apex-core --example live_replay -- session.ibt [références .ibt/.csv Garage 61…]`
//! Les fichiers de référence sont importés d'abord (référence = perso/externe, sinon meilleur tour propre).
use apex_core::live_coach::LiveCoach;
use apex_core::service::AppService;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let svc = AppService::in_memory().unwrap();
    for r in &args[1..] {
        let p = std::path::Path::new(r);
        if r.ends_with(".csv") {
            println!("référence externe : {:?}", svc.import_garage61(p).map(|x| (x.driver, x.lap_time_s)).map_err(|e| e.to_string()));
        } else {
            let s = apex_core::ingest::ibt::import_ibt_file(p).unwrap();
            svc.insert_imported(&s).unwrap();
        }
    }
    let b = std::fs::read(&args[0]).unwrap();
    let (yaml, rows) = apex_core::ingest::ibt::ibt_live_rows(&b).unwrap();
    let meta = { let (c, _) = LiveCoach::new(&yaml, None, vec![]); c.session().clone() };
    let reference = svc.live_reference(&meta).unwrap();
    let (mut coach, hello) = LiveCoach::new(&yaml, reference, vec![]);
    println!("[radio] {}", hello.text);
    let t0 = std::time::Instant::now();
    for r in rows {
        if let Some(m) = coach.push(r) {
            println!("[tour {:>2} {:>8}] {:?} {}", m.lap, m.lap_time_s.map(|t| format!("{t:.3}")).unwrap_or_default(), m.kind, m.text);
        }
    }
    println!("« répète » → {}\n« point » → {}", coach.repeat_text(), coach.status_text());
    println!("({} ms pour toute la session)", t0.elapsed().as_millis());
}
