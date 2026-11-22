# Ingénieur setup guidé (Setup Builder, étapes 3 et 4)

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

## Mémoire d'essais et setup recommandé (0.50)

Module `analysis/setup_memory.rs`. Il s'affiche dans l'onglet Setups, sous l'essai guidé, pour la voiture et le circuit de la séance choisie. Tout est relu dans les enregistrements `.apexlmu` : rien n'est stocké en base, et aucun setup acheté n'est lu ni copié.

- **Mémoire** : pour chaque paire de séances consécutives avec setup enregistré, Apex liste les réglages changés (valeurs du jeu et crans) et l'effet mesuré sur les quatre métriques d'essieu : intérieur − extérieur et milieu − bords, à l'avant et à l'arrière. Le seuil est le même que pour l'essai guidé : 2 erreurs types et 1 °C.
  - **Effet mesuré** : un seul réglage changé (une roue, ou les deux roues d'un essieu), avec un effet au-delà du seuil dans le sens attendu (carrossage moins négatif : intérieur moins chaud ; pression plus basse : milieu moins chaud).
  - **Non concluant** : un seul réglage changé, pas d'effet au-delà de la dispersion.
  - **Non attribuable** : plusieurs réglages changés à la fois. Les écarts sont notés, jamais attribués.
- **File d'essais** : les constats du dernier diagnostic, classés par netteté (écart / dispersion) × répétition (séances où le même constat revient, même essieu, même sens, au moins 3 °C). Un réglage en butée est écarté avec la raison. Un essai déjà fait sans effet passe en dernier, avec la mention « déjà essayé ». Apex propose toujours **un seul essai à la fois**. Il vérifie dans la séance suivante si le réglage a été fait et si d'autres ont changé.
- **Setup recommandé** : le setup de la dernière séance, plus les seuls changements « effet mesuré » encore présents, chacun avec sa preuve (séances, mesures). Sans changement validé, c'est le setup actuel, et l'écran le dit. Ce n'est jamais un optimum garanti.
- **Consommation par setup** : énergie médiane par tour de chaque séance, avec le réglage d'aileron. Elle sert à la stratégie quand le setup change (Monza 18 nov. : aileron P6 4,36 %/tour, P4 4,32 %/tour, sur 30 tours chacune ; écart dans la dispersion).

Sur le corpus (Monza 18 nov.) :
- la paire de courses est **non attribuable** (9 réglages changés) ;
- la file propose « Carrossage AR vers 0°, 1 cran », puis « Carrossage AV vers 0°, 2 crans », constat répété sur 2 séances sur 2 ;
- les pressions sont écartées (cran 0 sur 0–36) ;
- setup recommandé : le setup actuel, aucun changement validé.

Règle vérifiée par `real_corpus` : un changement recommandé vient toujours d'un essai à un seul réglage avec effet mesuré.

Comparateur `.svm` (onglet Setups, depuis la 0.36) : lecture locale des deux fichiers choisis, réglage par réglage, valeurs du jeu, crans et sens physique de l'écart. Les fichiers ne sont ni copiés ni stockés. Un setup acheté peut y être comparé sur le PC d'Alex, sans jamais quitter son dossier.

### Procédure d'essai à faire sur le PC d'Alex

1. Séance d'essais, setup actuel : 5 tours propres au moins.
2. Rentrer au garage, changer **uniquement** le réglage en tête de la file (ex. carrossage AR, 1 cran vers 0°), ressortir : 5 tours propres au moins, même circuit, même voiture.
3. Onglet Setups, choisir la 2e séance : la mémoire doit afficher la paire avec « Effet mesuré » ou « Non concluant », et le setup recommandé doit en tenir compte.
