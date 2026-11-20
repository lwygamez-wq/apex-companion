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
- Fichiers `.apexlmu` : le décodage doit accepter les anciens fichiers sans les extensions récentes. Extensions : `EXT1` (0.31 : énergie, usure, limites de piste), `EXT2` (0.36 : hauteur par roue, pneus et freins à 5 Hz) ; setup de la séance dans l'en-tête JSON (`setup`). Températures de surface dans l'ordre du jeu : gauche / milieu / droite (gauche = extérieur pour les roues gauches).

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
- LMU vérifié en jeu (essais Imola 9X8, 12 nov.) : `mVirtualEnergy` en % (≈ 3 %/tour), `mWear` = % restant (100 = neuf). Encore à vérifier : remise à zéro des limites de piste (restée à 0 alors que le jeu a invalidé 7 tours sur 9), détection d'arrêt (énergie +2 %).
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
- Niveau à régler : aux passages de ligne, l'ingénieur compte en tours entiers (`n - b`) ; selon que l'arrêt est avant ou après la ligne, il peut manquer un tour (Monza, ligne dans la voie : 54 % annoncés pour 58 % nécessaires, panne évitée parce qu'Alex a suivi le MFD). La consigne qui fait foi est donnée à l'entrée des stands (`on_pit_entry`), sur la distance exacte restante (`race_laps_left`, fraction du tour comprise). Le banc de rejeu des courses de Monza (fin de chrono enregistrée) donne les bons tours restants une fois les arrêts du leader faits ; avant, ±1 tour.
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
- Pas de look « IA » (dégradés violets, emojis décoratifs, cartes génériques) : thème « Paddock » actuel.
- Local-first (voir ci-dessus).
- Tout conseil de coaching est justifié par des chiffres de télémétrie (mètres, km/h, dixièmes, %). Alex est débutant et ne peut pas juger un conseil : sans écart mesurable, le coach le dit au lieu d'inventer.
- Simulateur principal actuel : Le Mans Ultimate (Hypercar, LMGT3) ; iRacing reste pris en charge.

## Version et feuille de route

- Version publiée : **v0.40.0** (20 nov. 2026) : temps perdu par phase de virage (freinage, entrée, milieu, sortie) dans l'analyse, le tableau de bord et le programme. Avant : v0.39.1 (19 nov.) : consigne d'énergie exacte à l'entrée des stands, MFD « trop haut / trop bas », une seule instance.
- Plan produit (analyse concurrentielle du 20 nov. : Delta Auto Insights découpe aussi par phase ; Delta 2.0 et Track Titan font l'analyse de relais et les parcours guidés) : (1) phases de virage, fait en 0.40 ; (2) analyse de relais : dérive virage × phase d'une moitié à l'autre + usure avant / arrière, faite en 0.40 (`analysis/stint_drift.rs`) ; sur les 6 courses du corpus, Alex accélère au fil du relais (aucune priorité « fin de relais » déclenchée), les dérives locales vont jusqu'à 0,29 s ; (3) cause d'un conseil confrontée à la phase mesurée, faite en 0.40 ; reste la consigne radio par phase.
- Suivant, par priorité : valider en course la consigne d'entrée des stands ; position de l'arrêt par rapport à la ligne apprise par circuit (consigne exacte dès les passages de ligne) ; Setup Builder étape 2 (calibrer d'abord : surface 51–63 °C contre idéal 92 °C à Bahreïn) ; recouper les tours restants de l'ingénieur avec `time_remain_s` (banc de rejeu des courses enregistrées en 0.38+) ; limites de piste LMU (compteur lu à 0) ; ménage (relecture en boucle de 3 tours Garage 61 illisibles au démarrage). iRacing et Garage 61 : gelés, maintenance seule.
- Setup Builder : **go** (sonde du 12 nov.), plan dans le projet claude.ai (`claude/plan-setup-builder.md`). Étape 1 faite (v0.36.0). Suivant : étape 2, diagnostic sans conseil (pneus, freins, hauteurs, équilibre), validé sur ≥ 2 séances même voiture avec setups différents.
- À valider sur le PC d'Alex : lectures LMU (limites de piste), annonces de l'ingénieur en course, débrief Gemini gratuit, plugin SimHub.
