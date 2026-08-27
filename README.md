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
- **Deux lignes** — tablature + ligne de grattage (sens du médiator / tapping).
- **Trois lignes** — tablature + grattage + portée en notation classique (clé de sol 8va).

L'ordre tablature/portée est configurable (`StaffOrder::TabFirst` / `NotationFirst`,
défaut tablature en haut). Les trois lignes d'un même bloc partagent le même espacement
horizontal, calculé une seule fois, ce qui les garde alignées verticalement par
construction.

## Format de fichier

Les documents sont enregistrés en JSON lisible et diffable, extension `.gtab`
(`serde_json::to_string_pretty`). Le format est tolérant aux évolutions futures : les
champs absents dans un fichier plus ancien reprennent une valeur par défaut sensée au
chargement (`#[serde(default)]`).

## Cibles Makefile

| Cible | Effet |
|---|---|
| `run` | `cargo run` |
| `build` | `cargo build --release` puis copie du binaire dans `dist/` |
| `app` | empaquette `dist/Strungin.app` (Info.plist + binaire) — macOS |
| `examples` | régénère `exemples/*.pdf` via `--export` (arrive en phase 6) |
| `test` | `cargo test` |
| `fmt` | `cargo fmt` |
| `lint` | `cargo clippy --all-targets -- -D warnings` |
| `clean` | `cargo clean` + vide `dist/` |

## État du projet

Ce dépôt suit un plan de développement en 6 phases, chacune laissant une application qui
compile et se lance. Sont en place ici les **phases 1 et 2** : squelette de l'application
(fenêtre eframe thémée, menu complet, raccourcis clavier, i18n FR/EN avec détection de la
locale système) et le modèle de données (`model.rs`) avec persistance JSON (nouveau /
charger / enregistrer / enregistrer sous, indicateur de modifications non enregistrées).

La gravure musicale complète (ligatures, hampes, liaisons, portée, altérations...), le
rendu à l'écran de la page et l'export PDF arrivent dans les phases suivantes.

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
