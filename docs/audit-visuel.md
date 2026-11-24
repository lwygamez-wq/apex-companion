# Audit visuel et refonte (0.51 →)

22 nov. 2026. L'aperçu local fait tourner l'app complète sur une vraie base, construite à partir du corpus (13 séances, dont 7 LMU). Les commandes IPC sont servies par le vrai `AppService` : outil `examples/zz_preview_server.rs` et `src/preview.tsx`, exclus de git. Captures Playwright à 1280×720, 1920×1080 et 2560×1440, en quatre états : données, vide, chargement sans fin et erreur. L'écran « Au volant » est aussi capturé avec une séance LMU simulée en piste.

## 1. Constat « avant » (0.50.0)

### Mesures sur `styles.css` et les composants

| Mesure | Valeur | Cible |
|---|---|---|
| Tailles de texte distinctes | **20** (11 à 40 px, dont 12,5 px) | ≤ 6 |
| Valeurs d'espacement distinctes (px) | **26** (1 à 50 px, dont 3, 5, 7, 9, 11, 13) | une échelle 4/8 |
| Rayons distincts | **15** (0 à 999 px) | ≤ 3 |
| Couleurs écrites en dur dans le CSS | **91** distinctes, 149 occurrences | 0 hors jetons |
| Couleurs écrites en dur dans les composants TSX | **138** (carte 71, graphiques 28, progression 15) | 0 hors jetons |
| `:focus-visible` / `:hover` / `:disabled` | 2 / 19 / 1 | sur chaque composant de base |
| `prefers-reduced-motion` | 1 | toutes les animations |

### Contrastes des jetons (WCAG)

| Jeton | sur fond (#111318) | sur carte (#1a1d24) | sur carte claire (#242833) |
|---|---|---|---|
| texte #f2f4f8 | 16,9 | 15,3 | 13,4 |
| secondaire #a3aab8 | 8,0 | 7,2 | 6,3 |
| violet #a78bfa | 6,8 | 6,2 | 5,4 |
| vert #3dd9b3 | 10,4 | 9,5 | 8,3 |
| « jaune » #ff9a5c | 8,9 | 8,1 | 7,0 |
| accent #ff7a45 | 7,2 | 6,5 | 5,7 |
| **rouge #e10600** | **3,7** | **3,4** | **3,0** (texte : échoue) |
| **trait #2e3340** | 1,5 | 1,3 | 1,2 (élément graphique : échoue) |

Le texte courant passe partout. Le rouge de marque échoue comme couleur de texte. Les traits de séparation et les bords des champs échouent comme éléments graphiques (< 3:1).

### Sens des couleurs ambigus

- **« Jaune » = orange** : la perte (#ff9a5c) et l'accent de marque, celui des boutons principaux (#ff7a45), ont presque la même teinte. Un bouton « Importer » et une perte de temps ont la même couleur.
- **Rouge** : il sert à la fois de marque (logo, soulignement des titres, barre de navigation active, meilleure option de course) et d'erreur.
- **Violet** : il est réservé au « meilleur absolu », mais il colore aussi le badge « Setup libre » sur toutes les séances et dans Progression.

### Par page (1280×720 sauf mention)

| Page | Défauts mesurés |
|---|---|
| Briefing | La bannière iRacing « Alt+L » occupe le premier écran, alors que LMU est le simulateur principal. Deux boutons d'en-tête de même poids (Importer, Scanner iRacing) : pas d'action principale claire. Chargement : page vide, sans indicateur. |
| Stratégie | Bonne hiérarchie, mais 3 niveaux d'encadrés imbriqués (carte, consigne, options). La note « Non mesuré… » fait 3 lignes en gris. 5 à 8 cibles sous 32 px (boutons « min / tours », bascule d'unité). |
| Setups | Sans dossier Steam, la carte montre un message et un bouton : correct. « Lecture de l'enregistrement… » est un texte sans indicateur visuel. |
| Séances | 17 cibles sous 32 px : bouton « ··· » de 32×24, filtres de 24 px de haut. Badge « Setup libre » violet. Erreur : bloc rouge sans action (« Réessayer » absent). |
| Références | Lisible. Pas d'état vide illustré (texte seul). |
| Progression | « −7.574 s » passe à la ligne dans sa carte (texte coupé). 8 cartes et 5 483 px de haut à 1280. Les records sont en violet italique (cohérent). 11 cibles sous 32 px. |
| Au volant | Chiffres principaux à 28 px. Le relais, la radio, la voix, le volume, la clé Azure et le bouton radio sont sur le même écran : 2 088 px de haut, au moins 6 informations à la fois. Le bouton « Choisir le bouton » est coupé sur 3 lignes. |
| Réglages | 3 867 px de haut, 9 cartes, 10 cibles sous 32 px. |

Aucun défilement horizontal mesuré de 1280 à 2560 px de large. Aucune erreur JavaScript de l'app (les erreurs relevées venaient de l'aperçu, corrigé).

Captures : `docs/audit/avant/`.

## 2. Direction visuelle retenue : « écran de chronométrage »

Une seule direction, celle des écrans de timing et des outils de télémétrie sombres. Le fond reste neutre et calme, les chiffres sont la matière première, la couleur ne sert qu'à dire quelque chose. Raisons :

1. Alex est débutant : la couleur doit être un signal fiable, jamais de la décoration. On garde la convention qu'il verra sur tous les écrans de course : violet = meilleur absolu, vert = gain, jaune = perte.
2. L'app sert surtout de près, sur un écran de PC, entre deux séances. Mais « Au volant » se lit de loin, à distance de volant. Il faut donc deux régimes : dense et aligné sur les écrans d'analyse, très gros et très peu d'informations « Au volant ».
3. L'identité « Paddock » actuelle reste : Archivo large et grasse pour les chiffres, Manrope pour le texte, fonds asphalte, angles légèrement biseautés. Elle est déjà lisible (texte courant ≥ 6,3:1). Il manque la discipline : trop de valeurs, couleurs ambiguës, états absents.

### Règles

- **Couleurs à sens unique.**

  | Couleur | Rôle unique |
  |---|---|
  | Orange (accent) | action principale et repère de navigation |
  | Rouge | erreur et danger seulement (il n'est plus la couleur de marque des titres) |
  | Jaune | perte |
  | Vert | gain, réussite |
  | Violet | meilleur absolu |
  | Bleu | tour comparé et information |
  | Gris | tout le reste, badges neutres compris (« Setup libre » devient gris) |

- **Six tailles de texte** : 12, 14, 16, 20, 28, 48 px. Le 48 sert aux chiffres héros et à l'écran « Au volant ».
- **Espacements** sur une grille de 4 : 2, 4, 8, 12, 16, 24, 32, 48 px.
- **Trois rayons** : 6 px (contrôles et badges), 12 px (cartes), 999 px (pastilles).
- **Trois niveaux de surface** : fond, carte, élément dans une carte. Bords à ≥ 3:1 sur les contrôles.
- **Une action principale par page**, en haut à droite, en orange. Les autres boutons sont secondaires (contour) ou discrets (texte).
- **États** : chaque chargement montre un squelette, chaque erreur une alerte avec « Réessayer », chaque liste vide un état vide qui dit quoi faire.
- **Mouvement** : transitions de 120 ms sur la couleur et le contour seulement, toutes coupées par `prefers-reduced-motion`.

## 3. Plan de migration

| Version | Lot |
|---|---|
| 0.51 | Jetons et composants de base ; Briefing et Stratégie |
| 0.52 | Setups, Références, Séances |
| 0.53 | Analyse, Comparaison, Progression (graphiques et carte de piste sur les jetons) |
| 0.54 | Au volant (lecture à distance), Réglages, app iPhone |

Chaque lot ajoute ici ses captures « après » et ses mesures.

## 4. Lot 0.51 : jetons, composants de base, Briefing et Stratégie

### Ce qui change

- **Jetons** (`:root` de `styles.css`) :
  - surfaces (fond, carte, élément, survol) et traits (décoratif, fort) ;
  - couleurs à sens unique ;
  - 6 tailles de texte, grille de 4, 3 rayons, 3 élévations, transition de 120 ms ;
  - les anciens noms (`--panel`, `--yellow`…) restent comme alias.
- **Tout `styles.css` passe sur les jetons** : les 20 tailles, 26 espacements, 15 rayons et 149 couleurs en dur sont ramenés à l'échelle. Toutes les pages en profitent dès ce lot.
- **Sens des couleurs** :
  - la perte devient un vrai jaune (#f5c542, au lieu de l'orange #ff9a5c, presque identique à l'accent) ;
  - le rouge ne sert plus qu'aux erreurs ; les repères (titre, navigation, sélection, onglet actif) passent à l'orange de l'accent ;
  - « Setup libre / imposé » devient neutre ;
  - « la plus rapide » (options de course) passe au violet.
- **Composants de base** (`components/ui.tsx`) : Button (primaire, secondaire, discret), Card, Stat, Badge, Alert, LoadError (avec « Réessayer »), EmptyState, Skeleton, Tabs, Tooltip. Tous ont focus visible, survol et désactivé. Les contrôles font 32 px au moins, et la cible d'une case à cocher est son étiquette entière.
- **Briefing** :
  - une seule action principale, « Analyser ma dernière séance » ; « Importer » et « Scanner iRacing » passent en secondaire ;
  - chargement en squelette, erreur avec « Réessayer », état vide qui dit quoi faire ;
  - le bandeau iRacing « Alt+L » descend en bas de page, en alerte discrète : la fonction est gardée ;
  - l'objectif du prochain run passe à 28 px, en couleur d'action et non plus de perte.
- **Stratégie** :
  - le plan de course (la décision) passe avant le convertisseur du stand (l'outil) ;
  - chargement en squelette, message « Pas encore de plan » en alerte.
- **Garde-fous en CI** (`src/design.test.ts`) : contrastes calculés sur les jetons, 6 tailles, échelle d'espacement, 3 rayons, aucune couleur en dur dans le CSS ni dans les composants. 7 fichiers restent à migrer (carte, graphiques, Progression, course, rejeu) : la liste ne peut que rétrécir.

### Mesures (vérifiées)

| Critère | Avant | Après 0.51 |
|---|---|---|
| Tailles de texte (CSS) | 20 | **6** (12, 14, 16, 20, 28, 48) |
| Espacements (CSS) | 26 valeurs | **8** (2, 4, 8, 12, 16, 24, 32, 48) |
| Rayons | 15 | **3** (8, 12, pilule) |
| Couleurs en dur dans le CSS | 149 | **0** |
| Couleurs en dur dans les TSX | 138 dans 9 fichiers | 0 hors des 7 fichiers listés (lots 0.52–0.54) |
| Contraste texte, tous jetons × toutes surfaces | rouge 3,0 à 3,7 | **≥ 4,5** (plus bas : danger sur survol, 4,52) |
| Contraste bords de contrôles | 1,2 à 1,5 | **≥ 3,36** |
| Cibles < 32 px, Briefing / Stratégie (1280) | 1 / 5 à 8 | **0 / 0** |
| Cibles < 32 px, Séances (1280) | 17 | 4, en état erreur (filtres) |
| Défilement horizontal, 1280 → 2560 | aucun | aucun |
| Chargement Briefing | page vide | squelette |
| Erreur Briefing | bloc rouge sans action | alerte avec « Réessayer » |

Captures : `docs/audit/apres-0.51/`.

### Reste (listé, non fait)

- Tailles de texte écrites dans le code des composants (`style={{ fontSize }}`) : 28 occurrences dans 19 fichiers, migrées page par page aux lots suivants.
- Carte de piste, graphiques, Progression, rejeu : couleurs en dur (lot 0.53).
- « Au volant » : refonte de lecture à distance (lot 0.54). Les chiffres sont toujours à 28 px.
- App iPhone : `mobile.css` garde ses propres valeurs (lot 0.54).
- Vérification à faire sur le PC d'Alex : rendu des couleurs `color-mix` dans WebView2. WebView2 suit Chrome et le prend en charge depuis Chrome 111, mais ce n'est vérifié qu'en Chromium Linux.

## 5. Lot 0.52 : Setups, Références, Séances

- **Séances** :
  - filtre par type de séance en onglets (composant Tabs) ;
  - chargement en squelette, erreur avec « Réessayer », état vide qui dit quoi faire ;
  - message d'import en alerte avec « OK ».
- **Références** : chargement, erreur avec « Réessayer », état vide (« roule une séance avec au moins un tour propre complet »).
- **Setups** :
  - le diagnostic de séance, avec son setup recommandé, passe en premier. C'est la carte qui sert à décider. Le comparateur de fichiers `.svm` passe en second : c'est un outil, et il demande le dossier Steam ;
  - « Lecture de l'enregistrement… » devient un squelette ;
  - « Deux voitures différentes » et les erreurs deviennent des alertes.

| Mesure, 1280 → 2560, états données / vide / chargement / erreur | Résultat |
|---|---|
| Cibles < 32 px sur Setups, Séances, Références | 0 (Séances : 17 avant la 0.51, 4 en 0.51) |
| Défilement horizontal | aucun |
| Erreurs JavaScript | aucune |

Captures : `docs/audit/apres-0.52/`.

## 6. Lot 0.53 : Analyse, Comparaison, Progression (graphiques et carte)

- **Plus aucune couleur en dur dans les composants desktop.** Il y en avait 138 dans 9 fichiers avant la 0.51. Le test `design.test.ts` impose désormais zéro, sans liste d'exceptions.
  - Carte de piste (SVG) : nouveaux jetons pour la piste, l'herbe, la peinture, les vibreurs, la voiture, les pédales (frein, gaz partiel, plein gaz, roue libre) et la vitesse en 6 paliers.
  - Graphiques uPlot (canvas) : couleurs lues dans les jetons au moment du dessin (`lib/palette.ts`).
- **Corrections de sens** :
  - sur la carte en mode « écart », la perte passe de l'orange au jaune, comme partout ailleurs ;
  - le gaz partiel a désormais la même couleur (vert clair) dans les deux modes de la carte : il était orange dans l'un, vert clair dans l'autre ;
  - le « hors piste » de Progression passe en rouge (incident) au lieu d'orange ;
  - sur la course, les incidents de 4x et plus sont rouges, les autres jaunes.
- **Analyse** :
  - chargement de la séance et de la comparaison en squelette, erreurs avec « Réessayer » ;
  - le titre « Virage par virage » n'est plus affiché deux fois ;
  - boutons de zoom de la carte et bouton d'inversion à 32 px.
- **Progression** : chargement, erreur avec « Réessayer », état vide.

| Mesure (Analyse et Progression, 1280 et 1920 ; Progression aussi 2560, vide, chargement, erreur) | Résultat |
|---|---|
| Cibles < 32 px | 0 (Analyse : 3 avant ce lot ; Progression : 11 avant la 0.51) |
| Défilement horizontal | aucun |
| Couleurs en dur dans les composants desktop | 0 |

Captures : `docs/audit/apres-0.53/`.

## 7. Lot 0.54 : Au volant, Réglages, app iPhone

- **Au volant** : un bandeau de lecture à distance en haut de page.
  - État de la source en clair : « En piste », « Connecté, au stand », ou le message d'attente ou d'erreur du jeu.
  - **Trois informations au plus**, chiffres à **48 px** : la consigne de l'ingénieur (« Box fin T21 », « Box ce tour », « Lift and coast », « Dans la cible », « Arrêt nécessaire »), la réserve d'énergie ou de carburant avec les tours possibles, et les tours restants.
  - Couleur seulement quand elle a un sens : orange pour « Box ce tour » (action), jaune pour le lift and coast (perte), vert pour « dans la cible », rouge pour « arrêt nécessaire ».
  - Aucune animation. Visible sans défilement à 1280×720.
  - Logique dans `components/LiveGlance.tsx`, testée.
  - Le « Détail du relais » ne répète plus l'énergie, les tours restants ni le message « Il manque… », déjà dans le bandeau. Il garde la conso par tour, le carburant à bord, le ratio et le bloc ingénieur (MFD, arrêt).
  - Les réglages de voix et de bouton radio passent sous un titre « Réglages de la radio ».
- **Réglages et radio** : chaque ligne à case à cocher est cliquable en entier (cible de la hauteur de la ligne, au lieu d'une case de 18 px). Curseurs à 32 px de haut.
- **App iPhone** : `mobile.css` passe sur les mêmes jetons (6 tailles, couleurs, rayons). La barre du logo prend l'orange de l'accent au lieu du rouge.
- **Tailles de texte écrites dans le code** : ramenées sur l'échelle (13 → 14, 26 → 28) et vérifiées par un test. Seuls échappent les textes de la carte, mis à l'échelle du zoom.

## 8. Bilan des critères (fin de la refonte, 0.54)

| Critère | Mesure | Statut |
|---|---|---|
| Contraste texte ≥ 4,5:1, éléments graphiques ≥ 3:1 | calculé par `design.test.ts` sur tous les jetons × toutes les surfaces | **vérifié** |
| ≤ 6 tailles de texte, une échelle d'espacement, ≤ 3 rayons | 6 / 8 valeurs sur une grille de 4 / 3 rayons (CSS desktop et mobile, tailles écrites dans les composants) | **vérifié** (test) |
| Aucune couleur écrite en dur hors jetons | 0 dans le CSS desktop et mobile et dans tous les composants | **vérifié** (test) |
| États hover, focus, désactivé, vide, chargement, erreur | présents sur les composants de base ; chargement, erreur et vide sur Briefing, Stratégie, Séances, Références, Setups, Analyse, Progression | **vérifié** sur captures |
| Cibles ≥ 32 px | 0 cible plus petite sur les 8 pages, de 1280 à 2560, dans les 4 états | **vérifié** (mesure Playwright) |
| Navigation clavier avec focus visible | anneau orange `:focus-visible` sur tous les contrôles ; lignes de séance activables à Entrée | vérifié sur le code ; **testé de bout en bout en 0.56** (§10) |
| Aucun défilement horizontal, 1280 → 2560 | aucun mesuré | **vérifié** |
| Au volant : texte principal ≥ 48 px, ≤ 3 informations, sans défilement | 48 px, 3 tuiles, bandeau entier à 1280×720 | **vérifié** sur captures ; **à valider en jeu** sur l'écran réel d'Alex, à distance de volant |
| Un seul style de tableau, une seule carte de statistique, une seule alerte | composants `Stat` et `Alert` uniques ; anciennes classes retirées en 0.56 (§10) | **vérifié** en 0.56 |
| Tests existants verts, aucun calcul modifié | Vitest 63/63, aucun fichier Rust de logique touché | **vérifié** |

### Reste à faire (listé)

- Remplacer les dernières variantes de chiffres clés (`.ss-kpi`, `.lf-main`, `.kpi`) et de messages (`.warning`, `.error`) par `Stat` et `Alert`.
- Tester toute la navigation au clavier, page par page.
- Vérifier dans WebView2 (PC d'Alex) le rendu de `color-mix` et de `var()` dans les attributs SVG. Ce n'est vérifié que sous Chromium Linux.

Captures : `docs/audit/apres-0.54/`.

## 9. Lot 0.55 : identité « mur des stands »

Retour d'Alex sur la 0.54 (23 nov.) : « on voit de suite que c'est une IA qui l'a faite ». Il avait raison. Les lots 0.51 à 0.54 ont rangé le code (jetons, états, contraste), mais l'apparence n'a presque pas bougé : même police Manrope, mêmes cartes grises arrondies, mêmes petits libellés en capitales. En plus, l'aperçu local ne chargeait pas les polices, donc les captures de ces lots montraient une police système. Corrigé : `preview.tsx` importe maintenant les mêmes polices que `main.tsx`.

Ce qui change, et pourquoi c'est propre au sujet :

- **Panneau de stand** : c'est l'élément signature. La plaque noire est faite de lames séparées par un cadre, avec des lettres peintes (Big Shoulders Display, en capitales parce que c'est le langage d'un panneau). Il sert au message principal : la consigne du prochain run (Briefing), les tuiles d'Au volant et le plan de plein (Stratégie). C'est exactement l'objet qu'on lit en roulant.
- **Écrans d'ingénieur** : chaque panneau a un bandeau de titre comme une fenêtre de logiciel de télémétrie, un trait de 1 px et des rayons de 2 à 4 px. Les tableaux de chronométrage sont denses, avec des lignes alternées.
- **Typographie** : Saira (largeur variable) pour toute l'interface et les chiffres alignés ; Big Shoulders Display seulement pour les panneaux. Il n'y a plus de libellés en capitales espacées ni de chiffres en italique large.
- **Navigation** : elle est rangée par moment du week-end (Avant de rouler, En piste, Après la séance), sans icônes. L'en-tête est un mot-marque « APEX/ ».
- **Couleurs** : le fond nuit bleutée (`#0f141b`) et l'orange d'action (`#ff7a1f`) ont été ajustés. Les sens des couleurs sont inchangés (violet meilleur, vert gain, jaune perte, rouge erreur, bleu info, orange action). La sélection d'un onglet est en blanc, pas en orange : l'orange reste réservé à l'action.
- **Inchangé** : les garde-fous de `design.test.ts` (contraste, 6 tailles, grille de 4, 3 rayons, aucune couleur en dur), aucun calcul modifié.

Captures : `docs/audit/apres-0.55/` (Briefing, Stratégie, Séances, Setups, Progression, Au volant en piste, iPhone).

## 10. Lot 0.56 : fin de refonte (§8)

| Point resté ouvert en 0.54 | Fait en 0.56 | Statut |
|---|---|---|
| Un seul chiffre clé, une seule alerte | `.ss-kpi` (Briefing, Analyse), `.lf-main` (Au volant), `.kpi` / `.kpi-sub` (Progression, app iPhone), `.race-kpi` (bilan de course iRacing) remplacés par `Stat` ; `.warning` / `.error` (11 endroits, desktop et iPhone) remplacés par `Alert`. Règles CSS supprimées. Il ne reste aucune de ces classes dans les composants. | **vérifié** (recherche dans le code, captures) |
| Navigation au clavier, page par page | Script `docs/audit/clavier.cjs` (Playwright). Sur chaque page : Tab jusqu'à l'entrée du menu, Entrée, puis Tab sur toute la page. Résultat sur les 8 pages : menu atteint et page ouverte au clavier, focus visible sur chaque contrôle (de 12 à 45 contrôles par page). Corrigés : focus invisible sur les lignes de Séances (`outline: none`) et 15 champs sans nom lisible par un lecteur d'écran (Réglages, Au volant, case « tour de formation »). Les 3 champs Azure non atteints sont dans un volet replié : on l'ouvre avec Entrée sur « Clé Azure ». | **vérifié** (Chromium) |
| `color-mix`, `:has()` et `var()` dans les SVG sous WebView2 | Pas de PC Windows ici. Ajouté dans Réglages › Avancé › Affichage, le bouton « Vérifier l'affichage » : l'app teste elle-même `color-mix`, `:has()`, la couleur calculée d'un dessin SVG en `var(--accent)`, les deux polices, et donne la version du moteur. Tout est OK sous Chromium 157. | **à valider par Alex** : une capture de ce panneau dans l'app Windows suffit |

Captures : `docs/audit/apres-0.56/`.

## 11. Carte du circuit (0.57)

But : la carte est l'instrument principal pour répondre à « où est-ce que je perds du temps ». Alex est débutant : un coup d'œil doit lui dire quels virages comptent.

### Audit avant (0.56, Monza 18 nov., tour 34 contre tour 15, 5 793 points)

Script : `docs/audit/carte.cjs`. Il passe par Résumé, Données normal, 50 % et plein écran, en 1280, 1920 et 2560, avec et sans bords. Captures dans `docs/audit/carte-avant/`.

| Défaut | Mesure |
|---|---|
| Numéros de virage faux | Numéros tirés des noms : « 1, 2, 1, 2, 5, 6 » pour Rettifilo, Roggia, Lesmo 1, Lesmo 2, Ascari, Parabolica |
| Étiquettes qui se chevauchent | 1 paire à 1280 (Rettifilo × Ascari), en Résumé et en 50 % |
| Noms de virage | Jamais affichés en vue circuit (seulement sous 1 200 m de large) |
| Cartouches vitesse / pédales | Posés sur la carte, ils masquent un virage à 1280 |
| Modes à comprendre | 3 couleurs (gain / perte, frein / gaz, vitesse) en 3 boutons, plus 9 à 12 boutons d'outils |
| Lisibilité de l'écart | 4 niveaux de couleur, sans échelle |
| Repères | Ni sens de la piste ni distances ; voiture dessinée en monoplace |
| Rendu | 60 images/s au zoom molette (image médiane 16,7 ms ; 90e centile 16,8 ms, 33 ms en plein écran 2560) |

### Ce qui change (0.57)

- **Étiquettes** : numéro d'ordre sur le tour et nom du virage, placés en pixels écran par un algorithme anti-collision (`lib/mapLabels.ts`).
  - L'étiquette complète est essayée à 10 angles et 4 distances, puis la pastille seule.
  - La trajectoire est évitée si possible, mais un virage n'est jamais retiré pour elle.
  - Ordre de placement : virage sélectionné, puis plus grosse perte.
  - Tests Vitest : chicane serrée, 12 virages sur 120 px, zones réservées, trajectoire.
- **Écart de temps continu (mode par défaut)** : couleur tous les 10 m selon la pente de l'écart sur ±15 m. Vert pour le gain, jaune pour la perte, 6 crans par côté. L'échelle est affichée : couleur pleine à 0,2 s gagnés ou perdus sur 100 m.
- **Autres couleurs (pédales, vitesse)** : dans un menu déroulant « Couleur ». Les options rares (fantôme même temps / même endroit, vue pilote, tailles) passent dans « Plus ».
- **Piste** :
  - un ruban à la largeur réelle, en bords relevés quand ils existent, sinon 12 m constants ;
  - un trait tous les 100 m, avec une cote tous les 500 m une fois zoomé (une cote n'est jamais écrite sur une étiquette) ;
  - une ligne de départ en damier et une flèche de sens 120 m après la ligne ;
  - un fond d'asphalte sobre partout.
- **Voiture** : silhouette générique de prototype fermé, sans marque ni modèle. Feux arrière et halo rouge au freinage (vérifié en relecture au freinage de Rettifilo). Le fantôme de la référence est en contour pointillé.
- **Cartouches vitesse et pédales** : sous la carte, plus dessus.
- **Clavier** : la carte prend le focus (contour visible).
  - Flèches gauche et droite : virage suivant ou précédent, avec zoom sur le virage.
  - `+` et `−` : zoom. `0` : vue circuit.

### Mesures après (même script)

| Critère | 1280 | 1920 | 2560 |
|---|---|---|---|
| Étiquettes qui se chevauchent (Résumé, Données, 50 %, plein écran) | 0 | 0 | 0 |
| Plus petit texte de la carte | 22 px | 22 px | 22 px |
| Image médiane au zoom molette | 16,7 ms | 16,7 ms | 16,7 ms |

Le 90e centile reste à 33 ms en plein écran 2560, comme avant.

Captures : `docs/audit/carte-0.57/` (Résumé, 50 %, pédales, sans bords, plein écran, zoom sur un virage, feu de freinage).

### Reste (0.58 et 0.59)

- Comparaison de freinage par virage (repères F avec l'écart en mètres) et fiche virage synchronisée.
- Les 3 virages prioritaires marqués d'office.
- Choix de la référence dans la carte, app iPhone, plan d'attaque.

### À valider par Alex

- La lisibilité de la carte sur son écran, en vue circuit : noms de virage et dégradé gain / perte lisibles sans zoomer.

## 12. Comparaison de freinage et fiche virage (0.58)

### Calcul (apex-core, `analysis/braking.rs`)

- **Point de freinage** : premier point où la pédale atteint 10 % (`BRAKE_ON`, même seuil que `corner_metrics`), entre le début de la fenêtre du virage et la vitesse mini. Il vient du **canal frein mesuré**, jamais d'une décélération.
- **Zone de freinage** : du point de freinage au relâchement (pédale repassée sous 10 %).
- **Écarts** :
  - point de freinage (> 0 : tu freines plus tard) ;
  - vitesse mini à la corde ;
  - remise des gaz (20 % tenus sur 10 m, comme avant).
- **Seuils d'égalité** : sous 5 m, « au même endroit » ; sous 1 km/h, rien n'est écrit.
- **Rien n'est comparé dans trois cas**, et la raison est donnée (`missing`) :
  - un tour sans canal frein (pédale jamais au-dessus de 5 % sur tout le tour) ;
  - un virage sans freinage (pic sous 20 %) ;
  - un point ambigu : deux appuis de plus de 5 m séparés de plus de 10 m (une touche de frein puis le vrai freinage). Un freinage modulé, où la pédale repasse un instant sous 10 %, compte pour un seul appui.
- **Phrase d'ingénieur chiffrée** (`braking_sentence`), par exemple « Tu freines 18 m trop tôt, 4 km/h de moins à la corde, gaz 12 m plus tard. »
- **Tests** : 6 tests unitaires sur fixtures (écarts exacts, moins de 5 m, canal absent, touche de frein, freinage modulé, virage sans frein).
- **`real_corpus`** : instantanés mis à jour exprès. Seuls les champs `braking` et `sentence` sont ajoutés, aucun autre chiffre ne bouge. Trois règles sont vérifiées :
  - aucun écart de freinage sans les deux points mesurés ;
  - toujours une raison quand le freinage n'est pas comparé ;
  - la phrase cite un chiffre.

  Sur les 166 virages comparés du corpus : 134 ont un écart mesuré, 19 un point ambigu, 13 pas de freinage, 0 un canal absent.

### Carte

- **Repères F** : ils viennent désormais de cette comparaison. Il n'y a plus de F tiré d'un simple pic de pédale ; une zone sans point mesuré n'a pas de F.
- **Zones de freinage** : rouges, la tienne pleine, celle de la référence plus fine.
- **Écart écrit près de ton F** (« F 27 m trop tôt »), pour le virage choisi et, une fois zoomé, pour les virages prioritaires. Il est placé par le même algorithme anti-collision.
- **Virages prioritaires** : les 3 qui coûtent le plus (au-delà de 0,05 s) sont cerclés d'orange d'office, avec leur perte écrite (« +0,35 s »). La carte seule les désigne. Sur le tour de l'aperçu, seuls 2 virages dépassent 0,05 s.
- **Sélection** : choisir un virage dans la liste zoome la carte dessus.

### Fiche virage (`components/CornerSheet.tsx`)

- **Ouverture** : elle s'ouvre sous la carte quand un virage est choisi.
- **Contenu** :
  - la phrase chiffrée ;
  - les écarts avec leur unité ;
  - vitesse, frein et gaz des deux tours selon la distance ;
  - les phases (freinage, entrée, milieu, sortie) en bandes, avec leur perte ;
  - les points de freinage mesurés en traits verticaux.
- **Curseur partagé avec la carte** : survoler ou glisser place la voiture à cet endroit, et la relecture fait avancer le curseur. Les boutons précédent et suivant changent de virage.

### Mesures

- **Chevauchements** : 0 aux 3 tailles (Résumé, Données, 50 %, plein écran), étiquettes d'écart comprises.
- **Image médiane au zoom** : 16,7 ms. Le 90e centile monte à 42 ms en plein écran 2560 (33 ms avant).

Captures : `docs/audit/carte-0.58/`.

### À valider par Alex en piste

- La justesse des écarts de freinage sur un vrai tour : prends un repère de piste (panneau) et compare avec « F x m trop tôt ».
