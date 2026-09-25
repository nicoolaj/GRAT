# Changelog

Toutes les évolutions notables de GRAT sont consignées ici, reconstituées à partir de l'historique
Git et de ses tags. Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) et le
projet respecte le [versionnage sémantique](https://semver.org/lang/fr/).

## [4.1.0] — 2026-09-25

### Modifié
- **Espacement par groupes** : l'écart entre deux groupes rythmiques vaut 1,3 fois l'écart entre
  deux notes d'un même groupe, sur toutes les lignes (tablature, rythme, portée). La séparation
  tombe à chaque temps dès qu'une mesure contient des croches ou plus rapide, tous les 2 temps
  (au milieu de la mesure en 4/4) quand elle ne va pas plus vite que la noire.
- **Liaisons uniformes** : une note liée reçoit au moins la place d'une croche, et la tablature
  comme la portée dessinent la même courbe, plus plate quand elle est courte. Une double-croche
  liée n'a plus de liaison écrasée, ni de « U » profond dans la tab.
- **Fin de mesure resserrée** : l'espace avant la barre de mesure est celui de la durée de la
  dernière note, sans marge en plus (et jamais moins que l'espace en début de mesure).

## [4.0.0] — 2026-09-24

### Ajouté
- Menu **Édition** : choix de l'**instrument** (guitare, basse, ukulélé, guitare baryton, ukulélé
  baryton, banjo 5 cordes, mandoline), du **nombre de cordes** (guitare 6/7/8, basse 4/5/6), d'un
  **accordage** parmi les standards de l'instrument (Drop D, Open G, DADGAD, ukulélé Sol grave…),
  ou d'un **accordage personnalisé** corde par corde.
- Au changement d'accordage, choix entre **garder les frettes** (la tablature reste telle quelle)
  et **garder les hauteurs** (chaque note est re-frettée pour sonner pareil). Annulable.
- **Afficher l'accordage** : nulle part, sur une ligne sous le titre, ou au début de chaque corde,
  en noms de notes (Mi La Ré…) ou en lettres (E A D…).

### Modifié
- La tablature dessine une ligne par corde ; les points de reprise, « TAB » et la signature
  rythmique s'adaptent à la hauteur de la portée.
- La portée prend la clé de l'instrument : sol 8vb (guitares, banjo, ukulélé baryton), sol
  (ukulélé, mandoline), fa 8vb (basse, nouvelle clé de fa dessinée).
- Les noms d'accords suivent l'accordage ; sur un accordage rentrant (Sol aigu du ukulélé,
  5e corde du banjo) la note la plus grave n'est plus prise pour une basse (`Am`, pas `Am/C`).

### Incompatible
- Format `.gtab` en version 4 : `tuning` compte un nombre quelconque de cordes. Les fichiers
  existants se chargent sans changement, mais une version antérieure de GRAT refuse un fichier
  v4 comme trop récent.

## [3.6.1] — 2026-09-24

### Documentation
- Ajout de ce `CHANGELOG.md`.

## [3.6.0] — 2026-09-24

### Ajouté
- La signature rythmique s'imprime aussi sur la ligne de tablature, selon les règles de gravure :
  en début de morceau, à chaque changement (avec la place réservée après la barre de mesure), et
  en signature de courtoisie en fin de système quand le suivant change de mesure. Une signature à
  deux chiffres (12/8) se place après la première barre pour ne pas heurter la clé.

## [3.5.0] — 2026-09-24

### Ajouté
- Menu **Outils > Signature rythmique de toute la partition** : remesure tout le morceau en gardant
  chaque note à sa place dans le temps. Annulable.

## [3.4.0] — 2026-09-24

### Modifié
- Changer la signature d'une mesure redistribue toute la section qu'elle gouverne au lieu de
  rogner chaque mesure : 4/4 → 2/4 double les mesures sans perdre de note. Les reprises suivent.

## [3.3.5] — 2026-09-24

### Corrigé
- Le collage part du début de la plage sélectionnée.

## [3.3.4] — 2026-09-24

### Corrigé
- Collage au clavier, et collage avec le rythme copié.

## [3.3.3] — 2026-09-24

### Corrigé
- Cmd/Ctrl+C, X, V atteignent bien l'éditeur.

## [3.3.2] — 2026-09-24

### Modifié
- Les boutons de la palette affichent les flèches de strumming.

## [3.3.1] — 2026-09-24

### Modifié
- Les coups de strumming sont dessinés comme de simples flèches haut/bas.

## [3.3.0] — 2026-09-24

### Ajouté
- Les blocs deviennent une liste ordonnée de lignes activables depuis un menu **Lignes** :
  tablature, rythme, portée, strumming, noms d'accords.
- Nouvelle ligne de noms d'accords, reconnus à partir des notes de chaque événement.

### Modifié
- Format `.gtab` v3 : l'ancien modèle à 1/2/3 lignes est migré automatiquement au chargement.

## [3.2.4] — 2026-09-24

### Corrigé
- Le ventre de la clé de sol croise la hampe sur la troisième ligne.

## [3.2.3] — 2026-09-24

### Corrigé
- Clé de sol : tête complète et croisement plus bas.

## [3.2.2] — 2026-09-24

### Corrigé
- Hampes et crochets gravés selon les règles d'usage.

## [3.2.1] — 2026-09-24

### Corrigé
- Le collage conserve la durée de la cellule cible : le total d'une mesure ne change plus.

## [3.2.0] — 2026-09-17

### Ajouté
- Décorations calendaires du logo (une quinzaine de dates par an : Nouvel An, Fête de la musique,
  Halloween, fêtes nationales, Noël…), sur l'écran d'accueil, la fenêtre À propos et l'icône du
  dock. `GRAT_ICON_DATE=MM-JJ` permet d'en prévisualiser une.

## [3.1.7] — 2026-09-17

### Corrigé
- Tempo du mode live dans les mesures composées.

## [3.1.6] — 2026-09-17

### Modifié
- Le basculement page/ligne du mode live tient en un seul bouton.

## [3.1.5] — 2026-09-17

### Performance
- La pagination est mise en cache au lieu d'être recalculée à chaque image.

## [3.1.4] — 2026-09-12

### Modifié
- Géométrie des crochets de ligature partagée entre la portée et la ligne de rythme.

## [3.1.3] — 2026-09-12

### Modifié
- La fusion des silences respecte les temps forts et faibles de la mesure.

## [3.1.2] — 2026-09-11

### Modifié
- Spirale de la clé de sol tracée géométriquement.

## [3.1.1] — 2026-09-11

### Modifié
- Clé de sol redessinée d'après un modèle gravé, la spirale partant de la ligne de sol.

## [3.1.0] — 2026-09-05

### Ajouté
- Automatisation des releases : paquets pour les six plateformes plus le bundle `.app` macOS,
  CI (fmt, lint, tests) à chaque push et PR, workflow de publication sur tag.
- README d'accueil, traduction anglaise (`README.EN.md`), lien vers les Releases.

## [3.0.0] — 2026-09-05

Première version publique.

### Ajouté
- Nom officiel : **GRAT — Rythmes, Accords, Tablatures**.

## [2.6.1] — 2026-09-05

### Maintenance
- Mise à jour des dépendances (`cargo update`).

## [2.6.0] — 2026-09-05

### Ajouté
- `make audit` : contrôles `cargo-audit` et `cargo-deny`.

## [2.5.0] — 2026-09-05

### Ajouté
- Copier / couper / coller sur une plage d'événements.

## [2.4.0] — 2026-09-05

### Modifié
- Les styles de note sont regroupés en sous-menus par catégorie.

## [2.3.1] — 2026-09-05

### Corrigé
- L'écart entre tablature et portée grandit quand les notes en ont besoin.

## [2.3.0] — 2026-09-05

### Ajouté
- Mode live : bends, slides et trilles ont leur propre courbe de hauteur.

## [2.2.0] — 2026-09-04

### Ajouté
- Mode live : le morceau se fait entendre (un son synthétisé par note), avec un fader qui dose
  métronome et musique.

## [2.1.0] — 2026-09-04

### Ajouté
- Les notes qui chevauchent un temps sont coupées et reliées par des liaisons.

## [2.0.3] — 2026-09-04

### Corrigé
- Le dernier événement d'une mesure reçoit sa largeur sur le papier.

## [2.0.2] — 2026-09-04

### Corrigé
- Une ligature se prolonge à travers une note qui ne fait que tenir sur le temps.

## [2.0.1] — 2026-09-04

### Corrigé
- En 4/4, les croches consécutives sont ligaturées par demi-mesure.

## [2.0.0] — 2026-09-04

### Modifié
- Le projet Strungin devient **GRAT** (crate, binaire, bundle, documentation).
- Nouveau logo : icône de fenêtre, écran d'accueil, icône de document et illustrations
  À propos/Aide.

## [1.7.0] — 2026-09-04

### Modifié
- Renommage en GRAT, acronyme récursif tiré au hasard dans la barre de titre.

## [1.6.0] — 2026-09-03

### Ajouté
- Mode live : métronome (clic + flash numéroté par temps) et décompte configurable avant la
  lecture.

## [1.5.0] — 2026-09-03

### Ajouté
- Mode live (Cmd+L) : la partition défile au tempo et le temps à jouer est surligné. Vue pages ou
  ligne continue, vitesse de ×0,25 à ×2.

## [1.4.0] — 2026-08-30

### Modifié
- Palette : chaque technique et son numéro partagent un même halo de couleur.

## [1.3.1] — 2026-08-30

### Modifié
- À propos : la mention de licence est un lien vers la page CC BY-NC-SA.

## [1.3.0] — 2026-08-30

### Ajouté
- Saisie entièrement au clavier, avec des touches de durée.

### Corrigé
- egui ne vole plus le focus clavier.

## [1.2.1] — 2026-08-30

### Corrigé
- `make dist-cross` vérifie la présence de la chaîne zig.

## [1.2.0] — 2026-08-30

### Ajouté
- Makefile : compilation croisée Windows et Linux, amd64 et arm64.

## [1.1.0] — 2026-08-30

### Ajouté
- **Édition > Styles de note** : masquer les boutons de palette inutilisés.

## [1.0.1] — 2026-08-30

### Modifié
- Le mode de saisie devient un sous-menu du menu Édition.

## [1.0.0] — 2026-08-29

### Modifié
- Format `.gtab` v2 : les valeurs par défaut ne sont plus écrites et une durée est un nombre de
  ticks (−62 % sur un fichier réel). Les fichiers v1 se chargent toujours.

## [0.7.0] — 2026-08-29

### Ajouté
- Le format `.gtab` est versionné ; un fichier issu d'une version plus récente est refusé.

## [0.6.0] — 2026-08-29

### Ajouté
- L'export PDF normalise les silences : fusion des temps vides, suppression de la fin vide.

## [0.5.0] — 2026-08-29

### Ajouté
- Le menu Édition porte le mode de saisie ; voyant INS dans la barre d'état.

## [0.4.2] — 2026-08-29

### Corrigé
- Slide d'approche : les deux chiffres partagent la ligne de base des frettes.

## [0.4.1] — 2026-08-29

### Corrigé
- Le slide d'approche se lit comme une appoggiature et prend sa propre place.

## [0.4.0] — 2026-08-29

### Ajouté
- Nouvelles techniques : slide d'approche, slap, pop.
- Taille de tablature réglable séparément en hauteur et en espacement horizontal.
- Le document s'agrandit pour toujours offrir une ligne vierge.

### Modifié
- Une mesure se centre entre ses barres ; des mesures identiques ont la même largeur.
- Barre de menus native macOS (muda) essayée puis abandonnée (plantage au clic).

## [0.3.0] — 2026-08-28

### Ajouté
- Rééquilibrage des mesures et durée « armée » appliquée à chaque note saisie.
- Changement de durée avec décalage de la suite du morceau (optionnel).

## [0.1.0] — 2026-08-27

Première version.

### Ajouté
- Squelette de l'application : fenêtre thémée, menus, internationalisation FR/EN.
- Modèle de données et sauvegarde JSON.
- Moteur de gravure : portée et ligne de tablature.
- Mise en page A4 et pagination.
- Édition à la souris et palette d'outils.
- Export PDF (menu, Cmd+E, `--export`), pages À propos et Aide, icône macOS, exemples.

[4.1.0]: https://github.com/nicoolaj/GRAT/compare/v4.0.0...v4.1.0
[4.0.0]: https://github.com/nicoolaj/GRAT/compare/v3.6.1...v4.0.0
[3.6.1]: https://github.com/nicoolaj/GRAT/compare/v3.6.0...v3.6.1
