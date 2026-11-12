//! Setups Le Mans Ultimate (`.svm`) : lecture et comparaison.
//!
//! Un `.svm` est un texte `[SECTION]` / `CléSetting=cran//valeur affichée`. Le jeu n'y écrit que le cran
//! choisi et la valeur affichée de ce cran : ni le minimum ni le maximum, ni la valeur des autres crans.
//! Apex ne fait donc que lire et comparer : aucune valeur n'est déduite ni proposée ici.
//!
//! Les setups vendus (GOFast, Coach Dave…) se lisent sur le PC du pilote ; ils ne sont jamais copiés
//! dans le dépôt ni dans le corpus.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SetupSetting {
    pub section: String,
    pub key: String,
    pub index: i32,
    /// Valeur affichée par le jeu pour ce cran (« -1.4 deg », « P19 »).
    pub display: String,
}

impl SetupSetting {
    /// Réglage figé ou sans objet pour cette voiture.
    pub fn fixed(&self) -> bool {
        let d = self.display.trim().to_ascii_lowercase();
        d == "n/a" || d.starts_with("non-adjustable") || d.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupFile {
    pub path: PathBuf,
    /// Nom du fichier sans extension.
    pub name: String,
    /// Dossier du circuit (`Imola`, `Paulricard`…).
    pub track: String,
    /// `VehicleClassSetting` (« Hypercar Peugeot_9x8 WEC2026 »).
    pub vehicle_class: String,
    pub notes: String,
    pub settings: Vec<SetupSetting>,
}

/// Texte d'un `.svm` : UTF-8, sinon Latin-1 (anciens fichiers).
fn text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

pub fn parse_svm(raw: &str) -> (String, String, Vec<SetupSetting>) {
    let (mut section, mut class, mut notes, mut out) = (String::new(), String::new(), String::new(), vec![]);
    for line in raw.lines() {
        let l = line.trim();
        if l.starts_with("//") || l.is_empty() {
            continue;
        }
        if l.starts_with('[') && l.ends_with(']') {
            section = l[1..l.len() - 1].to_string();
            continue;
        }
        let Some((key, rest)) = l.split_once('=') else { continue };
        if key == "VehicleClassSetting" {
            class = rest.trim_matches('"').to_string();
            continue;
        }
        if key == "Notes" {
            notes = rest.trim_matches('"').to_string();
            continue;
        }
        if !key.ends_with("Setting") {
            continue;
        }
        let (idx, display) = match rest.split_once("//") {
            Some((i, d)) => (i, d.trim()),
            None => (rest, ""),
        };
        let Ok(index) = idx.trim().parse::<i32>() else { continue };
        out.push(SetupSetting { section: section.clone(), key: key.to_string(), index, display: display.to_string() });
    }
    (class, notes, out)
}

pub fn read_svm(path: &Path) -> std::io::Result<SetupFile> {
    let raw = text(&std::fs::read(path)?);
    let (vehicle_class, notes, settings) = parse_svm(&raw);
    Ok(SetupFile {
        path: path.to_path_buf(),
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        track: path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        vehicle_class,
        notes,
        settings,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupEntry {
    pub path: PathBuf,
    pub name: String,
    pub track: String,
    pub vehicle_class: String,
    pub modified_unix: i64,
}

/// Tous les `.svm` d'un dossier `Settings` de LMU (un sous-dossier par circuit), du plus récent au plus ancien.
pub fn library(settings_dir: &Path) -> Vec<SetupEntry> {
    let mut out = vec![];
    let Ok(tracks) = std::fs::read_dir(settings_dir) else { return out };
    for t in tracks.flatten().filter(|e| e.path().is_dir()) {
        let Ok(files) = std::fs::read_dir(t.path()) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svm")) {
                continue;
            }
            let Ok(s) = read_svm(&p) else { continue };
            let modified_unix = f.metadata().ok().and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            out.push(SetupEntry { path: p, name: s.name, track: s.track, vehicle_class: s.vehicle_class, modified_unix });
        }
    }
    out.sort_by(|a, b| a.track.cmp(&b.track).then(a.vehicle_class.cmp(&b.vehicle_class)).then(a.name.cmp(&b.name)));
    out
}

/// Dossiers `Settings` de LMU trouvés dans les bibliothèques Steam (`libraryfolders.vdf`).
pub fn find_settings_dirs() -> Vec<PathBuf> {
    let mut steam = vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"C:\Program Files\Steam")];
    let mut libs: Vec<PathBuf> = vec![];
    for s in steam.drain(..) {
        if let Ok(vdf) = std::fs::read_to_string(s.join("steamapps").join("libraryfolders.vdf")) {
            libs.extend(vdf_paths(&vdf));
        }
        libs.push(s);
    }
    let mut out: Vec<PathBuf> = vec![];
    for l in libs {
        let d = l.join("steamapps").join("common").join("Le Mans Ultimate").join("UserData").join("player").join("Settings");
        if d.is_dir() && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// Chemins des bibliothèques dans `libraryfolders.vdf` (`"path"  "F:\\Programmes\\Steam"`).
pub fn vdf_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let mut parts = l.split('"').filter(|s| !s.trim().is_empty());
            (parts.next()? == "path").then(|| parts.next().map(|p| PathBuf::from(p.replace("\\\\", "\\"))))?
        })
        .collect()
}

// --- Comparaison ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SetupChange {
    /// Groupe affiché (« Aéro », « Suspension AV »…).
    pub group: String,
    /// Nom du réglage en français, côté compris (« Carrossage AV »).
    pub label: String,
    pub key: String,
    pub from: String,
    pub to: String,
    pub from_index: i32,
    pub to_index: i32,
    /// Écart en crans (B − A).
    pub steps: i32,
    /// Sens physique de l'écart, sans chiffre de gain (absent quand le sens dépend de la voiture).
    pub meaning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupComparison {
    pub a: String,
    pub b: String,
    pub same_car: bool,
    pub vehicle_a: String,
    pub vehicle_b: String,
    pub changes: Vec<SetupChange>,
    /// Réglages réglables identiques des deux côtés.
    pub unchanged: usize,
}

fn corner(section: &str) -> Option<(&'static str, &'static str)> {
    Some(match section {
        "FRONTLEFT" => ("AV", "AvG"),
        "FRONTRIGHT" => ("AV", "AvD"),
        "REARLEFT" => ("AR", "ArG"),
        "REARRIGHT" => ("AR", "ArD"),
        _ => return None,
    })
}

/// Nom français d'un réglage, et groupe d'affichage.
fn label(section: &str, key: &str) -> (String, String) {
    let k = key.trim_end_matches("Setting");
    if let Some((axle, _)) = corner(section) {
        let name = match k {
            "Camber" => "Carrossage",
            "Pressure" => "Pression pneus",
            "Packer" => "Butée (packer)",
            "Spring" => "Ressort",
            "TenderSpring" => "Ressort auxiliaire",
            "TenderTravel" => "Course ressort auxiliaire",
            "SpringRubber" => "Tampon de ressort",
            "RideHeight" => "Hauteur de caisse",
            "SlowBump" => "Amortisseur compression lente",
            "FastBump" => "Amortisseur compression rapide",
            "SlowRebound" => "Amortisseur détente lente",
            "FastRebound" => "Amortisseur détente rapide",
            "BrakeDisc" => "Disque de frein",
            "BrakePad" => "Plaquettes",
            "Compound" => "Gomme",
            _ => k,
        };
        return (format!("{name} {axle}"), format!("Roues {axle}"));
    }
    let (name, group) = match (section, k) {
        ("GENERAL", "Fuel") => ("Carburant (ratio en Hypercar)", "Général"),
        ("GENERAL", "NumPitstops") => ("Nombre d'arrêts prévus", "Général"),
        ("GENERAL", "FuelCapacity") => ("Carburant (litres)", "Général"),
        ("GENERAL", "VirtualEnergy") => ("Énergie virtuelle", "Général"),
        ("FRONTWING", "FW") => ("Aileron avant", "Aéro"),
        ("REARWING", "RW") => ("Aileron arrière", "Aéro"),
        ("BODYAERO", "WaterRadiator") => ("Radiateur d'eau (scotch)", "Aéro"),
        ("BODYAERO", "OilRadiator") => ("Radiateur d'huile (scotch)", "Aéro"),
        ("BODYAERO", "BrakeDuct") => ("Écopes de frein AV", "Aéro"),
        ("BODYAERO", "BrakeDuctRear") => ("Écopes de frein AR", "Aéro"),
        (_, "FrontAntiSway") => ("Barre antiroulis AV", "Suspension"),
        (_, "RearAntiSway") => ("Barre antiroulis AR", "Suspension"),
        (_, "FrontToeIn") => ("Pincement AV", "Suspension"),
        (_, "RearToeIn") => ("Pincement AR", "Suspension"),
        (_, "Front3rdPacker") => ("Butée 3e élément AV", "Suspension"),
        (_, "Rear3rdPacker") => ("Butée 3e élément AR", "Suspension"),
        (_, "Front3rdSpring") => ("Ressort 3e élément AV", "Suspension"),
        (_, "Rear3rdSpring") => ("Ressort 3e élément AR", "Suspension"),
        (_, "Front3rdSlowBump") => ("3e élément AV compression lente", "Suspension"),
        (_, "Front3rdFastBump") => ("3e élément AV compression rapide", "Suspension"),
        (_, "Front3rdSlowRebound") => ("3e élément AV détente lente", "Suspension"),
        (_, "Front3rdFastRebound") => ("3e élément AV détente rapide", "Suspension"),
        (_, "Rear3rdSlowBump") => ("3e élément AR compression lente", "Suspension"),
        (_, "Rear3rdFastBump") => ("3e élément AR compression rapide", "Suspension"),
        (_, "Rear3rdSlowRebound") => ("3e élément AR détente lente", "Suspension"),
        (_, "Rear3rdFastRebound") => ("3e élément AR détente rapide", "Suspension"),
        (_, "SteerLock") => ("Angle de braquage", "Commandes"),
        (_, "RearBrake") => ("Répartition de freinage (AV:AR)", "Freins"),
        (_, "BrakeMigration") => ("Migration de freinage", "Freins"),
        (_, "BrakePressure") => ("Pression de freinage", "Freins"),
        (_, "TractionControlMap") => ("Antipatinage (TC)", "Aides"),
        (_, "TCPowerCutMap") => ("Antipatinage coupure (TC cut)", "Aides"),
        (_, "TCSlipAngleMap") => ("Antipatinage glisse (TC slip)", "Aides"),
        (_, "AntilockBrakeSystemMap") => ("ABS", "Aides"),
        (_, "RevLimit") => ("Limiteur de régime", "Moteur"),
        (_, "EngineMixture") => ("Mélange moteur", "Moteur"),
        (_, "EngineBrakingMap") => ("Frein moteur", "Moteur"),
        (_, "RegenerationMap") => ("Récupération hybride", "Hybride"),
        (_, "ElectricMotorMap") => ("Puissance électrique", "Hybride"),
        (_, "FinalDrive") => ("Rapport final", "Boîte"),
        (_, "RatioSet") => ("Étagement de boîte", "Boîte"),
        (_, "DiffPower") => ("Différentiel en accélération", "Différentiel"),
        (_, "DiffCoast") => ("Différentiel au lâcher", "Différentiel"),
        (_, "DiffPreload") => ("Précharge du différentiel", "Différentiel"),
        _ => (k, "Autres"),
    };
    let name = if k.starts_with("Gear") && k.len() <= 5 { format!("Rapport {}", &k[4..]) } else { name.to_string() };
    (name, if k.starts_with("Gear") { "Boîte".into() } else { group.into() })
}

/// Premier nombre d'une valeur affichée (« -1.4 deg » → -1,4 ; « P19 » → 19 ; « 54.1:45.9 » → 54,1).
pub fn number(display: &str) -> Option<f64> {
    let s = display.trim();
    let start = s.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let tail = &s[start..];
    let end = tail.char_indices().skip(1).find(|(_, c)| !(c.is_ascii_digit() || *c == '.')).map(|(i, _)| i).unwrap_or(tail.len());
    tail[..end].parse().ok()
}

/// Sens physique d'un écart, d'après la valeur affichée quand elle est numérique (sinon le cran).
/// Seulement des tendances admises en dynamique du véhicule ; l'ampleur dépend de la voiture et se mesure.
fn meaning(section: &str, key: &str, from: &SetupSetting, to: &SetupSetting) -> Option<String> {
    let k = key.trim_end_matches("Setting");
    let up = match (number(&from.display), number(&to.display)) {
        (Some(a), Some(b)) if (b - a).abs() > 1e-9 => b > a,
        _ => to.index > from.index,
    };
    let axle = corner(section).map(|c| c.0);
    let s = |a: &str, b: &str| Some(if up { a } else { b }.to_string());
    match (k, axle) {
        ("RW", _) => s("Plus d'appui à l'arrière (stabilité en courbe rapide), plus de traînée en ligne droite.", "Moins d'appui à l'arrière, moins de traînée : plus de vitesse de pointe, arrière plus vif en courbe rapide."),
        ("FW", _) => s("Plus d'appui à l'avant : l'avant tourne mieux en courbe rapide.", "Moins d'appui à l'avant : moins de rotation en courbe rapide."),
        ("FrontAntiSway", _) => s("Barre AV plus raide : réponse plus directe, tendance au sous-virage.", "Barre AV plus souple : plus d'adhérence à l'avant en appui, réponse plus lente."),
        ("RearAntiSway", _) => s("Barre AR plus raide : la voiture tourne plus, l'arrière décroche plus facilement.", "Barre AR plus souple : arrière plus stable, voiture moins vive."),
        ("Spring", Some("AV")) => s("Ressorts AV plus raides : plateforme aéro plus stable, moins d'adhérence mécanique à l'avant (sous-virage en lent).", "Ressorts AV plus souples : plus d'adhérence mécanique à l'avant, caisse qui bouge plus."),
        ("Spring", Some("AR")) => s("Ressorts AR plus raides : moins d'adhérence mécanique à l'arrière (motricité), voiture plus vive.", "Ressorts AR plus souples : plus de motricité en sortie de virage lent."),
        ("Camber", _) => {
            // Carrossage négatif : plus négatif = valeur plus petite.
            let more_neg = !up;
            Some(if more_neg { "Carrossage plus négatif : plus d'adhérence en appui, l'intérieur du pneu chauffe et s'use plus, freinage en ligne un peu moins bon." } else { "Carrossage moins négatif : meilleure surface au freinage et en accélération, moins d'adhérence en appui." }.to_string())
        }
        ("Pressure", _) => s("Pression plus haute : pneu plus réactif, contact réduit si trop haute (glisse, chauffe au centre).", "Pression plus basse : plus de surface de contact, pneu plus mou, chauffe sur les bords si trop basse."),
        ("RideHeight", Some("AV")) => s("Avant plus haut : moins de rake, équilibre aéro vers l'arrière, moins de risque de talonner.", "Avant plus bas : plus d'appui à l'avant, risque de talonner (vérifier sur les bosses et vibreurs)."),
        ("RideHeight", Some("AR")) => s("Arrière plus haut : plus de rake, équilibre aéro vers l'avant (plus de rotation), arrière moins stable.", "Arrière plus bas : moins de rake, arrière plus stable, moins de rotation."),
        ("FrontToeIn", _) => s("Moins d'ouverture à l'avant : avant plus stable en ligne, entrée en virage moins incisive.", "Plus d'ouverture à l'avant : entrée en virage plus incisive, un peu plus d'usure et de chauffe."),
        ("RearToeIn", _) => s("Plus de pincement à l'arrière : arrière plus stable (entrée et sortie), un peu plus de traînée et de chauffe.", "Moins de pincement à l'arrière : voiture plus vive, arrière moins stable."),
        ("RearBrake", _) => {
            // Affichage « AV:AR » : un premier nombre plus grand = plus de freinage à l'avant.
            s("Freinage plus à l'avant : plus stable au freinage, risque de bloquer l'avant.", "Freinage plus à l'arrière : aide la voiture à tourner au freinage, risque de bloquer l'arrière.")
        }
        ("BrakePressure", _) => s("Pression de freinage plus haute : plus de freinage pour le même effort, blocage plus facile.", "Pression plus basse : freinage plus progressif, il faut appuyer plus fort."),
        ("TractionControlMap" | "TCPowerCutMap" | "TCSlipAngleMap", _) => s("Plus d'antipatinage : sorties plus sûres, un peu moins de motricité si trop.", "Moins d'antipatinage : plus de motricité possible, plus de risque de patinage."),
        ("AntilockBrakeSystemMap", _) => None,
        ("DiffCoast", _) => s("Plus de blocage au lâcher : entrée en virage plus stable, moins de rotation.", "Moins de blocage au lâcher : la voiture tourne plus en entrée, arrière plus mobile."),
        ("DiffPower", _) => s("Plus de blocage en accélération : meilleure motricité en ligne, sous-virage en sortie.", "Moins de blocage en accélération : la voiture tourne plus en sortie, roue intérieure qui patine plus."),
        ("DiffPreload", _) => s("Précharge plus forte : comportement plus stable, moins de rotation au milieu du virage.", "Précharge plus faible : plus de rotation, transitions moins stables."),
        ("WaterRadiator" | "OilRadiator", _) => s("Plus de scotch : moins de refroidissement, moins de traînée (surveiller les températures).", "Moins de scotch : plus de refroidissement, plus de traînée."),
        ("BrakeDuct" | "BrakeDuctRear", _) => None,
        ("ElectricMotorMap", _) => s("Plus de puissance électrique : plus de déploiement, l'énergie se vide plus vite.", "Moins de puissance électrique : l'énergie dure plus longtemps."),
        ("RegenerationMap", _) => s("Plus de récupération au freinage : plus d'énergie récupérée, frein moteur plus fort sur l'arrière.", "Moins de récupération : arrière plus libre au freinage."),
        ("Fuel", _) if section == "GENERAL" => None,
        _ => None,
    }
}

pub fn compare(a: &SetupFile, b: &SetupFile) -> SetupComparison {
    let mut raw: Vec<(String, String, &SetupSetting, &SetupSetting)> = vec![];
    let mut unchanged = 0;
    for x in &a.settings {
        let Some(y) = b.settings.iter().find(|y| y.section == x.section && y.key == x.key) else { continue };
        if x.index == y.index && x.display == y.display {
            if !x.fixed() {
                unchanged += 1;
            }
            continue;
        }
        raw.push((x.section.clone(), x.key.clone(), x, y));
    }
    let mut changes: Vec<SetupChange> = vec![];
    for (section, key, x, y) in &raw {
        let (lab, group) = label(section, key);
        let side = corner(section);
        // Gauche et droite changés pareil : une seule ligne pour l'essieu.
        if let Some((axle, wheel)) = side {
            let twin = match section.as_str() {
                "FRONTLEFT" => "FRONTRIGHT",
                "FRONTRIGHT" => "FRONTLEFT",
                "REARLEFT" => "REARRIGHT",
                _ => "REARLEFT",
            };
            let same = raw.iter().any(|(s2, k2, x2, y2)| s2 == twin && k2 == key && x2.index == x.index && y2.index == y.index);
            if same {
                if section.ends_with("RIGHT") {
                    continue;
                }
            } else {
                let lab = lab.replace(&format!(" {axle}"), &format!(" {wheel}"));
                changes.push(SetupChange { group, label: lab, key: key.clone(), from: x.display.clone(), to: y.display.clone(), from_index: x.index, to_index: y.index, steps: y.index - x.index, meaning: meaning(section, key, x, y) });
                continue;
            }
        }
        changes.push(SetupChange { group, label: lab, key: key.clone(), from: x.display.clone(), to: y.display.clone(), from_index: x.index, to_index: y.index, steps: y.index - x.index, meaning: meaning(section, key, x, y) });
    }
    SetupComparison { a: a.name.clone(), b: b.name.clone(), same_car: a.vehicle_class == b.vehicle_class, vehicle_a: a.vehicle_class.clone(), vehicle_b: b.vehicle_class.clone(), changes, unchanged }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fichier synthétique au format du jeu (valeurs inventées pour le test, aucun setup réel).
    const A: &str = "VehicleClassSetting=\"Hypercar Test_Car WEC2026\"\nUpgradeSetting=(0,0,0,0)\n//Note: settings commented out if using the default\n\n[GENERAL]\nNotes=\"\"\nSymmetric=1\nCGHeightSetting=0//Non-adjustable\nFuelSetting=77//0.82\n\n[REARWING]\nRWSetting=18//P19\n\n[SUSPENSION]\nFrontAntiSwaySetting=2//P2\nRearToeInSetting=18//0.12 deg\n\n[CONTROLS]\nRearBrakeSetting=13//54.1:45.9\n\n[FRONTLEFT]\nCamberSetting=26//-1.4 deg\nPressureSetting=0//136 kPa\n\n[FRONTRIGHT]\nCamberSetting=26//-1.4 deg\nPressureSetting=0//136 kPa\n\n[REARLEFT]\nRideHeightSetting=20//8.0 cm\n\n[REARRIGHT]\nRideHeightSetting=20//8.0 cm\n";

    fn file(raw: &str, name: &str) -> SetupFile {
        let (vehicle_class, notes, settings) = parse_svm(raw);
        SetupFile { path: PathBuf::from(format!("{name}.svm")), name: name.into(), track: "Imola".into(), vehicle_class, notes, settings }
    }

    #[test]
    fn reads_index_and_displayed_value() {
        let f = file(A, "a");
        assert_eq!(f.vehicle_class, "Hypercar Test_Car WEC2026");
        let cam = f.settings.iter().find(|s| s.section == "FRONTLEFT" && s.key == "CamberSetting").unwrap();
        assert_eq!((cam.index, cam.display.as_str()), (26, "-1.4 deg"));
        assert!(f.settings.iter().find(|s| s.key == "CGHeightSetting").unwrap().fixed());
        assert_eq!(number("-1.4 deg"), Some(-1.4));
        assert_eq!(number("P19"), Some(19.0));
        assert_eq!(number("54.1:45.9"), Some(54.1));
        // Fichier Latin-1 (accents d'anciennes versions).
        assert_eq!(text(b"D\xe9tach\xe9"), "Détaché");
    }

    #[test]
    fn compares_two_setups_with_axle_merge_and_meaning() {
        let b_raw = A.replace("RWSetting=18//P19", "RWSetting=19//P20").replace("CamberSetting=26//-1.4 deg", "CamberSetting=28//-1.2 deg").replace("RideHeightSetting=20//8.0 cm\n\n[REARRIGHT]", "RideHeightSetting=25//8.5 cm\n\n[REARRIGHT]");
        let c = compare(&file(A, "a"), &file(&b_raw, "b"));
        assert!(c.same_car);
        let labels: Vec<_> = c.changes.iter().map(|x| x.label.as_str()).collect();
        // Carrossage changé à gauche et à droite : une ligne « AV » ; hauteur changée à gauche seulement.
        assert_eq!(labels, ["Aileron arrière", "Carrossage AV", "Hauteur de caisse ArG"]);
        let rw = &c.changes[0];
        assert_eq!((rw.from.as_str(), rw.to.as_str(), rw.steps), ("P19", "P20", 1));
        assert!(rw.meaning.as_deref().unwrap().starts_with("Plus d'appui à l'arrière"));
        // -1.4 → -1.2 : moins négatif.
        assert!(c.changes[1].meaning.as_deref().unwrap().starts_with("Carrossage moins négatif"));
        assert_eq!(c.changes[1].steps, 2);
        // Réglables identiques : carburant, barre AV, pincement AR, répartition, pressions AV (2), hauteur ArD.
        assert_eq!(c.unchanged, 7);
    }

    #[test]
    fn steam_library_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"F:\\\\Programmes\\\\Steam\"\n\t}\n}\n";
        assert_eq!(vdf_paths(vdf), [PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"F:\Programmes\Steam")]);
    }

    #[test]
    fn library_lists_svm_by_track() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Imola")).unwrap();
        std::fs::write(d.path().join("Imola").join("Base.svm"), A).unwrap();
        std::fs::write(d.path().join("Imola").join("Sauvegarde auto.rrbin"), b"x").unwrap();
        let l = library(d.path());
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].track.as_str(), l[0].name.as_str()), ("Imola", "Base"));
    }
}
