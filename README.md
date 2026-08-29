# Strungin

Strungin est un éditeur de tablatures pour guitare 6 cordes, en Rust (`egui`/`eframe`), avec saisie à la
souris et export PDF A4 prêt à imprimer. Binaire unique, portable (macOS en priorité,
Windows/Linux visés), thème clair/sombre suivant l'OS, interface FR/EN détectée
automatiquement.

## Compilation et lancement

```sh
make run      # cargo run — lance la fenêtre
make build    # cargo build --release, copie le binaire dans dist/
make app      # (macOS) empaquette dist/Strungin.app
make test     # cargo test
```

Voir `make help` pour la liste complète des cibles.

## Les trois modèles de bloc

Un document choisit, globalement ou par réglage, entre trois présentations
(`model::BlockModel`) :

- **Une ligne** — tablature seule.
- **Deux lignes** — tablature + portée en notation classique (clé de sol 8va).
- **Trois lignes** — tablature + grattage (sens du médiator / tapping) + portée en notation classique.

L'ordre tablature/portée est configurable (`StaffOrder::TabFirst` / `NotationFirst`,
défaut tablature en haut). Les trois lignes d'un même bloc partagent le même espacement
horizontal, calculé une seule fois, ce qui les garde alignées verticalement par
construction.

## Format de fichier

Les documents sont enregistrés en JSON lisible et diffable, extension `.gtab`
(`serde_json::to_string_pretty`). Le format est tolérant aux évolutions futures : les
champs absents dans un fichier plus ancien reprennent une valeur par défaut sensée au
chargement (`#[serde(default)]`). Depuis la version 2 du format, un champ qui vaut son
défaut n'est simplement plus écrit (une durée s'enregistre comme un nombre de ticks
plutôt que `{base, dots}`), ce qui réduit nettement la taille des fichiers sans changer
le modèle de données ; les fichiers v1 existants continuent de se charger tels quels.

## Cibles Makefile

| Cible | Effet |
|---|---|
| `run` | `cargo run` |
| `build` | `cargo build --release` puis copie du binaire dans `dist/` |
| `app` | empaquette `dist/Strungin.app` (Info.plist + icône .icns + binaire) — macOS |
| `examples` | régénère `exemples/*.pdf` via `--export` |
| `test` | `cargo test` |
| `fmt` | `cargo fmt` |
| `lint` | `cargo clippy --all-targets -- -D warnings` |
| `clean` | `cargo clean` + vide `dist/` et `build/` |

## État du projet

Les 6 phases du plan de développement sont terminées : squelette et thème, modèle de
données et persistance JSON, mise en page et rendu écran, édition à la souris, gravure
complète (ligatures, hampes, liaisons, portée, altérations), export PDF et finitions
(menu Aide, à propos, icône de fenêtre et de bundle, exemples). `make run` ouvre
l'application complète ; `make examples` régénère les PDF du dossier `exemples/`, qui
montrent les trois modèles de bloc sur des morceaux courts mais musicalement plausibles.

## Licence

CC BY-NC-SA 4.0 — voir [LICENSE](LICENSE).

Ce logiciel peut être réutilisé et modifié librement, à trois conditions : créditer l'auteur,
ne pas en faire un usage commercial, et redistribuer les versions dérivées sous cette même
licence.

Auteur : Nicolas Jalibert <nicoolaj@gmail.com> — <https://github.com/nicoolaj>

## Faire évoluer le logiciel

[CLAUDE.md](CLAUDE.md) est le guide destiné à Claude et aux autres agents : l'idée
d'architecture, la carte des modules, les invariants à ne pas casser, les faits d'API déjà
vérifiés, et des recettes pour les ajouts courants (une technique de jeu, une langue, un
modèle de bloc).
