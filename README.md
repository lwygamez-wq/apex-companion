# Apex Companion

Compagnon de pilotage simracing : import de télémétrie, comparaison de tours **par distance**, conseils de pilotage chiffrés avec niveau de confiance. **100 % local.**

- Architecture, limites par simulateur, schéma de données et plan de développement : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Statut : **MVP (phase 1)**. iRacing `.ibt` est implémenté. La lecture live iRacing est écrite mais pas encore validée. LMU, AC et AC EVO sont prévus.

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
cargo test -p apex-core          # 41 tests : parseur, analyse, stockage, service
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

- Base locale : `%APPDATA%\app.apexcompanion.desktop\apex.db` (SQLite). Journaux locaux dans le dossier `logs` voisin.
- Aucune télémétrie envoyée. Seule connexion : la vérification des mises à jour auprès de GitHub (lecture de `latest.json`, désactivable dans Réglages). Le CSP bloque toute connexion sortante de l'interface.
- Réglages → export JSON complet, suppression totale des données, diagnostic sans données personnelles.
- Aucun secret ni clé d'API : pas de `.env` nécessaire.

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
