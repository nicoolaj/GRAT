# Changelog

Toutes les évolutions notables de GRAT sont consignées ici, reconstituées à partir de l'historique
Git et de ses tags. Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) et le
projet respecte le [versionnage sémantique](https://semver.org/lang/fr/).

## [4.5.0] — 2026-09-26

### Ajouté
- Menu « Mesure » (et clic droit) : insérer une mesure avant ou après la sélection, dupliquer ou
  supprimer les mesures sélectionnées, chacun en une seule étape d'annulation.
  - Une mesure insérée prend le chiffrage en vigueur à cet endroit.
  - Les copies gardent leur chiffrage mais pas les marques de reprise.
  - Une suppression transmet ce dont dépend la suite : le changement de chiffrage passe à la
    mesure suivante, une ‖: dont le passage continue passe à la mesure suivante, une :‖ à la
    précédente avec son nombre de passages ; une reprise supprimée en entier disparaît. Il reste
    toujours au moins une mesure.

## [4.4.0] — 2026-09-26

### Ajouté
- Le lecteur live joue les reprises : le passage entre ‖: et :‖ est rejoué autant de fois
  qu'indiqué, avec le métronome et le son. Les flèches ← → avancent mesure par mesure dans
  l'ordre joué, et la vue défile d'un trait jusqu'à la barre de reprise avant de revenir.
- Le nombre de passages d'une reprise se choisit de ×2 à ×8 (« x3 » s'imprime au-dessus de la
  barre).
- Nouveau menu « Mesure », repris au clic droit sur la partition : « Reprise sur la sélection »
  (Cmd+R) pose ‖: sur la première mesure sélectionnée et :‖ sur la dernière, ou les retire. Les
  cases de reprise de la palette agissent aussi sur toute la sélection.

### Corrigé
- Un clic droit à l'intérieur d'une sélection la conserve (le menu agit sur toute la plage) ; un
  clic droit ailleurs sélectionne la seule case visée, au lieu d'étirer la sélection jusqu'à elle.

## [4.3.0] — 2026-09-26

### Ajouté
- Numéros de mesure : au début de chaque ligne sur la page et dans le PDF (la mesure 1 exceptée),
  au-dessus de chaque mesure dans le mode live sur une ligne. Ils sont centrés sur la barre de
  mesure, juste au-dessus de la portée.

## [4.2.4] — 2026-09-25

### Modifié
- En français, la page imprime « laisser sonner » au lieu de « let ring », comme la case de la
  palette. Le « full » d'un tiré d'un ton passe lui aussi par les fichiers de traduction.
- La palette range le trille avec les autres ornements, dans le même ordre que l'aide et le menu
  des styles de notes, qu'elle suit désormais d'office.
- Factorisation interne sans effet sur la gravure : barres de mesure et reprises résolues une
  seule fois pour toutes les rangées, largeur de l'en-tête de portée unique, liste des valeurs de
  note unique, accès à la note sélectionnée mis en commun (environ 200 lignes de moins). Le PDF de
  chaque document de contrôle est identique à l'octet près.

## [4.2.3] — 2026-09-25

### Corrigé
- À l'écran, les crochets des croches et doubles-croches isolées ont enfin leur vraie forme :
  l'affichage remplissait toute la courbe comme une voile (trois fois la surface du crochet),
  alors que le PDF était juste. Même correction pour les étoiles des décorations du logo.
- Une liaison (hammer-on, pull-off, glissé, appoggiature) s'arrête juste avant le numéro
  d'arrivée, calculé avec sa propre largeur : dans « 5h12 », l'arc entrait dans le « 12 ». La
  lettre H ou P est centrée au-dessus de l'arc.
- Changer la langue de l'interface met aussitôt la page à jour (noms d'accords, ligne
  d'accordage), au lieu d'attendre la modification suivante.

## [4.2.2] — 2026-09-25

### Corrigé
- Dans le PDF, le titre, l'auteur, les noms d'accords et tout texte centré tombent enfin au
  centre : leur largeur était estimée comme une suite de chiffres, et « Hotel California »
  partait 8,6 mm trop à gauche. La largeur de chaque caractère est désormais celle de la police
  Helvetica du PDF.
- Les caches blancs derrière `(5)`, `<12>`, `6(9)` ou `x` épousent leur texte au lieu d'effacer
  la corde trop largement.

### Modifié
- L'éditeur ne dessine plus que les pages visibles : 0,2 ms par image au lieu de 6,6 ms pour une
  partition de 400 mesures.
- Surligner une plage de mesures ne ralentit plus l'affichage : une sélection en fin de longue
  partition coûtait 43 ms par image, 0,3 ms désormais.
- Le mode live sur une ligne ne dessine plus que la portion visible du ruban (0,3 ms par image au
  lieu de 4,9 ms à 400 mesures).

## [4.2.1] — 2026-09-25

### Corrigé
- Fermer la fenêtre (bouton de fermeture, Alt+F4, Quitter depuis le Dock ou la barre des tâches)
  avec des modifications non enregistrées pose désormais la question au lieu de tout perdre.
- Nouveau et Ouvrir repartent d'un historique vide : un Annuler juste après l'ouverture d'un
  fichier ne ramène plus le morceau précédent sous le nom du nouveau (qu'un Enregistrer aurait
  alors écrasé).
- Plus de plantage en tapant une case après l'ouverture d'un fichier ou un Annuler qui compte
  moins de cordes que la sélection.
- Windows : ouvrir le PDF exporté passe par l'Explorateur et non plus par `cmd`, qui lisait un
  `&` du nom de fichier (repris du titre du morceau) comme une seconde commande à exécuter.
- Le nom proposé pour le PDF retire les caractères qu'un système de fichiers refuse (« AC/DC »).
- L'enregistrement passe par un fichier temporaire renommé ensuite : un plantage en pleine
  écriture laisse la version précédente intacte au lieu d'un fichier tronqué.
- Un fichier `.gtab` (ou un collage) aux valeurs hors limites ne fait plus planter ni ne produit
  de hauteurs fausses : trop de points d'augmentation ou une mesure sans temps sont refusés, la
  hauteur est plafonnée au lieu de déborder, l'échelle et l'espacement sont ramenés dans des
  bornes raisonnables.

## [4.2.0] — 2026-09-25

### Ajouté
- Nouvelles mesures proposées : 3/2, 6/4, 7/4, 3/8 et 5/8. La liste est désormais rangée par
  dénominateur (2/2 et 3/2 en tête, puis les /4, puis les /8).

### Corrigé
- Le menu déroulant de la mesure n'est plus limité en hauteur : 2/2, placé en fin de liste,
  était caché sous la barre de défilement.

## [4.1.2] — 2026-09-25

### Corrigé
- L'espace entre deux temps est le même partout : un temps qui se termine sur une double-croche
  (une syncope coupée au temps) est désormais aussi loin du suivant qu'un temps qui se termine
  sur une croche. Les liaisons qui traversent ces temps ont donc la même longueur.

## [4.1.1] — 2026-09-25

### Corrigé
- Une note liée garde l'espace de sa propre durée : en 4.1.0, une double-croche liée recevait
  la place d'une croche et paraissait plus longue que la croche pointée qui la précède.
- Sur la portée, la liaison d'une note seule (ou de la note extérieure d'un accord) passe sous
  les têtes, les extrémités près de leur centre, comme en gravure : même entre deux notes
  rapprochées elle reste lisible, et elle a la longueur de la liaison de la tablature.

## [4.1.0] — 2026-09-25

### Modifié
- **Espacement par groupes** : l'écart entre deux groupes rythmiques vaut 1,3 fois l'écart entre
  deux notes d'un même groupe, sur toutes les lignes (tablature, rythme, portée). La séparation
  tombe à chaque temps dès qu'une mesure contient des croches ou plus rapide, tous les 2 temps
  (au milieu de la mesure en 4/4) quand elle ne va pas plus vite que la noire.
- **Liaisons uniformes** : une note liée reçoit au moins la place d'une croche, et la tablature
  comme la portée dessinent la même courbe, plus plate quand elle est courte. Une double-croche
  liée n'a plus de liaison écrasée, ni de « U » profond dans la tab. (Place minimale retirée en
  4.1.1.)
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

[4.2.0]: https://github.com/nicoolaj/GRAT/compare/v4.1.2...v4.2.0
[4.1.2]: https://github.com/nicoolaj/GRAT/compare/v4.1.1...v4.1.2
[4.1.1]: https://github.com/nicoolaj/GRAT/compare/v4.1.0...v4.1.1
[4.1.0]: https://github.com/nicoolaj/GRAT/compare/v4.0.0...v4.1.0
[4.0.0]: https://github.com/nicoolaj/GRAT/compare/v3.6.1...v4.0.0
[3.6.1]: https://github.com/nicoolaj/GRAT/compare/v3.6.0...v3.6.1
