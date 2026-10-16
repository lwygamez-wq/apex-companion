# Apex Companion — Architecture technique

> Ce document couvre les points 1 à 8 du cahier des charges : synthèse, choix techniques, limites par simulateur, architecture, arborescence, schéma de données, interfaces des connecteurs et plan de développement.
> Statut : **MVP livré (phase 1)**. Toute intégration non validée sur une vraie installation est marquée comme telle, dans ce document comme dans l'application.

---

## 1. Synthèse

Apex Companion est une application de bureau **locale** qui importe la télémétrie d'un simulateur, l'aligne **par distance**, la compare à une référence et en tire des conseils de pilotage **chiffrés, localisés et accompagnés d'un niveau de confiance**.

**Parti pris produit.** L'essentiel de la valeur se trouve dans le moteur d'analyse (section 6 du cahier des charges), pas dans la détection de processus ni dans l'overlay. Les outils existants (Garage 61, Track Titan, Coach Dave Delta, VRS, SimHub) couvrent déjà l'affichage de courbes. Ce qui nous différencie :

- des conseils **virage par virage**, reliés à une métrique mesurée ;
- des **défauts récurrents** repérés sur plusieurs tours ;
- une **honnêteté explicite** : niveau de confiance, données manquantes, comparaisons invalides signalées ;
- un fonctionnement **100 % local** et **multi-simulateur**.

**Ce qui fonctionne aujourd'hui**, testé et démontrable :

| Fonction | État |
|---|---|
| Import iRacing `.ibt` (format irsdk public) | ✅ testé sur fichiers conformes à la spec, **à valider sur tes .ibt réels** |
| Découpage en tours, franchissement de ligne interpolé, tours de sortie/lancés/retour | ✅ |
| Validité **estimée** (hors-piste, incidents, drapeaux, stands, discontinuités) | ✅ (affichée « estimée ») |
| Stockage SQLite (WAL, transactions, anti-doublon) | ✅ |
| Delta par distance, superposition de 2 tours, carte colorée par gain/perte | ✅ |
| Détection automatique des virages, métriques par virage | ✅ |
| Conseils : freinage tôt/tard, vitesse mini, remise des gaz tardive/brutale, sortie | ✅ |
| Tour théorique par segments, régularité, zones de progrès récurrentes | ✅ |
| Référence personnelle unique par combo sim/circuit/config/voiture | ✅ |
| Détection des processus (iRacing, LMU, AC, AC EVO, ACC) | ✅ |
| Lecture live iRacing (mémoire partagée) + import auto en fin de session | ⚠️ code écrit, **non validé** (Windows uniquement) |
| Connecteurs LMU / AC / AC EVO | 🔜 déclarés « Prévu », aucune donnée simulée |
| Live Coach, overlay | 🔜 phase 2 (écran de statut uniquement) |

---

## 2. Choix technologiques

| Couche | Choix | Justification | Alternative écartée |
|---|---|---|---|
| Shell desktop | **Tauri 2** | Binaire de ~10 Mo, RAM minimale, WebView2 déjà installé sous Windows 11. Aucun impact sur les FPS du simulateur. | Electron : ~150 Mo de RAM et un Chromium de plus, sans gain d'accès aux SDK (la mémoire partagée se lit mieux en natif). |
| Accès télémétrie | **Rust** (`windows-sys`) | Mémoire partagée Win32, parsing binaire et zéro allocation dans la boucle chaude. | Node (ffi) : fragile. |
| Analyse | **Rust**, dans le même crate | Rééchantillonnage, delta et segmentation relèvent d'algorithmes simples et déterministes. Un runtime Python embarqué ajouterait de l'IPC et 50 Mo pour aucun gain. | Python/NumPy : pertinent pour du ML plus tard, pas pour ce périmètre. |
| Base locale | **SQLite** (`rusqlite` bundled) | Métadonnées indexées ; traces en **BLOB colonnaire LZ4**. | Une ligne par échantillon : 5 000 lignes × 12 colonnes par tour, comparaisons 100× plus lentes. |
| UI | **React + TypeScript strict** | Standard, typage du contrat IPC dans `src/lib/types.ts`. | — |
| Graphiques | **uPlot** | La librairie la plus rapide sur de longues séries synchronisées (~40 Ko). Curseur partagé natif. | Recharts/Chart.js : trop lents au-delà de 10 000 points. |
| Tests | `cargo test` (cœur), Vitest (formatage) | Toute la logique métier est testable sans WebView. | — |
| Logs | `tauri-plugin-log`, fichiers **locaux** avec rotation | Aucun crash reporting distant : un diagnostic exportable **sans données personnelles** le remplace. | Sentry : contraire au principe local-first. |
| Mises à jour | Module maison (`src-tauri/src/updater.rs`) : `latest.json` sur les releases GitHub, installeur vérifié par **signature minisign** avant lancement | Même format que `tauri-plugin-updater`, indisponible en oct. 2026 (dépendance `zip` retirée de crates.io). Seule requête réseau de l'app, désactivable. | Mise à jour manuelle : pénible, donc versions anciennes jamais remplacées. |

**Architecture volontairement non retenue : un overlay natif au-dessus du jeu.**
En plein écran exclusif et en triple écran NVIDIA Surround, un overlay transparent est fragile : il perd le focus, clignote et casse la topologie. Le Live Coach (phase 2) sera donc servi en **WebSocket local**, affichable sur un second écran, une tablette ou un téléphone. Un overlay en fenêtre sans bordure restera une option pour les joueurs en mode fenêtré.

---

## 3. Limites connues par simulateur

> Vérifié en septembre 2026. Les APIs des jeux évoluent : chaque connecteur déclare ses capacités (`Implemented` / `Unverified` / `Planned` / `Unavailable`), et l'UI les affiche telles quelles.

### iRacing
- **Import `.ibt`** : format public (`irsdk_defines.h`). L'enregistrement doit être activé en jeu (**Alt+L**) ; sinon, aucun fichier n'est produit.
- **Position X/Y** : dérivée de `Lat`/`Lon`, présents **uniquement dans les `.ibt`**, pas en live.
- **Validité du tour** : **non exposée** par iRacing. Elle est déduite (`PlayerTrackSurface == OffTrack`, hausse du compteur d'incidents) et affichée « estimée ».
- **Pneus** : températures et usure ne sont mises à jour **qu'aux stands**, donc inexploitables pour l'analyse de dégradation en roulant.
- **Live** : mémoire partagée `Local\IRSDKMemMapFileName`. Même format d'en-tête que les `.ibt`, ce qui permet de réutiliser le parseur. Code écrit, **non testé sur une vraie session**.
- **Tours de référence d'autres pilotes** : iRacing ne les fournit pas. Il faut les importer (`.ibt` d'un coéquipier, avec son consentement).

### Le Mans Ultimate
- Mémoire partagée native **`LMU_Data`**, publiée lorsque *Réglages → Gameplay → Enable Plugins* est activé (redémarrage du jeu requis). Ancienne voie : le plugin rF2 Shared Memory Map.
- API REST locale sur le port 6397 : météo prévue, classement, dégâts.
- **Disposition mémoire non documentée officiellement** par Studio 397 : implémenter des offsets devinés serait contraire au cahier des charges. **Piste recommandée** : évaluer la crate Rust `kerb`, qui l'implémente déjà (iRacing, LMU, AC EVO), plutôt que de réécrire le lecteur.
- Pas d'import après session implémenté (exports MoTeC `.ld` possibles, format à évaluer).

### Assetto Corsa (AC1)
- Mémoire partagée **documentée par Kunos** (`acpmf_physics`, `acpmf_graphics`, `acpmf_static`) : la source la plus solide des trois. Prévue en phase 3.
- **Aucun fichier de télémétrie natif** : l'analyse passera donc par l'enregistrement live.

### Assetto Corsa EVO
- Mémoire partagée native annoncée et utilisée par des outils tiers, **mais le jeu est en accès anticipé** : le format peut changer à chaque mise à jour.
- Le nom du processus (`AssettoCorsaEVO.exe`) est **à confirmer** sur ta machine.

### ACC
- Même famille de mémoire partagée, plus l'API broadcasting UDP. Non prioritaire.

---

## 4. Architecture

```
┌──────────────────────────── Processus Apex Companion ────────────────────────────┐
│                                                                                   │
│  WebView (React/TS)                     Rust (src-tauri)                          │
│  ┌──────────────────┐   invoke IPC    ┌────────────────────┐                      │
│  │ pages/*          │ ──────────────► │ commands.rs        │  spawn_blocking      │
│  │ components/*     │ ◄────────────── │ (enveloppes fines) │──────────┐           │
│  │ lib/api.ts       │  événements     └────────────────────┘          ▼           │
│  └──────────────────┘  apex://status  ┌────────────────────┐   ┌──────────────┐   │
│                          ◄─────────── │ monitor.rs (thread)│──►│  apex-core   │   │
│                                       │ 2 s : processus,   │   │  service.rs  │   │
│                                       │ live, import auto  │   └──────┬───────┘   │
│                                       └────────────────────┘          │           │
│   apex-core (Rust pur, sans UI — 100 % testable)                      │           │
│   ┌──────────┐ ┌─────────────┐ ┌──────────┐ ┌──────────┐ ┌─────────┐  │           │
│   │connectors│ │ingest (.ibt)│ │ analysis │ │ storage  │ │ detect  │◄─┘           │
│   │ trait +  │→│ parseur     │→│ resample │→│ SQLite   │ │ machine │              │
│   │capacités │ │ irsdk       │ │ corners  │ │ WAL+LZ4  │ │ à états │              │
│   └──────────┘ └─────────────┘ │ insights │ └──────────┘ └─────────┘              │
│                                │ progress │                                       │
│                                └──────────┘                                       │
└───────────────────────────────────────────────────────────────────────────────────┘
        ▲ mémoire partagée (lecture seule)        ▲ fichiers .ibt (lecture seule)
     Simulateurs                              Documents\iRacing\telemetry
```

**Règles de séparation**
- `apex-core` ne dépend ni de Tauri ni de l'UI. Toute la logique des commandes vit dans `service.rs`, ce qui permet de la tester en mémoire.
- `analysis/` est **pur** : pas d'entrées/sorties, entrées et sorties sérialisables.
- `src-tauri` ne contient que le câblage : commandes, thread de surveillance, plugins.
- L'UI n'appelle jamais `invoke` directement : elle passe par `lib/api.ts`, et le contrat de types se trouve dans `lib/types.ts`.

**Pipeline d'analyse**
1. **Rééchantillonnage** sur une grille de distance fixe (1 m par défaut), avec interpolation linéaire. Les retours arrière de distance sont écartés et la couverture est mesurée (un trou de plus de 15 m compte comme non couvert).
2. **Delta** : `t_tour(d) − t_réf(d)` en chaque point de la grille.
3. **Virages** : minima de vitesse lissée, filtrés par proéminence (≥ 10 km/h). La fenêtre d'un virage va de *levé − 30 m* à *pleine charge + 120 m*.
4. **Métriques par virage** : début de freinage, pic de freinage, vitesse minimale et sa position, remise des gaz, vitesse de sortie, durée de montée des gaz, inversions de volant (hystérésis de 3°).
5. **Conseil** : la cause principale est retenue selon une règle ordonnée, puis la confiance est calculée à partir de la qualité des données (tours propres, couverture, densité d'échantillonnage, conditions, récurrence).
6. **Zones de progrès** : chaque tour propre est comparé au meilleur, puis les pertes sont regroupées par virage et triées par *récurrence × perte moyenne*.

---

## 5. Arborescence

```
apex-companion/
├── Cargo.toml                  # workspace (default-members = apex-core : tests sans WebView)
├── package.json  tsconfig.json  vite.config.ts  index.html
├── crates/apex-core/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── model.rs            # modèle unifié (SimId, SessionMeta, LapMeta, LapTrace…)
│   │   ├── codec.rs            # trace colonnaire binaire + LZ4
│   │   ├── storage.rs          # SQLite : migrations, anti-doublon, références
│   │   ├── service.rs          # logique des commandes UI (testable)
│   │   ├── detect.rs           # scan processus + machine à états de session
│   │   ├── mock.rs             # circuit et sessions de démo (défauts injectés connus)
│   │   ├── ingest/ibt.rs       # parseur iRacing .ibt (+ écrivain de test)
│   │   ├── connectors/         # mod.rs (traits), iracing.rs, iracing_live.rs, lmu.rs, ac.rs
│   │   └── analysis/           # resample, corners, insights, compare, session, progress
│   ├── tests/                  # analysis_e2e.rs, ibt_real.rs (tes fichiers réels)
│   └── examples/               # demo_report.rs (rapport texte), dump_fixtures.rs
├── src-tauri/
│   ├── tauri.conf.json  capabilities/default.json  icons/
│   └── src/                    # main.rs, lib.rs, commands.rs, monitor.rs
├── src/                        # UI React
│   ├── App.tsx  main.tsx  styles.css
│   ├── lib/                    # types.ts (contrat IPC), api.ts, hooks.ts, format.ts (+ tests)
│   ├── components/             # TelemetryCharts (uPlot), TrackMap, InsightCard, …
│   └── pages/                  # Dashboard, Sessions, LapAnalysis, Comparison, LiveCoach, Settings
└── docs/ARCHITECTURE.md
```

---

## 6. Schéma de données

### SQLite (migration v1)

```sql
sessions   (id PK, sim, track, track_config, track_length_m, car, kind, started_at,
            weather_json, source_fingerprint UNIQUE, source_path, sample_rate_hz, imported_at)
            INDEX (sim, track, car, started_at)
laps       (id PK, session_id FK→sessions ON DELETE CASCADE, number, kind, lap_time_s,
            validity, validity_source, flags_json, sectors_json, fuel_start_l, fuel_used_l,
            is_reference)          INDEX (session_id)
lap_traces (lap_id PK FK→laps ON DELETE CASCADE, codec, n_samples, data BLOB)
settings   (key PK, value)
```

- **Anti-doublon** : la contrainte `source_fingerprint UNIQUE` est une empreinte FNV du YAML de session, de la date de départ et du nombre d'enregistrements. Réimporter un même `.ibt` renvoie `Duplicate`.
- **Robustesse** : mode WAL, `synchronous=NORMAL`, clés étrangères actives, **un import = une transaction**. Un crash en cours d'import ne laisse aucune session partielle.
- **Trace** : canaux obligatoires `time_s, dist_m, speed_mps, throttle, brake`. Canaux optionnels : `gear, rpm, steer_rad, pos_x, pos_y, fuel_l, yaw_rate`, plus `extra` (températures et usure pneus, etc.). **Un canal absent reste absent** : il n'est jamais rempli de zéros.

### Correspondance avec le modèle demandé

| Entité demandée | Implémentation |
|---|---|
| Simulator, Car, Track, TrackLayout | champs de `SessionMeta` (`sim`, `car`, `track`, `track_config`, `track_length_m`) |
| Session | `sessions` + `SessionMeta` |
| Lap, Sector | `laps` + `LapMeta` (`sector_times_s`) |
| TelemetrySample | `LapTrace` colonnaire (une colonne par canal) |
| ReferenceLap | `laps.is_reference`, unique par combo |
| Incident | `LapFlags` (hors-piste, incident, stands, jaune, discontinuité) |
| CoachingInsight | `Insight`, calculé à la demande et non persisté (il se recalcule quand les seuils évoluent) |
| DriverProfile, Stint, SetupSnapshot | phase 3 (le relais est déductible des tours de sortie et de retour) |

---

## 7. Interfaces des connecteurs

```rust
pub trait SimulatorConnector: Send + Sync {
    fn id(&self) -> SimId;
    fn display_name(&self) -> &'static str;
    fn process_names(&self) -> &'static [&'static str];
    fn capabilities(&self) -> Capabilities;           // honnêteté : Implemented / Unverified / Planned / Unavailable
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus;
    fn importer(&self) -> Option<&dyn SessionImporter> { None }
    fn open_live(&self, cfg: &ConnectorConfig) -> Result<Box<dyn LiveSource>, ConnectorError>;
}

pub trait SessionImporter: Send + Sync {
    fn extensions(&self) -> &'static [&'static str];
    fn default_dir(&self) -> Option<PathBuf>;
    fn import_file(&self, path: &Path) -> Result<ImportedSession, ConnectorError>;
    fn scan(&self, dir: &Path) -> Result<Vec<PathBuf>, ConnectorError>;
}

pub trait LiveSource: Send {
    fn poll(&mut self, timeout_ms: u32) -> Result<Option<LiveSnapshot>, ConnectorError>;
}
```

**Écarts avec l'interface TypeScript proposée**, et leurs raisons :
- **Traits Rust plutôt qu'une interface TS** : l'accès à la télémétrie se fait en natif. L'UI n'en voit que les capacités, sérialisées.
- **Import et live découpés en deux traits optionnels** : un connecteur qui ne sait qu'importer (ou que lire en live) n'a pas à implémenter de méthodes vides.
- **`detectRunningProcess` sorti du connecteur** : le connecteur déclare `process_names()`, et un seul `ProcessScanner` mutualise le scan de tous les connecteurs (un seul appel système toutes les 2 s).
- **`poll` avec délai** plutôt que `start`/`stop` : c'est le thread de surveillance qui possède la boucle, ce qui facilite l'arrêt propre et la reconnexion.

**Ajouter un simulateur** : créer `connectors/<sim>.rs`, implémenter le trait, puis l'ajouter à `registry()`. L'UI (Réglages, statut) le prend en compte sans autre modification.

---

## 8. Plan de développement

| Phase | Contenu | Validation |
|---|---|---|
| **1 — MVP (livré)** | Import `.ibt`, stockage, analyse 2 tours, conseils, tableau de bord, sessions, réglages, données de démo, détection de processus | 41 tests Rust et 4 tests Vitest ; l'appli démarre (vérifiée sous Linux/WebKitGTK) |
| **1b — Validation réelle** | Lancer `tests/ibt_real.rs` sur tes `.ibt` ; calibrer les seuils (`Thresholds`, `CornerConfig`) sur tes données F4/F3/SF | À faire sur ta machine |
| **2 — Live** | Valider la lecture live iRacing ; enregistreur live → `ImportedSession` (pour les sims sans fichier) ; delta live ; Live Coach en WebSocket local ; règles anti-distraction par type de session | Session réelle iRacing |
| **3 — Multi-sim** | Connecteur LMU (évaluer `kerb`), AC1 (acpmf documenté), AC EVO ; table de virages manuelle par circuit ; relais, pneus, carburant | Une session réelle par sim |
| **4 — Avancé** | Tour moyen, comparaison inter-voitures, référence importée (coéquipier), exports CSV/MoTeC, installeur signé, mises à jour signées (plugin updater) | — |
| **5 — Optionnel** | Partage opt-in (désactivé par défaut) | — |

### Risques techniques

| Risque | Impact | Mitigation |
|---|---|---|
| Variables `.ibt` absentes ou renommées selon la voiture | Canaux manquants | Tous les canaux sont optionnels sauf 6 ; l'erreur nomme le canal manquant |
| Virages mal segmentés (chicanes, courbes rapides sans lever de pied) | Conseils mal localisés | Seuils ajustables, puis table de virages manuelle par circuit (phase 3) |
| Faux positifs de conseils | Perte de confiance du pilote | Seuils au-dessus du bruit, confiance dégradée si données imparfaites, « cause non isolée » plutôt qu'invention |
| Format LMU / AC EVO modifié par une mise à jour | Connecteur cassé | Isolé derrière le trait, version du jeu à vérifier au démarrage, statut `Unverified` |
| Comparaison de conditions incompatibles | Conseils trompeurs | Avertissements bloquants (circuit/sim différents → aucun conseil) ou dégradant la confiance (voiture, météo) |
| Charge CPU pendant la session | FPS du sim | Scan toutes les 2 s, lecture live à 1 Hz pour le statut ; l'enregistrement à 60 Hz reste à mesurer en phase 2 |
