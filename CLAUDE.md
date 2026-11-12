# CLAUDE.md — Apex Companion

Instructions pour les sessions de développement. Ce que fait l'app : `README.md`. Architecture, limites par simulateur, schéma de données : `docs/ARCHITECTURE.md`. Ne pas dupliquer ces deux fichiers ici.

## Plan des dossiers

| Dossier | Rôle |
|---|---|
| `crates/apex-core` | Toute la logique, testable sans WebView : ingestion (`ingest/` : .ibt, .apexlmu, Garage 61), connecteurs live (`connectors/`), analyse (`analysis/`), coach en direct (`live_coach.rs`), ingénieur de course (`race_engineer.rs`), setups LMU `.svm` (`setup.rs`), sonde setup LMU (`connectors/lmu_probe.rs`), stockage SQLite (`storage.rs`), commandes UI (`service.rs`). Outils de vérification dans `examples/` (ex. `strategy_check`, `dump_all` sur une copie de base) |
| `src-tauri` | Coque Tauri 2 : commandes IPC (`commands.rs`), fil de surveillance et radio (`monitor.rs`), bouton radio (`ptt_loop.rs`), mise à jour signée (`updater.rs`) |
| `src` | UI React/TS. `lib/types.ts` = contrat IPC (miroir des structs Rust sérialisées) ; `src/mobile/` = app iPhone |
| `mobile` | Point d'entrée de l'app mobile (page installable, GitHub Pages), construite par `vite.mobile.config.ts` à partir de `src/mobile/` |
| `simhub` | Plugin SimHub (C# 5, compilé sur le PC de l'utilisateur par Apex) : LEDs de freinage via UDP local |
| `.github/workflows` | `build-windows.yml` (manuel : tests, installeur, signature, release + `latest.json`) ; `mobile-pages.yml` (à chaque push sur `main` touchant `src/`, `mobile/`… : publie `gh-pages`) |

## Commandes (vérifiées sous Linux dans le conteneur de dev ; sous Windows, mêmes commandes)

```bash
npm ci                                                   # dépendances front
npm run typecheck && npm test                            # TypeScript strict + Vitest
npm run build                                            # front desktop (dist/)
npm run build:mobile                                     # app mobile (dist-mobile/)
cargo test -p apex-core                                  # cœur Rust (sans Tauri)
cargo test -p apex-companion                             # coque Tauri
cargo clippy -p apex-core --all-targets -- -D warnings   # obligatoire avant commit
cargo clippy -p apex-companion --all-targets -- -D warnings
npm run dev                                              # Vite seul, http://localhost:1420
npm run dev:mobile                                       # app mobile, http://localhost:1421
npm run tauri dev                                        # app complète (Linux sans écran : xvfb-run -a npm run tauri dev)
cargo run -p apex-core --example demo_report             # rapport texte sur données de démo
cargo run -p apex-core --example strategy_check -- copie/apex.db   # stratégie sur une COPIE de vraie base
APEX_CORPUS_DIR=corpus cargo test -p apex-core --test real_corpus -- --nocapture   # séances réelles
APEX_UPDATE_SNAPSHOTS=1 APEX_CORPUS_DIR=corpus cargo test -p apex-core --test real_corpus   # changement voulu
cargo run --release -p apex-core --example corpus_prepare -- sortie/ fichier.ibt   # anonymise + compresse
cargo run -p apex-core --example setup_compare -- a.svm b.svm               # comparateur de setups LMU
```

Corpus de séances réelles : dépôt privé `lwygamez-wq/apex-corpus` (fichiers `.ibt.gz` / `.apexlmu.gz` passés par `corpus_prepare` : noms et identifiants de pilotes remplacés, tours vérifiés identiques). La CI le récupère avec le secret `APEX_CORPUS_TOKEN` (lecture seule). Les instantanés (`crates/apex-core/tests/corpus/*.json`, chiffres seulement, sans iRating ni nom) sont versionnés ici : tout écart fait échouer la CI. Un écart voulu se met à jour avec `APEX_UPDATE_SNAPSHOTS=1` et s'explique dans la PR. Le test vérifie aussi qu'aucun conseil n'est donné sans chiffre mesuré.

L'installeur Windows se construit par le workflow `build-windows.yml`, pas en local.

## Conventions

- Code, commentaires, messages et UI en français ; textes destinés au pilote courts et prononçables (le coach les lit à voix haute).
- Ne jamais lancer `cargo fmt` (lignes longues assumées, diffs illisibles sinon).
- Commits : identité `Alex <lwygamez-wq@users.noreply.github.com>`, jamais l'email personnel. Message `vX.Y.Z : titre` + liste à puces de ce qui change.
- Tout nouveau calcul de coaching ou de stratégie vit dans `apex-core` avec un test qui fixe les chiffres attendus.
- Nouveau champ dans `LapMeta` / un struct persisté : `#[serde(default)]`, migration SQLite ajoutée en fin de `MIGRATIONS` (jamais modifier une migration existante), mettre à jour tous les constructeurs (`mock`, `race`, `session`, `service`, `tests/analysis_e2e.rs`).
- Canaux live : lus par nom dans `LIVE_CHANNELS` (absent = 0), garder `LIVE_N` et l'ordre de `LiveState::row` (LMU) alignés.
- Fichiers `.apexlmu` : le décodage doit accepter les anciens fichiers sans les extensions récentes.

## Pièges déjà rencontrés

- `AppService::db()` prend un `Mutex` non réentrant : ne jamais appeler une méthode qui reprend `self.db()` pendant qu'un garde est tenu (blocage de l'écran, v0.32.0).
- React dans WebView2 : un `useEffect(() => expr)` qui renvoie une promesse a donné une page noire ; écrire les effets en bloc `{ … }`. Pages enveloppées dans `PageBoundary`.
- Arrondi des niveaux à régler : `(v - 0.05).ceil()` (sinon 100,000 01 % devient 101 %).
- Calculateur et ingénieur de course partagent `analysis::strategy` (`target`, `pit_fill`) ; le test `live_calls_match_the_calculator` garantit les mêmes chiffres : ne pas le contourner.
- Modèles IA (Gemini surtout) renommés ou retirés sans préavis : passer par `ai::list_models` / `pick_model`, jamais un nom figé seul.
- Vibreurs : distance point-segment au bord (points relevés jusqu'à 7–9 m du bord).
- LMU vérifié en jeu (essais Imola 9X8, 12 nov.) : `mVirtualEnergy` en % (≈ 3 %/tour), `mWear` = % restant (100 = neuf). Encore à vérifier : remise à zéro des limites de piste (restée à 0 alors que le jeu a invalidé 7 tours sur 9), détection d'arrêt (énergie +2 %).
- Course au temps : le tour « multiclasse » en plus ne vaut que hors catégorie la plus rapide (`top_class`, Hypercar en LMU) ; le temps d'arrêt mesuré (`pit_loss_s`, à défaut celui des autres voitures du circuit) est décompté du chrono. En direct, la catégorie vient de la séance (`is_top_class`), pas seulement du calculateur (nouvelle voiture sans historique). Sans ça, 96 % au lieu de ~87 % à Imola (14 % d'énergie restante). En LMU, l'énergie ne donne pas de puissance : l'ingénieur ne dit jamais « attaque ».
- Ce que l'ingénieur a dit : journal d'Apex (`%LOCALAPPDATA%\app.apexcompanion.desktop\logs`, lignes `coach :`).
- Tours invalidés par le jeu (limites de piste) : `clean` est faux, mais la consommation reste valable. La stratégie filtre sur les drapeaux (stand, sortie, incident, jaune, coupure), pas sur `clean`.

## Sécurité et données

- Local-first : aucune télémétrie ne quitte le PC sans action explicite ; le débrief IA n'envoie que des chiffres calculés, visibles avant envoi.
- Clé de signature des mises à jour : uniquement dans le secret GitHub `TAURI_SIGNING_PRIVATE_KEY`, jamais dans le dépôt.
- Jeton Garage 61, clés Azure et IA : jamais dans les journaux ni les exports (`dump_all` les masque). Ne pas contourner les conditions de Garage 61.
- Setups LMU vendus (GOFast, Coach Dave…) : lus sur le PC d'Alex seulement, jamais copiés dans le dépôt, le corpus ni les tests (fixtures synthétiques). Ne pas ouvrir les `.VEH` / `.mas` du jeu.
- API locale de LMU (`127.0.0.1:6397`) : GET seulement ; les écritures font planter le jeu.
- Ne pas versionner les DLL SimHub. `preview.html`, `src/preview.tsx`, `src/preview-data.json` (aperçu local avec une vraie base) sont exclus via `.git/info/exclude` : ne jamais les ajouter.

## Règles produit (Alex)

- Direction artistique inspirée de la F1, sans logo, nom ni police propriétaires (F1, équipes, partenaires).
- Pas de guide de premier lancement, pas de lexique.
- Pas de look « IA » (dégradés violets, emojis décoratifs, cartes génériques) : thème « Paddock » actuel.
- Local-first (voir ci-dessus).
- Tout conseil de coaching est justifié par des chiffres de télémétrie (mètres, km/h, dixièmes, %). Alex est débutant et ne peut pas juger un conseil : sans écart mesurable, le coach le dit au lieu d'inventer.
- Simulateur principal actuel : Le Mans Ultimate (Hypercar, LMGT3) ; iRacing reste pris en charge.

## Version et feuille de route

- Version publiée : **v0.34.2** (12 nov. 2026) : catégorie Hypercar lue dans la séance en direct, temps d'arrêt pris sur les autres voitures du circuit faute d'historique. Avant : v0.34.1, plein à l'arrêt corrigé en course au temps.
- Suivant, par priorité : trafic multiclasse (tours gênés exclus des références) ; temps perdu aux stands mesuré ; lift and coast localisé ; séparation piste humide / nuit ; températures et pressions pneus ; hybride ; coach de course.
- Setup Builder : décision go / analyseur seul / abandon après le premier rapport de la sonde (`sonde-lmu/` dans le dossier de données) ; étude dans le projet claude.ai (`claude/faisabilite-setup-builder.md`).
- À valider sur le PC d'Alex : lectures LMU (limites de piste), annonces de l'ingénieur en course, débrief Gemini gratuit, plugin SimHub.
