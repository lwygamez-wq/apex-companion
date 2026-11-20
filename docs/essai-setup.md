# Ingénieur setup guidé (Setup Builder, étape 3)

20 nov. 2026. Module `analysis/setup_trial.rs`, carte « Essai guidé » de l'onglet Setups (sous le diagnostic de séance).

## Règles

- **Un seul essai à la fois**, tiré du constat le plus net et le plus répété du diagnostic sur un essieu entier (les deux roues dans le même sens, au moins 3 °C) :
  - écart intérieur / extérieur → carrossage de l'essieu (intérieur plus chaud : moins négatif ; extérieur plus chaud : plus négatif), 2 crans ;
  - écart milieu / bords → pression à froid de l'essieu (milieu plus chaud : plus basse ; bords plus chauds : plus haute), 1 cran.
  Le constat retenu est celui qui a le plus grand rapport écart / dispersion.
- **En crans, dans le sens donné par la valeur affichée par le jeu** (« vers 0° »). Le cran qui y mène dépend de la voiture : l'essai reste dans la marge disponible des deux côtés. Un réglage en butée est écarté (Monza : pression au cran 0, carrossage AR au cran 20 sur 21, donc 1 cran seulement).
- **Aucun gain promis.** Aucune comparaison à l'idéal de 92 °C : les essais portent sur des écarts entre zones du pneu.
- **Protocole** : 5 tours propres avec le réglage actuel, puis 5 avec le seul réglage changé, même circuit, même voiture.

## Verdict

- **Réglage fait ?** Le setup enregistré des deux séances est comparé : changé sur les deux roues dans le sens de l'essai, changé dans l'autre sens, ou inchangé.
- **Autres réglages changés** : listés, car l'effet peut leur être dû.
- **Effet** : la mesure de l'essieu avant et après. Il y a « effet mesuré » si l'écart dépasse 2 erreurs types de la différence et au moins 1 °C. L'erreur type est tirée de la dispersion entre tours (9e − 1er décile ≈ 2,56 écarts types).
- **Temps au tour médian** avant et après, à titre indicatif.

## Essai sur le corpus (Monza, Cadillac, 18 nov.)

- Proposition d'après la course de 20:10 : « Carrossage AR moins négatif, 1 cran sur les deux roues (aujourd'hui −1.0 deg / −1.0 deg) ». Raison : intérieur plus chaud que l'extérieur de 7,7 °C en appui, sur 30 tours (±0,5).
- Verdict contre la course de 21:39 :
  - carrossage AR inchangé ;
  - écart 7,7 → 7,8 °C, pas d'effet mesurable (seuil 1,0 °C) ;
  - 9 autres réglages changés, listés (aileron arrière P6 → P4, carrossage AV −1,0 → −1,1°, antiroulis AV, différentiel…).

C'est le cas que l'essai guidé veut éviter : beaucoup de réglages changés à la fois, aucun effet attribuable.

Non vérifié en jeu : aucun essai complet (avant / après sur un seul réglage) n'a encore été roulé.
