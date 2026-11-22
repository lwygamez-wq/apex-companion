# Bibliothèque de références (0.48)

Pour chaque combinaison **(simulateur, circuit, configuration, voiture, série fixed/open, conditions sec/humide/inconnues)**, Apex élit automatiquement ton meilleur tour propre et complet. Il n'y a rien à faire : l'élection a lieu à chaque import (y compris l'import automatique des `.apexlmu` et `.ibt`) et au démarrage, pour rattraper toute la base (`AppService::refresh_references`). Le code est dans `analysis/references.rs` (règles), `service.rs` (élection) et `storage.rs` (table `auto_references`, migration v12).

## Ce qui est lu

- Les séances déjà en base (tes séances seulement : un tour d'un autre pilote, Garage 61 ou autre, n'est jamais élu).
- Pour chaque tour candidat : drapeaux et validité du tour, et sa télémétrie (distance et temps).

Aucune source extérieure n'est consultée et rien n'est téléchargé.

## Ce qui est élu

Un tour devient la référence seulement s'il remplit **toutes** ces conditions :

| Condition | Seuil |
|---|---|
| Tour lancé chronométré | `kind = flying`, temps connu |
| Valide | pas invalidé par le jeu (limites de piste) |
| Non perturbé | pas de sortie, incident, drapeau jaune, passage au stand ni coupure |
| Télémétrie complète | ≥ 97 % de la longueur du circuit, aucun trou de plus de 60 m |
| Chrono cohérent | fin de la trace à ±max(1 s, 2 %) du temps officiel |
| Pas aberrant | pas plus rapide que 93 % de la médiane des tours propres de la séance (au moins 3 tours) |

La référence en place n'est remplacée que par un tour **plus rapide** et fiable : un tour incomplet ne chasse jamais une bonne référence. Chaque élection est gardée (date, séance, numéro du tour, temps, raison du remplacement : « battue de 0,768 s »). Si la séance de la référence est supprimée ou recalculée, la référence est retirée et le meilleur tour restant est élu à nouveau.

Le tour est repéré par (séance, numéro de tour) et non par son identifiant, qui change quand une séance est recalculée par un nouveau décodeur.

## Où elle sert

1. **Analyse d'une séance** : ta référence choisie à la main (ou importée) d'abord, puis la référence élue, sinon le meilleur tour de la séance. Les virages, phases, conseils, sorties de piste et carte de risque partent de cette référence, même quand elle vient d'une autre séance (correctif 0.48 : avant, la trace d'une référence d'une autre séance n'était pas chargée et les noms de virages disparaissaient).
2. **Coach en direct** : même ordre ; le coach annonce « ta référence auto, 1 minute 32 point 9 ».
3. **Onglet Références** : liste des combinaisons, temps, date et tour de la séance, gain sur la version précédente, bouton pour ouvrir la séance.

## Résultat sur le corpus (13 séances)

9 références élues, identiques d'une relecture complète à l'autre, aucune tirée d'un tour non propre (règles vérifiées par `real_corpus`). Interlagos FR04 n'en a pas : aucun tour propre dans la séance. Les séances d'Imola (Peugeot, 11 et 12 nov.) et la 2e course de Monza (18 nov., 21:39) sont désormais analysées contre la meilleure référence du circuit (1:32.921 à Imola, 1:37.058 à Monza) au lieu de leur propre meilleur tour.

## Référence de catégorie (0.45, EXT4)

Pas affichée : `rival_probe` n'a pas encore été lancé sur une vraie course enregistrée en 0.45 ou plus. Tant que la sonde n'a pas dit « OK », Apex ne compare pas au meilleur tour de la voiture de référence et l'écran le dit.

## Télémétrie écrite par LMU lui-même (sonde)

D'après des outils communautaires (non vérifié sur le PC d'Alex), LMU écrit depuis sa version 1.2 un fichier **DuckDB** par séance dans `Le Mans Ultimate\UserData\Telemetry` quand l'enregistrement est activé (option « Automatically Record Telemetry » ou touche dédiée) : une table par canal (avec `value` ou `value1..4` par roue), des tables de catalogue (`metadata`, `channelsList`, `eventsList`) et le setup complet en JSON dans `metadata`. Les mêmes sources disent que plusieurs canaux manquent (lacet, accélérations latérale et longitudinale, angle de braquage, aéro) et que deux canaux sont mal nommés.

Apex 0.48 ne lit pas encore ces fichiers. Au démarrage, il fait un inventaire en lecture seule (`connectors/game_telemetry.rs`) et l'écrit dans `sonde-lmu/telemetrie-jeu.json` :
- les fichiers du dossier `Telemetry` (nom, taille, date, format reconnu à l'en-tête : « DUCK » à l'octet 8 pour DuckDB) ;
- les réglages des JSON de `UserData\player` dont le nom contient « telemetry » ;
- un verdict (journal d'Apex, ligne « sonde télémétrie du jeu »).

Aucun contenu n'est ouvert ni copié. Si la sonde trouve des fichiers DuckDB, l'étape suivante est de lire les tables sur une copie, pour comparer canaux et fréquence à l'enregistrement `.apexlmu` d'Apex (50–60 Hz) avant de décider d'importer ces fichiers. Le lecteur DuckDB est une grosse dépendance (moteur C++) : on ne l'ajoute que si la sonde montre un gain réel (ex. séances roulées sans Apex ouvert).

## Télémétrie en accès libre (recherche du 22 nov.)

- **Le jeu lui-même** : les fichiers DuckDB ci-dessus, à toi, gratuits. C'est la seule source qui entre dans les règles d'Apex sans condition.
- **Tours de référence d'autres pilotes** : je n'ai trouvé aucun dépôt public et gratuit de tours LMU. Les références de Coach Dave Delta sont dans une offre payante, et la télémétrie de Track Titan passe par son greffon et son compte. Garage 61 est gelé.
- **Import manuel possible plus tard** (option, jamais automatique) : un fichier que tu possèdes, MoTeC `.ld` ou export SimHub, importé à la main comme référence externe, comme les tours Garage 61 aujourd'hui.

## Limites

- « Conditions inconnues » : iRacing et les anciens fichiers ne disent pas toujours si la piste est mouillée. Ces tours forment leur propre combinaison et ne sont jamais mélangés au sec.
- Un tour propre peut rester légèrement « aidé » (aspiration en course). La bibliothèque prend le meilleur tour propre, pas un tour en air libre.
- La référence auto ne remplace jamais une référence que tu as choisie (ou importée) toi-même.
