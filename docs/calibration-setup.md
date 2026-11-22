# Calibration du diagnostic setup (Setup Builder, étape 2)

20 nov. 2026. Outil : `cargo run --release -p apex-core --example setup_calib -- *.apexlmu` sur le corpus (`lwygamez-wq/apex-corpus`). Toutes les valeurs ci-dessous sont des médianes sur les échantillons en piste (hors stand, > 72 km/h). « En appui » : accélération latérale > 12 m/s² ; « ligne droite » : < 2 m/s² et pleine charge.

## 1. Ce qui est enregistré

| Séances du corpus | Pneus 3 zones, carcasse, pressions, freins (5 Hz) | Hauteur par roue (60 Hz) | Setup de la séance | Rotation des roues, accélérations, couche interne |
|---|---|---|---|---|
| Imola 11–12 nov. (4 séances : Peugeot 9X8 ×3, Alpine A424) | non (avant 0.36) | non | non | non |
| Bahreïn 12 nov., course (Cadillac V-Series.R) | oui | oui | oui (170 réglages) | non |
| Monza 18 nov., 2 courses (Cadillac V-Series.R) | oui | oui | oui | non |

Rotation des roues, vitesse du sol par roue, accélérations et couche interne des pneus : enregistrées à partir d'Apex 0.41 (extension `EXT3` du fichier). Les blocages ne sont donc mesurables que sur les séances à venir.

Idéal annoncé par le jeu (`optimalCompoundConditions`) : Medium 92 °C, Hard 99 °C, Wet 51,9 °C.

## 2. Surface ou carcasse ?

| Séance (piste) | Zones G/M/D AvG, appui | idem, ligne droite | Carcasse AvG, appui | Carcasse AvG, ligne droite | Carcasse, tours 0 à 5 |
|---|---|---|---|---|---|
| Monza 1 (14,7 °C) | 51,5 / 58,2 / 54,8 | 45,2 / 50,7 / 48,4 | 78,2 | 78,0 | 23 → 41 → 52 → 61 → 68 → 72 |
| Monza 2 (17,7 °C) | 51,1 / 57,7 / 54,7 | 44,9 / 50,4 / 48,3 | 76,9 | 76,7 | 51 → 61 → 67 → 71 → 73 → 75 |
| Bahreïn (41,4 °C) | 60,3 / 67,8 / 65,3 | 53,6 / 59,2 / 57,4 | 83,8 | 83,7 | 37 → 51 → 61 → 69 → 74 → 78 |

Constats :
- Les trois zones (`mTemperature`) gagnent 5 à 8 °C en appui par rapport à la ligne droite : elles réagissent en moins d'une seconde. C'est le comportement d'une couche de surface.
- La carcasse (`mTireCarcassTemperature`) est identique en appui et en ligne droite (à 0,2 °C près) et monte sur 5 à 8 tours : c'est une masse lente.
- Les trois zones restent 20 à 30 °C sous la carcasse, sur toutes les roues et les trois séances.
- Les deux suivent la piste : Bahreïn +6 à +9 °C par rapport à Monza.

Conclusion : les trois zones mesurent la surface de la bande de roulement, la carcasse mesure la carcasse. Certitude élevée sur ce point (comportement thermique net et répété). En revanche, **on ne sait pas laquelle de ces grandeurs le jeu compare à son idéal de 92 °C** : la carcasse (67 à 84 °C) en est plus proche que la surface (41 à 68 °C), mais rien ne le prouve. Certitude faible.

Décision : le diagnostic affiche les températures telles que lues, **sans aucune comparaison à l'idéal**, et le dit à l'écran. Relevé du HUD fait le 20 nov. (ci-dessous).

### Relevé du HUD du jeu (20 nov. 2026, Monza, Cadillac V-Series.R)

Alex a photographié le HUD pneus du jeu à 0:06:37 de temps restant de la course de 20 h 09, soit 3330,4 s dans le `.apexlmu` (19 s avant la ligne du tour 31). L'instant est confirmé : pressions et freins du HUD identiques à Apex à 0,1 kPa / 0,1 °C près, usure (75 / 81 / 73 / 76 %) et énergie (25 %) concordantes. Enregistrement 0.41 : la couche interne est disponible.

| °C | AvG | AvD | ArG | ArD |
|---|---|---|---|---|
| **HUD du jeu** | **68,8** | **62,1** | **71,0** | **66,7** |
| Surface (moyenne des 3 zones) | 46,4 | 48,5 | 48,5 | 48,2 |
| Couche interne (moyenne des 3 zones) | 61,6 | 56,1 | 63,2 | 59,8 |
| Carcasse | 77,6 | 68,3 | 80,2 | 74,5 |
| (carcasse + interne) / 2 | 69,6 | 62,2 | 71,7 | 67,2 |
| Écart au HUD | +0,8 | +0,1 | +0,7 | +0,5 |

- Surface exclue : 15 à 22 °C sous le HUD. Carcasse seule : 6 à 9 °C au-dessus. Couche interne seule : 6 à 8 °C en dessous.
- Premier relevé (fin des essais de 19 h 37, roue arrêtée, heure exacte inconnue) : même formule, 0,7 à 1,4 °C d'écart, même sens.
- Le tableau de bord SimHub affiche 70 / 62 / 72 / 67 : même échelle que le HUD.

Conclusion : la température affichée par le jeu est **estimée** par (carcasse + moyenne des 3 zones de la couche interne) / 2, à 1,4 °C près sur les deux relevés. C'est une hypothèse, pas une certitude : 4 roues à un seul instant (plus un relevé moins bien daté), et la carcasse et la couche interne varient ensemble, donc d'autres pondérations colleraient presque aussi bien. On ne sait toujours pas si le jeu compare cette valeur à son idéal de 92 °C.

Décision (0.42) : le diagnostic ajoute une colonne « Échelle du jeu (estimée) » par roue, avec la mention « estimation d'après 2 relevés du HUD, à confirmer ». Surface, carcasse et couche interne restent affichées telles que lues. **Aucune comparaison à 92 °C**, ni de la surface ni de l'estimation, tant qu'un 3e relevé n'a pas confirmé la formule (de préférence à un autre moment de la séance, pneus plus froids ou plus chauds, pour séparer carcasse et couche interne).

Contrôle du sens des zones (gauche = extérieur pour les roues gauches) : à Monza (virages surtout à droite), les pneus gauches sont plus chauds que les droits (carcasse AvG 78 °C contre AvD 69 °C), et l'intérieur est plus chaud que l'extérieur sur les quatre roues avec un carrossage négatif. C'est cohérent avec l'ordre gauche / milieu / droite du jeu.

## 3. Hauteur de caisse et talonnage

| Séance | Roue | Médiane | 1er centile | Mini absolu | Échantillons < 5 mm |
|---|---|---|---|---|---|
| Monza 1 | ArD | 37,5 mm | 22,9 mm | −3,3 mm | 12 sur bitume (sur 139 261), 5 sur vibreur |
| Monza 2 | ArG | 33,5 mm | 19,6 mm | −3,4 mm | 6 sur bitume (sur 136 656) |
| Bahreïn | ArD | 40,0 mm | 24,2 mm | 1,7 mm | 2 sur bitume (sur 135 843) |

- Les valeurs sous 5 mm sont des pointes isolées : au plus 17 échantillons à 60 Hz par course (0,3 s cumulées), surtout sur le bitume, pas sur les vibreurs. On ne voit pas de seuil de talonnage répété.
- Le jeu ne donne aucun signal de contact du fond plat : **le seuil de talonnage n'est pas mesurable** avec ces données.
- Décision : pas de mot « talonnage ». Le diagnostic donne le point bas robuste (1er centile de chaque tour, médiane sur les tours) et l'endroit où il se répète. À Monza, il est en ligne droite (appui aéro à pleine vitesse) ; à Bahreïn, au Virage 1 (freinage). Les pointes sous 5 mm sont comptées, pas interprétées.

## 4. Même voiture, setups différents

Cadillac V-Series.R à Monza, 18 nov. (deux courses, même circuit) :

| Réglage | Course 1 (20:10) | Course 2 (21:39) |
|---|---|---|
| Aileron arrière | P6 | P4 |
| Carrossage AV | −1,0° | −1,1° |
| Pression de freinage | 94 % | 93 % |
| Piste | 14,7 °C | 17,7 °C |

Effets attendus, mesurés :
- Carrossage AV plus négatif → intérieur plus chaud : AvG +2,4 → +2,7 °C, AvD +5,4 → +5,6 °C. Sens attendu, mais l'écart (0,2 à 0,3 °C) est plus petit que la dispersion d'un tour à l'autre (±0,4 à ±0,6 °C).
- Moins d'aileron arrière → voiture un peu plus haute à pleine vitesse : médianes +0,2 à +0,5 mm, point bas ArD 23,2 → 23,6 mm. Sens attendu, écart dans le bruit.

La validation du sens des mesures est donc **incomplète** : deux séances seulement, des changements de réglage très petits, et une piste 3 °C plus chaude qui se mélange aux effets. Pour la compléter : deux séances même voiture et même circuit, avec un seul réglage nettement changé (par exemple 3 crans de carrossage AV).

## 5. Équilibre (méthode)

Rapport de rotation = lacet / (vitesse × angle du volant), comparé à la même grandeur dans les courbes prises en douceur (2 à 6 m/s²) dans la même tranche de 50 km/h. Le signe volant/lacet est calé sur la séance. Sous 0,85 sur au moins 80 % des tours : sous-vire. Au-dessus de 1,15 : survire. Sinon : pas d'écart mesurable. Le sens est fiable, l'ampleur ne se compare qu'entre virages d'une même séance (elle dépend de la voiture, du rapport de direction et de la vitesse).

Sur le corpus : le milieu des virages lents sort souvent en sous-virage (Imola, 3 voitures : 0,51 à 0,74), et les sorties de l'Alpine en survirage (1,36 à 1,75). Ces chiffres ne sont pas vérifiés en jeu.

## 6. Ce que l'API garage remplit (sonde du 22 nov., lecture seule)

Mesuré sur le setup enregistré dans l'en-tête des `.apexlmu` du corpus (lu par `GET` sur l'API locale au début de chaque séance, depuis Apex 0.36). Aucune écriture dans l'API.

| Séances | Setup enregistré | Réglages | Avec crans mini et maxi | Avec valeur affichée | Setups fournis par le jeu (`carPresetSetups`) | Température idéale par gomme (`optimalCompoundConditions`) |
|---|---|---|---|---|---|---|
| Imola 11–12 nov. (3 courses, 1 essai) | non (avant 0.36) | — | — | — | — | — |
| Bahreïn 12 nov. (Cadillac) | oui | 170 | 170 (100 %) | 170 (100 %) | 3 | Medium 92, Hard 99, Wet 51,9 °C |
| Monza 18 nov. ×2 (Cadillac) | oui | 170 | 170 (100 %) | 170 (100 %) | 3 | idem |

- Les 170 réglages couvrent 40 familles : aéro, suspension (`WM_CAMBER`, `WM_PRESSURE`, `WM_RIDEHEIGHT`, amortisseurs, butées), freins, différentiel, électronique (TC, ABS, régénération), rapports.
- 117 à 124 réglages sont en butée (cran mini ou maxi). La plupart sont des réglages à deux positions ou laissés à 0 (cartes, options). Le diagnostic n'en tient compte que pour carrossage et pression : la pression à froid est au cran 0 sur 0–36 sur les trois courses, donc aucun essai de pression n'est possible dans le sens « plus basse ».
- La réponse complète de `getPlayerGarageData` (champs hors setup) reste sur le PC d'Alex dans `sonde-lmu/<séance>.json` (`GarageSummary`). Elle n'est pas dans le corpus, donc ses autres champs ne sont pas chiffrés ici.
- `mOptimalTemp` de la mémoire partagée n'est pas enregistré. La température idéale vient de l'API (`optimalCompoundConditions`). Elle n'est **jamais comparée** aux pneus (section 2).
