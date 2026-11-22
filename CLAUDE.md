# CLAUDE.md — Apex Companion

Instructions pour les sessions de développement. Ce que fait l'app : `README.md`. Architecture, limites par simulateur, schéma de données : `docs/ARCHITECTURE.md`. Ne pas dupliquer ces deux fichiers ici.

## Plan des dossiers

| Dossier | Rôle |
|---|---|
| `crates/apex-core` | Toute la logique, testable sans WebView : ingestion (`ingest/` : .ibt, .apexlmu, Garage 61), connecteurs live (`connectors/`), analyse (`analysis/`, dont le débrief de course `analysis/debrief.rs`), coach en direct (`live_coach.rs`), ingénieur de course (`race_engineer.rs`), setups LMU `.svm` (`setup.rs`), sonde setup LMU (`connectors/lmu_probe.rs`), stockage SQLite (`storage.rs`), commandes UI (`service.rs`). Outils de vérification dans `examples/` (ex. `strategy_check`, `dump_all` sur une copie de base) |
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
cargo run -p apex-core --example race_review -- course.apexlmu "Apex Companion.log"   # bilan de course LMU + annonces de l'ingénieur (VERDICT OK / ÉCART)
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
- Fichiers `.apexlmu` : le décodage doit accepter les anciens fichiers sans les extensions récentes. Extensions : `EXT1` (0.31 : énergie, usure, limites de piste), `EXT2` (0.36 : hauteur par roue, pneus et freins à 5 Hz), `EXT3` (0.41 : accélérations, rotation et vitesse sol de chaque roue à 60 Hz, couche interne des pneus à 5 Hz), `EXT4` (0.45 : voiture de référence de la catégorie, un relevé quand sa position change) ; setup de la séance dans l'en-tête JSON (`setup`). Températures de surface dans l'ordre du jeu : gauche / milieu / droite (gauche = extérieur pour les roues gauches).

## Méthode

Chaque changement, même petit :

1. Branche depuis `main` (`main` est protégée : pas de push direct).
2. Logique dans `apex-core` : écrire d'abord le test qui fixe les chiffres attendus.
3. Coder, puis toutes les vérifications de la section Commandes (TypeScript, Vitest, clippy, tests Rust).
4. Séances réelles : `real_corpus` doit rester « identique ». Un écart voulu se met à jour exprès et s'explique écart par écart dans la PR.
5. Interface modifiée : captures dans l'aperçu local et contrôle visuel.
6. PR avec le modèle (`.github/pull_request_template.md`) : résumé, tests, séances réelles, captures, limites. La CI (`linux`, `windows`) doit être verte.

## Autonomie de Claude

Consigne d'Alex (17 nov. 2026) : Claude agit en dirigeant exécutif de l'app, Alex en investisseur. Claude décide, développe, fusionne et publie sans attendre de validation dès que toutes les vérifications et la CI sont vertes, puis rend compte.

- **Décide, fusionne et publie seul** : corrections, nouvelles fonctions, changements visibles, versions (procédure de la skill `apex-release`, PR fusionnée par Claude une fois la CI verte).
- **Demande avant d'agir** : dépense ou service payant, clés, comptes, secrets ou permissions, migration qui supprime des données, changement de direction visuelle.
- Ne jamais désactiver une vérification, un test ou une règle de `main` pour faire passer une PR. Ne jamais présenter comme vérifié en jeu ce qui ne l'a été que sur données synthétiques ou anciennes courses.

## Pièges déjà rencontrés

- `AppService::db()` prend un `Mutex` non réentrant : ne jamais appeler une méthode qui reprend `self.db()` pendant qu'un garde est tenu (blocage de l'écran, v0.32.0).
- React dans WebView2 : un `useEffect(() => expr)` qui renvoie une promesse a donné une page noire ; écrire les effets en bloc `{ … }`. Pages enveloppées dans `PageBoundary`.
- Arrondi des niveaux à régler : `(v - 0.05).ceil()` (sinon 100,000 01 % devient 101 %).
- Calculateur et ingénieur de course partagent `analysis::strategy` (`target`, `pit_fill`) ; le test `live_calls_match_the_calculator` garantit les mêmes chiffres : ne pas le contourner.
- Modèles IA (Gemini surtout) renommés ou retirés sans préavis : passer par `ai::list_models` / `pick_model`, jamais un nom figé seul.
- Vibreurs : distance point-segment au bord (points relevés jusqu'à 7–9 m du bord).
- LMU vérifié en jeu (essais Imola 9X8, 12 nov.) : `mVirtualEnergy` en % (≈ 3 %/tour), `mWear` = % restant (100 = neuf). Limites de piste (vérifié sur le corpus, 0.44) : en course `mTrackLimitsSteps` avance et chaque avertissement tombe dans un tour non chronométré (`lap_time −1`, marqué invalide) ; en essais le jeu invalide les tours (`mLapInvalidated`) sans faire avancer le compteur. Encore à vérifier : détection d'arrêt (énergie +2 %).
- Course au temps : le tour « multiclasse » en plus ne vaut que hors catégorie la plus rapide (`top_class`, Hypercar en LMU) ; le temps d'arrêt mesuré (`pit_loss_s`, à défaut celui des autres voitures du circuit) est décompté du chrono. En direct, la catégorie vient de la séance (`is_top_class`), pas seulement du calculateur (nouvelle voiture sans historique). Sans ça, 96 % au lieu de ~87 % à Imola (14 % d'énergie restante). En LMU, l'énergie ne donne pas de puissance : l'ingénieur ne dit jamais « attaque ».
- Être doublé ne retire pas de tour : on finit au premier passage après la fin du chrono. Le leader peut en ajouter un (il passe juste avant la fin et repart) : `timed_laps_left` en tient compte avec l'écart au leader (`ApexLapsBehindLeader`, `ApexTimeBehindLeader`).
- Stratégie : ne pas recalculer ce que le jeu donne. `GameFacts` (`sonde-lmu/jeu.json`, lu dès qu'une voiture est chargée) fournit consommation annoncée, ratio réglé et durée de course ; tes tours les remplacent une fois mesurés. Sur 3 courses (11–12 nov.), l'ingénieur a demandé trop d'énergie 3 fois : audit dans le projet claude.ai (`claude/audit-refonte.md`). Suite : lire `/rest/strategy/*` et `/rest/watch/standings` (réponses brutes gardées dans `sonde-lmu/<séance>/`) pour recouper les tours restants.
- Coach en course : `race_voice` ne laisse passer que l'ingénieur, les sorties répétées et les changements de référence (pas de conseil de freinage contre un tour d'essais).
- Phases de virage (`analysis/phases.rs`, 0.40) : bornes de la référence, la somme des phases égale `time_delta_s` (vérifié par `real_corpus`). Le temps s'accumule souvent après l'erreur (corde lente payée en sortie) : dire « perdu en sortie », pas « erreur en sortie ». Chaque conseil reçoit la preuve « Temps par phase » ; si moins de 30 % de la perte est dans les phases attendues par sa cause (`expected_phases`), sa confiance baisse d'un cran avec la raison (13 conseils sur le corpus en 0.40).
- Débrief de course (`analysis/debrief.rs`) : déterministe, chaque perte est mesurée contre la médiane des tours de course sans incident (seuil d'un tour raté : 2 s ou 2 %). Arrêts, énergie et écart au leader viennent de l'enregistrement `.apexlmu` d'origine (`race_extras`) ; sans lui, c'est dit dans `missing`. Aucune priorité sans chiffre (vérifié par `real_corpus`). Le « plein en trop » est un maximum : le temps à l'arrêt peut inclure des pneus.
- Fin de course au temps : `mEndET` n'était pas enregistré avant la 0.38 (`LapEvent.time_remain_s`), donc impossible de rejouer hors ligne les tours restants des courses des 11–12 nov. à moins d'un tour près.
- Ce que l'ingénieur a dit : journal d'Apex (`%LOCALAPPDATA%\app.apexcompanion.desktop\logs`, lignes `coach :`).
- Tours invalidés par le jeu (limites de piste) : `clean` est faux, mais la consommation reste valable. La stratégie filtre sur les drapeaux (stand, sortie, incident, jaune, coupure), pas sur `clean`.
- MFD du jeu (0.39) : `GET /rest/garage/PitMenu/receivePitMenu`, lignes `{name, currentSetting, settings[{text}]}`, « VIRTUAL ENERGY: » au texte « 64% 21 laps » (confirmé en jeu à Monza le 18 nov. : « 100% 20 laps », ratio « 0.65 » ; réponse brute dans `sonde-lmu/<séance>/pit-menu.json`). Relu toutes les 5 s en piste, canaux `ApexMfdEnergy` / `ApexMfdLaps`, journal « jeu : MFD énergie ». `/rest/strategy/overall` répond 400 (paramètre inconnu) : ne pas compter dessus.
- Rythme face à la catégorie (0.39) : à chaque tour du pilote, médiane et meilleur des derniers tours des autres voitures de sa catégorie (mémoire partagée, hors stand, tours > +15 % écartés) dans `LapEvent.class_*`. Le débrief l'utilise à la place de l'écart au leader quand il existe.
- Plein (rejeu 0.43, `docs/plein-juste.md`) : à l'entrée des stands la distance est exacte si un temps d'arrêt est connu (de 30 à 95 s, même annonce à Monza) ; sans temps d'arrêt (0 s), +1 tour fantôme (+4,3 %), c'était le 63 % d'Alex le 18 nov. Fin de course plus gourmande (+5 à +8 % les 3–4 derniers tours) : plein sur max(moyenne récente, médiane de course). Position de l'arrêt / ligne : ≤ 0,05 tour, pas une cause. Restent ±1 tour aux passages de ligne en milieu de course (chrono → tours près de la limite).
- Niveau à régler : aux passages de ligne, l'ingénieur compte en tours entiers (`n - b`) ; selon que l'arrêt est avant ou après la ligne, il peut manquer un tour (Monza, ligne dans la voie : 54 % annoncés pour 58 % nécessaires, panne évitée parce qu'Alex a suivi le MFD). La consigne qui fait foi est donnée à l'entrée des stands (`on_pit_entry`), sur la distance exacte restante (`race_laps_left`, fraction du tour comprise). Le banc de rejeu des courses de Monza (fin de chrono enregistrée) donne les bons tours restants une fois les arrêts du leader faits ; avant, ±1 tour.
- Températures pneus LMU (calibration 20 nov., `docs/calibration-setup.md`) : les 3 zones `mTemperature` = surface (bougent de 5–8 °C entre appui et ligne droite), `mTireCarcassTemperature` = carcasse (inerte, monte sur 5–8 tours), surface 20–30 °C sous la carcasse. Relevés du HUD (Monza, 20 nov., 2 relevés) : température affichée par le jeu ≈ (carcasse + moyenne des 3 zones de la couche interne) / 2, à 1,4 °C près ; colonne « Échelle du jeu (estimée) » depuis la 0.42 (`setup_diag::game_scale_c`). Hypothèse : jamais d'écart à l'idéal de 92 °C affiché (ni surface ni estimation) tant qu'un 3e relevé n'a pas confirmé la formule et que la comparaison du jeu n'est pas prouvée.
- Hauteur de caisse LMU : pas de signal de contact du fond plat ; pointes isolées sous 5 mm (≤ 17 échantillons par course, jusqu'à −3,4 mm) = bruit. Ne jamais écrire « talonnage » ; point bas = 1er centile par tour. À Monza, points bas en ligne droite (appui aéro).
- Diagnostic setup : lit le `.apexlmu` d'origine (rien en base). Tours filtrés comme la stratégie (`setup_diag::usable`). Équilibre = lacet / (vitesse × volant) rapporté aux courbes douces (2–6 m/s²) de la même tranche de 50 km/h ; signe volant/lacet calé sur la séance.
- Dernier relais (Monza 21 nov., 0.47) : une marge d'arrivée sous le demi-tour de réserve faisait basculer le plan 0 ↔ 1 arrêt et dire « Box » à 4 tours de la fin. Règle : pas d'arrêt ajouté si le niveau finit à sec avec ≤ 8 % d'économie (`SAVE_MAX`) ni dans les 2 derniers tours ; cible bornée à 8 % d'économie. `real_corpus` refuse tout « Box » / « Changement de plan » après le dernier arrêt. Temps d'arrêt : `service::measured_pit_loss` sert le calculateur et l'ingénieur (voiture sans historique sur le circuit comprise).
- Bibliothèque de références (0.48) : élue à chaque import et au démarrage (`refresh_references`, idempotent), tour repéré par (séance, numéro) car un recalcul change les identifiants. Ordre d'analyse : référence manuelle ou importée, puis bibliothèque, puis meilleur tour de la séance. Une référence d'une autre séance doit charger sa trace (virages, carte de risque) : corrigé en 0.48. `real_corpus` vérifie la stabilité à la relecture et qu'aucune référence ne vient d'un tour non propre.
- Options de course (0.49) : temps de service mesuré 0,41 s par % d'énergie remis (5 arrêts du corpus, linéaire) ; voie hors service par circuit (tour de rentrée + sortie − 2 tours normaux − arrêt). Coût du lift and coast NON mesurable sur les courses d'Alex (énergie par tour trop stable) : seuil de rentabilité affiché, jamais classé « plus rapide ». Aucun changement de pneus dans le corpus : temps inconnu. « Plus rapide » seulement si l'avance dépasse 2 écarts types.
- Mémoire d'essais setup (0.50) : relue dans les `.apexlmu` (rien en base). Attribuable seulement si un seul réglage (ou une paire de roues d'un essieu) a changé ; validé seulement si l'effet dépasse 2 erreurs types et 1 °C dans le sens attendu (`setup_trial::axle_effect`). Corpus : la paire de Monza 18 nov. est non attribuable (9 réglages changés), aucun changement validé. Pression à froid au cran 0 sur 0–36 sur toutes les courses : pas d'essai « plus basse ».
- Deux instances d'Apex ouvertes (Monza, 18 nov.) : chaque annonce dite deux fois, même fichier d'enregistrement. `tauri-plugin-single-instance` depuis la 0.39.1.

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
- Pas de look « IA » (dégradés violets, emojis décoratifs, cartes génériques). Direction « écran de chronométrage » validée par Alex le 22 nov. (`docs/audit-visuel.md`) : jetons dans `:root` de `styles.css` (6 tailles, grille de 4, 3 rayons), couleurs à sens unique (violet meilleur, vert gain, jaune perte, rouge erreur seulement, bleu comparé/info, orange action), composants de base dans `components/ui.tsx`, garde-fous dans `src/design.test.ts` (ne pas les assouplir).
- Local-first (voir ci-dessus).
- Tout conseil de coaching est justifié par des chiffres de télémétrie (mètres, km/h, dixièmes, %). Alex est débutant et ne peut pas juger un conseil : sans écart mesurable, le coach le dit au lieu d'inventer.
- Simulateur principal actuel : Le Mans Ultimate (Hypercar, LMGT3) ; iRacing reste pris en charge.

## Version et feuille de route

- Version publiée : **v0.52.0** (22 nov. 2026) : refonte visuelle lot 2 (Setups, Références, Séances). Avant : v0.51.0 : refonte visuelle lot 1 (jetons, composants de base, Briefing, Stratégie ; `docs/audit-visuel.md`). Avant : v0.50.0 (22 nov. 2026) : mémoire d'essais setup, file d'essais, setup recommandé = setup actuel + changements validés par mesure (`analysis/setup_memory.rs`, `docs/essai-setup.md`), sonde de ce que remplit l'API garage (`docs/calibration-setup.md` §6). Avant : v0.49.0 (22 nov. 2026) : stratégie de course comparée (`analysis/race_options.rs`, carte « Options de course », annonce « Plan : » au premier passage de ligne, banc `options` dans `real_corpus`, `docs/strategie-course.md`). Avant : v0.48.0 (22 nov. 2026) : bibliothèque de références automatique (`analysis/references.rs`, table `auto_references`, onglet Références, `docs/bibliotheque-references.md`) et sonde en lecture seule de la télémétrie DuckDB écrite par LMU (`connectors/game_telemetry.rs`, `sonde-lmu/telemetrie-jeu.json`). Avant : v0.47.0 (21 nov. 2026) : plan d'arrêts stable en fin de course (Monza 21 nov., `docs/plein-juste.md`), temps d'arrêt du circuit pour une voiture nouvelle, banc sans annonce périmée après la fin du chrono. Avant : v0.46.0 (20 nov. 2026) : ingénieur setup guidé (étape 3). Un essai tiré du constat le plus net de `setup_diag` (carrossage ou pression d'un essieu, en crans, réglage en butée écarté), protocole 5 + 5 tours, verdict mesuré entre deux séances (réglage vérifié dans le setup enregistré, autres changements listés, effet contre dispersion). `docs/essai-setup.md`. Avant : v0.45.0 : sonde de la référence de catégorie.
- Plan produit (analyse concurrentielle du 20 nov. : Delta Auto Insights découpe aussi par phase ; Delta 2.0 et Track Titan font l'analyse de relais et les parcours guidés) : (1) phases de virage, fait en 0.40 ; (2) analyse de relais : dérive virage × phase d'une moitié à l'autre + usure avant / arrière, faite en 0.40 (`analysis/stint_drift.rs`) ; sur les 6 courses du corpus, Alex accélère au fil du relais (aucune priorité « fin de relais » déclenchée), les dérives locales vont jusqu'à 0,29 s ; (3) cause d'un conseil confrontée à la phase mesurée, faite en 0.40 ; reste la consigne radio par phase.
- Mission endurance (20 nov.) : 0 publier 0.42 (fait) ; 1 plein juste (0.43 faite sur le corpus ; course du 20 nov. et journal à verser au corpus pour vérifier) ; 4 fiabilité (0.44 faite sur le corpus) ; 2 référence de catégorie (0.45 : enregistrement et sonde ; comparaison seulement si `rival_probe` dit OK sur une vraie course) ; 3 ingénieur setup guidé (0.46 : un essai, verdict mesuré ; aucun essai complet roulé).
- Suivant, par priorité : refonte visuelle lots 0.53 (Analyse, Comparaison, Progression), 0.54 (Au volant, Réglages, mobile) ; lire les 327 fichiers DuckDB de LMU (sonde du 22 nov. : présents) ; verser le `.apexlmu` de Monza 21 nov. ; premier essai setup complet sur le PC d'Alex (procédure `docs/essai-setup.md`) ; lancer la sonde télémétrie du jeu sur le PC d'Alex (verdict DuckDB) ; verser au corpus la course de Monza du 21 nov. et son journal (vérifier 0.47 sur la vraie course, cause des 65 % pour 67 %) ; valider en course la consigne d'entrée des stands et le rappel « Toujours en piste » ; position de l'arrêt par rapport à la ligne apprise par circuit (consigne exacte dès les passages de ligne) ; valider le diagnostic setup en piste (séances A/B même voiture) ; recouper les tours restants de l'ingénieur avec `time_remain_s` (banc de rejeu des courses enregistrées en 0.38+) ; ménage (relecture en boucle de 3 tours Garage 61 illisibles au démarrage). iRacing et Garage 61 : gelés, maintenance seule.
- Setup Builder : **go** (sonde du 12 nov.), plan dans le projet claude.ai (`claude/plan-setup-builder.md`). Étape 1 faite (v0.36.0). Étape 2 faite (v0.41.0, `analysis/setup_diag.rs`, calibration dans `docs/calibration-setup.md`) ; validation du sens incomplète (Monza 18 nov., 2 courses Cadillac : changements de réglage trop petits, écarts dans le bruit). Étape 3 faite (v0.46.0, `analysis/setup_trial.rs`) : à valider par un vrai essai A/B (un seul réglage changé) ; 3e relevé du HUD (autre moment de séance, pneus plus froids ou plus chauds) pour confirmer la formule de l'échelle du jeu, puis établir ce que le jeu compare à 92 °C.
- À valider sur le PC d'Alex : lectures LMU (limites de piste), annonces de l'ingénieur en course, débrief Gemini gratuit, plugin SimHub.
