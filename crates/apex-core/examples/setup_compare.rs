//! Compare deux setups LMU (`.svm`) comme l'onglet Setups : `cargo run -p apex-core --example setup_compare -- a.svm b.svm`
//! Dossier entier : `cargo run -p apex-core --example setup_compare -- <Settings>` liste les setups.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [dir] => {
            for e in apex_core::setup::library(std::path::Path::new(dir)) {
                println!("{} | {} | {}", e.track, e.vehicle_class, e.name);
            }
        }
        [a, b] => {
            let read = |p: &str| apex_core::setup::read_svm(std::path::Path::new(p)).expect("setup illisible");
            let c = apex_core::setup::compare(&read(a), &read(b));
            if std::env::var_os("APEX_JSON").is_some() {
                println!("{}", serde_json::to_string(&c).unwrap());
                return;
            }
            println!("{} → {} ({}) : {} différences, {} identiques", c.a, c.b, if c.same_car { "même voiture" } else { "voitures différentes" }, c.changes.len(), c.unchanged);
            for ch in &c.changes {
                println!("[{}] {} : {} → {} ({:+} crans){}", ch.group, ch.label, ch.from, ch.to, ch.steps, ch.meaning.as_deref().map(|m| format!("\n    {m}")).unwrap_or_default());
            }
        }
        _ => eprintln!("usage : setup_compare <a.svm> <b.svm> | <dossier Settings>"),
    }
}
