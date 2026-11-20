# Le plein juste : rejeu de l'ingénieur de course (LMU)

20 nov. 2026. Outil : `cargo run --release -p apex-core --example engineer_replay -- course.apexlmu` (module `apex_core::engineer_replay`). L'enregistrement est rejoué comme en direct (mêmes échantillons, même `LiveCoach`, même `Engineer`) ; chaque consigne est comparée à ce qui s'est passé ensuite. Le même rejeu tourne dans `real_corpus` : il fait échouer la CI si, en suivant l'annonce, l'arrivée sort de 0,3 à 0,8 tour.

## Courses rejouables

Il faut la fin de chrono enregistrée (`LapEvent.time_remain_s`, Apex 0.38 et plus) :

| Course du corpus | Version | Rejouable |
|---|---|---|
| Imola 11 nov., 12 nov. ×2 | avant 0.38 | non : pas de fin de chrono |
| Bahreïn 12 nov. | avant 0.38 | non : pas de fin de chrono |
| Monza 18 nov. 20:10 et 21:39 | 0.39 | **oui** |
| Monza 20 nov. 19:37 (essais) et 20:09 (course, 0.41) | 0.41 | **pas dans le corpus** : fichiers et journal sur le PC d'Alex |

Le MFD du jeu n'est pas enregistré : rejoué sans. L'écart au leader n'est enregistré qu'aux passages de ligne.

## Arrêt de chaque course : annoncé contre nécessaire

« Nécessaire » = distance réelle depuis l'entrée des stands × consommation réelle après l'arrêt + une demi-tour de marge. « Arrivée » = énergie restante à l'arrivée en réglant le niveau annoncé, en tours (cible 0,5).

| Course | Réglé par Alex | Arrivée réelle | Annonce 0.42 | Arrivée 0.42 | Annonce 0.43 | Arrivée 0.43 | Sans temps d'arrêt connu |
|---|---|---|---|---|---|---|---|
| Monza 18 nov. 20:10 | 64 % | 0,64 tour | 63 % | 0,41 | **64 %** | **0,64** | 68 % → 1,56 tour |
| Monza 18 nov. 21:39 | 63 % | 1,30 tour | 58 % | 0,16 | **59 %** | **0,39** | 63 % → 1,30 tour |
| Monza 20 nov. 20:09 (relevé d'Alex) | 63 % | 7,2 % = 1,7 tour | ? | ? | ? | ? | non rejouable ici |

## Part de chaque cause (entrée des stands, 0.42)

| Cause | Monza 20:10 | Monza 21:39 |
|---|---|---|
| Distance restante (chrono → tours, temps d'arrêt décompté) | 0,00 tour (14,015 estimés, 14,015 réels) | 0,00 tour (13,015 / 13,015) |
| Consommation retenue (moyenne des 3 derniers tours) contre réelle | 4,32 contre 4,37 % : −0,7 % | 4,27 contre 4,40 % : −1,7 % |
| Arrondi au % supérieur | +0,4 % | +0,4 % |
| Position de l'arrêt / ligne | arrêt 0,5 % après la ligne : ≤ 0,05 tour | arrêt 1,1 % après la ligne : ≤ 0,05 tour |
| Temps d'arrêt inconnu (circuit sans historique) | +1 tour fantôme = +4,3 % | +1 tour fantôme = +4,3 % |

- **Temps d'arrêt** : de 30 à 95 s, l'annonce ne change pas sur ces deux courses ; seul « aucun temps connu » (0 s) crée le tour de trop. C'est exactement ce que réglait Alex le 18 nov. à 21:39 (63 %, 1,3 tour de trop). L'app retient le temps mesuré sur le circuit (Monza : 56 s), sinon la médiane des autres circuits (Imola : 64 à 95 s, Bahreïn : 46 s).
- **Consommation** : les 3 à 4 derniers tours de course consomment 4,45 à 4,71 %/tour à Monza (contre ≈ 4,3), la moyenne des 3 tours avant l'arrêt sous-estime donc la fin de course. Correctif 0.43 : pour le plein seulement, la plus haute de la moyenne récente et de la médiane des tours de course (cible et lift inchangés).
- **Position de l'arrêt** (mesurée) : Imola 0,95 à 0,97 du tour (avant la ligne), Monza 0,005 à 0,011 et Bahreïn 0,028 (après la ligne). Voie des stands : Monza 53 à 54 s dont 17,6 à 18 s à l'arrêt ; Bahreïn 43 s dont 10,2 s ; Imola 62 à 75 s dont 18 à 27 s. Avec la distance exacte à l'entrée des stands, l'effet de la position est ≤ 0,05 tour : pas une cause.

## Le pilote reste en piste après « box »

Les deux courses : « Box, box, ce tour. Règle l'énergie à 72 % » au tour 19, arrêt au tour 22. Le niveau recalculé (63 puis 62 %) n'était pas redit à la radio : régler le dernier niveau entendu fait sur-remplir. Correctif 0.43 : tant que le pilote reste en piste, l'ingénieur redit le niveau quand il change d'un point (« Toujours en piste : règle l'énergie à 63 % pour 14 tours après l'arrêt. »). La consigne de l'entrée des stands reste celle qui fait foi.

Hypothèse pour le 20 nov. (non vérifiable ici) : 63 % réglés pour 58 % nécessaires, arrêt fin du tour 22, c'est le niveau d'une consigne de ligne pour un arrêt un tour plus tôt, ou un réglage sans le temps d'arrêt. Pour trancher : le `.apexlmu` de 20:09 et le journal d'Apex (lignes `coach :` et `jeu : MFD énergie`).

## Tours restants aux passages de ligne

Distance exacte estimée par l'ingénieur contre réelle, à chaque passage (temps d'arrêt connu) :

| Course | Passages | Écart d'un tour (ou plus) | Où |
|---|---|---|---|
| Monza 20:10 | 34 | 9 | tours 1 à 6 (−1 à −3 : temps au tour pris sur les premiers tours), puis −1 aux tours 10 à 14, 18 et 20 ; 0 après l'arrêt |
| Monza 21:39 | 35 | 9 | +1 ou −1 aux tours 0 à 6, 10 à 14 et 18 à 19 ; 0 après l'arrêt |

À l'entrée des stands, l'estimation est exacte sur les deux courses. L'écart d'un tour aux passages de ligne vient de la conversion chrono → tours près de la limite (passage du leader juste avant ou après la fin du chrono). Le rejeu ne connaît l'écart au leader qu'aux passages de ligne, donc il peut exagérer cet écart. Le banc ne sait pas trancher : à vérifier sur un rejeu avec écart au leader continu (enregistrement à ajouter).

## Ce qui manque

- La course du 20 nov. (20:09), les essais de 19:37 et le journal d'Apex de ce soir : à déposer dans le corpus privé (`corpus_prepare`) et à côté, pour rejouer et vérifier si Alex a réglé selon Apex ou selon le MFD.
- Réponse brute de `GET /rest/strategy/pitstop-estimate` (déjà enregistrée par la sonde dans `sonde-lmu/<séance>/strategy-pitstop.json` sur le PC d'Alex) : nécessaire pour l'utiliser sur un circuit sans historique sans deviner son format.
