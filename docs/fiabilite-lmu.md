# Fiabilité des lectures LMU (chantier 4, 20 nov. 2026)

Vérifié sur le corpus (7 séances LMU, 11 au 18 nov.). La course du 20 nov. n'est pas dans le corpus : ses cas (tour 34 non chronométré, pic d'usure avant droit au tour 6) sont traités par les mêmes règles, à vérifier dès qu'elle y sera versée.

## Limites de piste

| Séance | Avertissements (`mTrackLimitsSteps`) | Tours invalidés (`mLapInvalidated`) | Tours non chronométrés par le jeu |
|---|---|---|---|
| Imola, essais 12 nov. | 0 (jamais) | 8 sur 10 | 9 |
| Monza 18 nov. 20:10 | 0 → 1 au tour 36 | 0 | tours 29 et 36 |
| Monza 18 nov. 21:39 | 0 → 2 au tour 2, → 3 au tour 15 | 0 | tours 2 et 15 |
| Bahreïn 12 nov. | 0 → 2 au tour 0, → 3 au tour 3 | 0 | tours 0 et 3 |

- **En course**, le compteur fonctionne : chaque nouvel avertissement tombe dans un tour que le jeu ne chronomètre pas (`lap_time −1`), toujours le même tour. Le drapeau « tour invalidé » reste à 0.
- **En essais**, le jeu invalide les tours (`mLapInvalidated`) sans faire avancer le compteur. Le « compteur resté à 0 » du 12 nov. venait de là : ce n'était pas une mauvaise lecture.
- **Correctif 0.44** : un tour que le jeu n'a pas chronométré est marqué invalide (source : le jeu). Il ne sert plus de référence ni dans la régularité. Sa consommation et son temps mesuré restent comptés : la stratégie et le débrief filtrent sur les drapeaux (stand, sortie, incident, jaune, coupure), pas sur la validité. Effet mesuré sur le corpus : stratégie, débrief et rejeu de l'ingénieur inchangés ; temps d'arrêt appris à Monza 55,986 → 55,975 s.

## Tour non chronométré (`lap_time −1`)

Le temps mesuré par Apex est gardé, la consommation par tour vient de l'énergie à 60 Hz (pas du temps officiel). Il ne fausse ni la consommation ni le débrief : vérifié sur les 6 tours non chronométrés du corpus, aucun changement de stratégie, de débrief ou de rejeu après le correctif.

## Pics d'usure

Nouvel événement du débrief « Usure anormale » : usure d'un pneu sur un tour d'au moins 3 fois sa médiane de course et de plus d'un point, avec l'endroit (plus forte baisse sur une seconde) et ce qui a été mesuré sur ce tour.

| Séance | Événement | Mesuré sur ce tour |
|---|---|---|
| Imola 12 nov. 12:16 | AvG 4,50 % au tour 51 (médiane 0,72), Tosa | sortie de piste |
| Imola 12 nov. 16:17 | AvG, ArG, ArD 2,5 à 3,8 % au tour 8, Tamburello ; ArD 3,25 % au tour 39, Rivazza ; ArG 2,41 % au tour 42 | sortie de piste et contact |
| Monza 18 nov. 20:10 | AvD 2,66 % au tour 22 (entrée des stands) ; AvG 4,16 % et ArG 2,95 % au tour 34, Rettifilo | contact ; sortie de piste |
| Monza 18 nov. 21:39 | ArG 4,53 % au tour 15 (médiane 0,84), Lesmo 2 | sortie de piste et contact |
| Bahreïn 12 nov. | AvD 3,23 % au tour 0, virage 14 | contact |

Chaque pic du corpus coïncide avec une sortie de piste ou un contact enregistrés sur ce tour. Un blocage de roue n'est mesurable qu'à partir des enregistrements 0.41 (rotation des roues, extension `EXT3`). Le pic AvD du 20 nov. (3,5 % au tour 6) sera expliqué par la même règle quand la course sera dans le corpus.
