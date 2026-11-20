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

Décision : le diagnostic affiche les températures telles que lues, **sans aucune comparaison à l'idéal**, et le dit à l'écran. Pour trancher : relever en jeu l'affichage des températures du HUD de LMU en même temps qu'un enregistrement 0.41 (qui ajoute la couche interne).

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
