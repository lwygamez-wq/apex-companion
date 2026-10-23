# Apex Companion

Compagnon de pilotage simracing pour iRacing : import automatique de la télémétrie, comparaison de tours **par distance**, conseils chiffrés, et un ingénieur de course à la radio. **Les données de conduite restent sur ton PC.**

Ce que fait l'application :

- **Analyse de tour** : virages nommés (relevés et géométrie publique), temps perdu virage par virage, conseils avec niveau de confiance, relecture synchronisée carte + courbes.
- **Carte de risque** : pour chaque virage, temps à prendre contre probabilité et coût d'une sortie de piste.
- **Coach de course** : grille, positions tour par tour, incidents typés et localisés, départ, fin de course, qualification contre rythme course.
- **Programme d'entraînement** : 2 à 3 objectifs mesurables pour la prochaine séance, évalués automatiquement ensuite.
- **Coach vocal en direct** : synthèse des derniers tours contre la référence, focus d'un virage, annonce avant le virage, bips de freinage, voix naturelle hors ligne ; commandes vocales au volant (« répète », « point », « plan », « carburant », « silence », « reprends »).
- **Références Garage 61** (facultatif) : import des tours exportés du site, API pour retrouver tes propres sessions.
- **Séries fixed et open séparées** : type de série lu dans chaque session iRacing ; une référence fixed ne sert qu'aux sessions fixed, une open qu'aux sessions open (analyse, coach en direct, historique).
- **Historique** de tous les tours par circuit, **app iPhone** (page installable) qui ouvre les exports de séance.

- Architecture, limites par simulateur, schéma de données : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Statut : iRacing (fichiers `.ibt` et lecture en direct) implémenté et utilisé au quotidien. LMU, AC et AC EVO ne sont pas encore pris en charge.

## Prérequis (Windows 10/11)

| Outil | Version | Installation |
|---|---|---|
| Visual Studio Build Tools | 2022, charge de travail « Développement Desktop C++ » | `winget install Microsoft.VisualStudio.2022.BuildTools` puis cocher *Desktop development with C++* |
| Rust (toolchain MSVC) | ≥ 1.80 | `winget install Rustlang.Rustup` puis `rustup default stable-msvc` |
| Node.js | ≥ 20 | `winget install OpenJS.NodeJS.LTS` |
| WebView2 | — | Déjà présent sous Windows 11 |

## Commandes

```powershell
# 1. Dépendances front
npm install

# 2. Lancer l'application en développement (Vite + fenêtre Tauri, rechargement à chaud)
npm run tauri dev

# 3. Tests
cargo test -p apex-core          # parseur, analyse, coach, stockage, service
npm test                         # tests front (Vitest)
npm run typecheck                # TypeScript strict

# 4. Valider le parseur sur TES fichiers iRacing (fortement recommandé)
$env:APEX_IBT_DIR = "$env:USERPROFILE\Documents\iRacing\telemetry"
cargo test -p apex-core --test ibt_real -- --nocapture

# 5. Rapport texte sans UI (démo ou fichier réel)
cargo run -p apex-core --example demo_report
cargo run -p apex-core --example demo_report -- "C:\chemin\vers\session.ibt"

# 6. Build local de l'installeur (ou laisser GitHub le faire : voir « Installeur sans compiler »)
npm run tauri build
```

## Premiers pas

1. Lance l'application, puis clique sur **Charger une session démo** pour explorer l'analyse sans simulateur.
2. Dans iRacing, active l'enregistrement de la télémétrie avec **Alt+L** pendant la session.
3. Utilise **Importer des .ibt** ou **Scanner le dossier iRacing**. Avec l'import automatique activé, les fichiers sont importés à la fin de chaque session détectée.
4. Dans **Analyse de tour**, choisis une référence et un tour à analyser : les conseils apparaissent triés par priorité.

## Données et confidentialité

- Base locale : `%APPDATA%\app.apexcompanion.desktop\apex.db` (SQLite). Journaux locaux dans le dossier `logs` voisin. Copies de garde des `.ibt` dans `telemetrie-gardee`, voix et reconnaissance vocale dans `voix`.
- **Aucune donnée de conduite n'est envoyée.** Connexions sortantes, toutes déclenchées par l'application elle-même :
  - mises à jour : lecture de `latest.json` sur GitHub (désactivable dans Réglages) ;
  - Garage 61, seulement si tu saisis un jeton : recherche de tours de référence et de tes propres sessions (critères envoyés : circuit, voiture, type de série) ;
  - voix Azure, seulement si tu saisis une clé Azure AI Speech : le texte des messages radio est envoyé à Azure pour être prononcé (rien d'autre) ;
  - téléchargements à la demande, une seule fois : moteur vocal Piper (GitHub), voix françaises (Hugging Face), reconnaissance vocale Vosk (GitHub) et son modèle français (alphacephei.com).
- Le micro n'est ouvert que pendant l'appui sur le bouton radio ; l'audio est analysé sur le PC puis jeté.
- Le CSP bloque toute connexion sortante de l'interface. Réglages → export JSON complet, suppression totale des données, diagnostic sans données personnelles.

## Marques et SDK

iRacing, Le Mans Ultimate, Assetto Corsa, Assetto Corsa EVO et Assetto Corsa Competizione sont des marques de leurs détenteurs respectifs. Ce projet n'est ni affilié ni approuvé par eux. Il n'utilise que des formats et interfaces publics (format irsdk, mémoires partagées exposées par les jeux) et ne contourne aucune protection.

## Licence

MIT — voir [`LICENSE`](LICENSE).

## Installer et mettre à jour

- **Première installation** : page *Releases* du dépôt GitHub → télécharger `ApexCompanion_X.Y.Z_x64-setup.exe` → lancer. SmartScreen affichera « éditeur inconnu » (installeur non signé Authenticode) : *Informations complémentaires → Exécuter quand même*.
- **Mises à jour** : l'application vérifie au démarrage s'il existe une version plus récente et propose de l'installer. L'installeur téléchargé est vérifié par **signature minisign** (clé publique intégrée à l'app, clé privée uniquement dans les secrets GitHub) avant d'être lancé.

## Publier une nouvelle version

1. Augmenter `version` dans `src-tauri/tauri.conf.json` (et `package.json`, `Cargo.toml`).
2. Onglet **Actions** → « Publier une version Windows » → **Run workflow**.
3. Le workflow lance les tests, construit et signe l'installeur, puis crée la release `vX.Y.Z` avec `latest.json`.

Prérequis (une seule fois) : secret de dépôt `TAURI_SIGNING_PRIVATE_KEY` contenant la clé privée générée par `npx tauri signer generate`. Si cette clé est perdue, les installations existantes ne pourront plus se mettre à jour automatiquement (réinstallation manuelle avec une nouvelle clé).

## Crédits des données de circuits

Les noms de virages des circuits sans relevé sur session réelle sont placés à partir de la
géométrie publique des tracés (lignes médianes issues d'OpenStreetMap) :

- [TUMFTM racetrack-database](https://github.com/TUMFTM/racetrack-database) (LGPL-3.0)
- [bacinger/f1-circuits](https://github.com/bacinger/f1-circuits) (MIT, © Tomislav Bacinger)
- [markthebault/open-racetrack-db](https://github.com/markthebault/open-racetrack-db) (ODbL 1.0) — données © [contributeurs OpenStreetMap](https://www.openstreetmap.org/copyright) : Fuji, Motegi, Bathurst, Road Atlanta, Monaco, Albert Park

Seules des valeurs dérivées (position et sens de chaque virage) sont intégrées à l'application.

## Crédits voix et reconnaissance vocale

Téléchargés à la demande sur le PC de l'utilisateur, non inclus dans l'installeur :

- [Piper](https://github.com/rhasspy/piper) (MIT, © Michael Hansen) et les voix françaises de [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices) (licence propre à chaque voix).
- [Vosk](https://github.com/alphacep/vosk-api) (Apache-2.0) et le modèle [vosk-model-small-fr-0.22](https://alphacephei.com/vosk/models) (Apache-2.0).
