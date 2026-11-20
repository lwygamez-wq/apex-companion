# Référence de catégorie : faisabilité (chantier 2)

20 nov. 2026.

## Ce que donne le jeu

La mémoire partagée de Le Mans Ultimate (`LMU_Data`) contient :
- le classement de toutes les voitures (`vehScoringInfo`) : distance depuis la ligne, tours, meilleur tour, position et vitesse, mis à jour environ 5 fois par seconde ;
- un tableau de télémétrie pour toutes les voitures (`telemInfo`, jusqu'à `MAX_MAPPED_VEHICLES`), relié au classement par `mID`.

On ne sait pas encore si le jeu remplit la télémétrie des autres voitures en course, ni à quelle fréquence : aucun enregistrement du corpus ne la contient (Apex n'enregistrait que le rythme de catégorie). La course du 20 nov. ne permet pas de le mesurer.

## Ce qui est fait (0.45)

- **Lecture seule** : à chaque image, Apex repère l'autre voiture de ta catégorie qui a le meilleur tour au classement et lit sa télémétrie (même `mID`).
- **Enregistrement** : extension `EXT4` du `.apexlmu`. Un relevé seulement quand sa position change (classement ou télémétrie), avec distance, tours, position, vitesse, pédales et horloge de sa télémétrie. Les anciens fichiers restent lisibles.
- **Sonde** : `cargo run --release -p apex-core --example rival_probe -- course.apexlmu`. Elle donne les mises à jour par seconde de la télémétrie et du classement, la part des relevés avec une vitesse, puis un verdict : OK si au moins 5 mises à jour par seconde avec vitesse, sinon INSUFFISANT.

## Suite, selon le verdict de la sonde

- **OK** : comparer ton tour au meilleur tour de la voiture de référence, virage par virage et par phase, à partir de la vitesse et de la position (vitesse mini, point de freinage déduit de la décélération, sortie). Afficher sur la carte où tu perds. Les pédales des autres voitures ne seront pas utilisées si le jeu les donne à 0.
- **INSUFFISANT** : s'arrêter là. Le classement seul (≈ 5 Hz, distance et vitesse) ne situe pas un freinage à mieux que 15 à 20 m à 300 km/h.

À faire par Alex : une course d'au moins 20 minutes avec Apex 0.45, puis verser le `.apexlmu` au corpus.
