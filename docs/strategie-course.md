# Stratégie de course comparée (0.49)

Avant la course, l'écran **Stratégie** compare les stratégies possibles (carte « Options de course » sous le plan). En course, l'ingénieur annonce la recommandée une fois, au premier passage de la ligne (« Plan : un arrêt, arrêt entre les tours 14 et 22. 30 secondes de moins au stand que l'option suivante. »). Ensuite, il garde la règle de la 0.47 à chaque tour : il n'ajoute pas d'arrêt si 8 % d'économie ou moins suffisent, ne dit rien dans les deux derniers tours et borne la cible à 8 %.

Code : `analysis/race_options.rs` (calcul pur, testé), `service::stop_samples` (arrêts mesurés), `RaceStrategyView.options`, `components/RaceOptionsCard.tsx`, `LiveCoach::set_race_plan`.

## Ce qui est mesuré, et comment

| Donnée | Source | Valeur sur le corpus |
|---|---|---|
| Consommation par tour et dispersion | tes tours de course propres (même filtre que le calculateur) | 2,8 à 4,4 %/tour, σ 0,04 à 0,12 |
| Temps de service | chaque arrêt enregistré dans le `.apexlmu` : temps à l'arrêt contre énergie remise (moindres carrés par l'origine) | **0,41 s par % remis** (5 arrêts : +35 % en 14,8 s … +94,5 % en 38,9 s) |
| Voie des stands hors service | tour de rentrée + tour de sortie − 2 tours normaux − temps à l'arrêt, médiane sur ce circuit | Monza 27–28 s, Bahreïn 28 s, Imola 36 s |
| Usure des pneus | `mWear` par tour, pneu le plus usé | 0,4 à 0,6 %/tour |

Le temps de service ne dépend pas du circuit : il est mesuré sur tous les arrêts. La voie des stands, si : sans arrêt mesuré sur ce circuit, rien n'est classé.

## Ce qui n'est pas mesuré (dit à l'écran, jamais deviné)

- **Coût du lift and coast** : sur les 6 courses, l'énergie par tour varie de ±0,04 à ±0,12 % seulement. La pente « temps au tour contre énergie » va de −5,7 à +3,1 s par %, toujours dans son incertitude (±1 à ±2,4) : elle n'est pas mesurable. Une option avec économie affiche donc son **seuil de rentabilité** : le temps au stand gagné, réparti sur les tours (« rentable si le lift and coast coûte moins de 1,6 s par tour »). Elle n'est jamais classée la plus rapide.
- **Changement de pneus** : aucun arrêt enregistré n'en a changé (usure identique avant et après), donc ni son temps ni le fait qu'il soit simultané au plein ne sont connus. L'usure restante projetée est affichée, sans seuil inventé.
- **API du jeu** (`/rest/strategy/pitstop-estimate`, `/rest/garage/UIScreen/RepairAndRefuel`) : réponses brutes absentes du corpus, donc non utilisées. À recouper quand les fichiers `sonde-lmu/<séance>/` seront versés.
- **Incidents** (voiture de sécurité, drapeaux) : non prévus, c'est écrit sous le tableau.

## Candidats et verdict

- De (arrêts minimum − 1) à (minimum + 1), 3 arrêts au plus. Une option avec économie n'est gardée que si l'économie tient dans 8 % (`SAVE_MAX`).
- Pour chaque option : tours (course au temps : moins de tours si on passe plus de temps au stand), fenêtre du premier arrêt (au plus tôt, au plus tard), niveau à régler à chaque arrêt (même règle `pit_fill` que l'ingénieur, arrêt au plus tard), temps au stand = arrêts × voie + 0,41 s × énergie remise, marge d'arrivée (0,5 tour, dans la plage 0,3–0,8), usure projetée.
- Incertitude de la différence entre deux options : écart type de la voie × √(écart d'arrêts), au moins 1 s. Il faut aussi au moins 2 s d'écart type par arrêt (peu d'arrêts mesurés).
- **Verdict** : « la plus rapide » seulement si son avance dépasse deux écarts types. Sinon « équivalentes », et la plus sûre (le plus d'arrêts, sans économie) est désignée. Sans arrêt mesuré : pas de classement.

## Banc sur le corpus (`real_corpus`, champ `options`)

Chaque course LMU est rejouée sur sa distance réelle. On compare la recommandation à ce qui a été fait :

| Course | Fait | Recommandé | Arrivée réelle | Gain estimé en suivant la recommandation |
|---|---|---|---|---|
| Imola 11 nov. (Peugeot, avant l'énergie) | 1 arrêt | 1 arrêt, T26–30 | non enregistrée | — |
| Imola 12 nov. 12:16 (Peugeot) | 1 arrêt, 38,9 s à l'arrêt | 1 arrêt, T27–30 | 4,53 tours d'énergie en trop | **5,3 s** (4,0 tours de trop × 3,17 % × 0,41 s) |
| Imola 12 nov. 16:17 (Alpine) | 1 arrêt, 26,6 s | 1 arrêt, T22–34 | 2,38 tours | **2,2 s** |
| Monza 18 nov. 20:10 | 1 arrêt, 26,1 s | 1 arrêt (63 % au T22) | 0,64 tour | 0,2 s |
| Monza 18 nov. 21:39 | 1 arrêt, 25,6 s | 1 arrêt | 1,32 tour | **1,5 s** |
| Bahreïn 12 nov. | 1 arrêt, 14,8 s | 1 arrêt (39 % au T23, 38 % réglés) | 0,51 tour | 0 s |

Le bon nombre d'arrêts a été fait partout. Le temps à gagner est dans le **plein en trop**, que les correctifs 0.43 à 0.47 ont déjà réduit (Monza, Bahreïn). Ces gains sont des estimations (temps de service mesuré × énergie en trop), pas des promesses. Ils ne tiennent compte ni du poids de l'énergie en trop ni des positions.

Règles vérifiées par `real_corpus` : aucune option avec économie n'est recommandée. L'ingénieur ne dit plus « Box » ni « Changement de plan » après le dernier arrêt (0.47). L'arrivée en suivant l'ingénieur reste dans 0,3–0,8 tour sur les courses de Monza.

## À valider sur le PC d'Alex

1. Ouvrir Stratégie avec la voiture chargée dans LMU (course au temps) : la carte « Options de course » doit afficher le verdict, la fenêtre et les niveaux.
2. En course, au premier passage de la ligne : l'annonce « Plan : … » suivie de la première consigne de l'ingénieur (journal : ligne `coach :`).
3. Pour mesurer le coût du lift and coast : en essais, 3 tours normaux, 3 tours avec lever de pied avant les gros freinages, 3 tours normaux, puis verser la séance au corpus.
