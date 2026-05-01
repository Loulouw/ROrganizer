<div align="center">

<img src="resources/icons/icon.png" alt="ROrganizer" width="128" height="128">

# ROrganizer

**Gère tes comptes Dofus Unity au clavier. Léger et rapide.**

*Tu joues jusqu'à 8 comptes en parallèle ? Bascule de l'un à l'autre **d'un seul appui clavier ou souris**. Sans interagir avec Dofus, sans connexion réseau, sans usine à gaz.*

[![Version](https://img.shields.io/github/v/release/Loulouw/ROrganizer?display_name=tag&label=version&color=97c459)](https://github.com/Loulouw/ROrganizer/releases)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#licence)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2B-0078D6?logo=windows)](https://github.com/Loulouw/ROrganizer/releases)
[![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org/)
[![100% hors-ligne](https://img.shields.io/badge/100%25-hors--ligne-2ea44f)](#-respect-de-ta-vie-privée)

<img src="resources/flags/fr.svg" height="16" alt="FR"> **Français** &nbsp;·&nbsp; <a href="README.en.md"><img src="resources/flags/en.svg" height="16" alt="EN"> English</a> &nbsp;·&nbsp; <a href="README.es.md"><img src="resources/flags/es.svg" height="16" alt="ES"> Español</a>

</div>

---

> ⚠️ **Attention aux contrefaçons.** ROrganizer n'a **pas de site web**
> officiel. Le seul endroit légitime pour télécharger l'exécutable est
> la page [Releases de ce dépôt GitHub](https://github.com/Loulouw/ROrganizer/releases).
> Tout autre site (forums, pages de téléchargement tiers, « miroirs »…)
> ne provient pas de moi et peut contenir un binaire modifié.

## Aperçu

<div align="center">
  <img src="resources/screenshots/main-dark-fr.png" alt="Aperçu de ROrganizer avec 4 comptes Dofus détectés" width="340">
</div>

## Pourquoi ROrganizer ?

Inspiré de **nAiO Organizer** — un outil que la communauté Dofus connaît
bien — mais réécrit de zéro pour être plus léger, démarrer instantanément
et offrir une interface soignée en thème clair ou sombre.

## ✅ Respect de ta vie privée

> **Hors-ligne · Aucune donnée collectée · Ne touche pas au jeu**

- L'app **ne se connecte à aucun serveur**, jamais.
- L'app **ne touche pas à Dofus** : elle ne le lit pas, ne le modifie
  pas, ne lui envoie rien. Elle se contente d'écouter tes raccourcis
  et de mettre la bonne fenêtre devant.
- **Aucune frappe clavier n'est enregistrée**. Les raccourcis sont
  identifiés à la volée puis oubliés.
- Le seul fichier que l'app écrit, c'est **sa propre configuration**
  (tes raccourcis, ta langue, ton thème) dans le dossier AppData de
  Windows.

## ⚡ Léger et rapide

<div align="center">
  <img src="resources/screenshots/perf-fr.svg" alt="Métriques de performance" width="720">
</div>

## Fonctionnalités

- 🔍 **Détection automatique** de tes comptes Dofus dès qu'ils sont lancés
- 🎯 **Un raccourci par compte** : touche du clavier ou bouton de souris
- 🔄 **Cycle entre tes comptes** avec un raccourci suivant / précédent
- ✋ **Glisse-dépose** pour ranger tes comptes dans l'ordre que tu veux
- 🌓 **Thème clair ou sombre**, en FR / EN / ES
- 📍 **Discret dans la barre système** Windows, activable d'un clic

## Démarrage en 3 étapes

<div align="center">

| 1️⃣ &nbsp; **Télécharge** | 2️⃣ &nbsp; **Lance** | 3️⃣ &nbsp; **Configure** |
|:--|:--|:--|
| Récupère le `.exe` depuis [Releases](https://github.com/Loulouw/ROrganizer/releases). Aucun installeur, aucune dépendance. | Double-clique sur l'`.exe`. C'est tout. ROrganizer détecte automatiquement tes comptes Dofus déjà lancés. | Clique sur **« définir »** à côté d'un compte, puis presse la touche ou le bouton de souris que tu veux lui attribuer. |

[![Télécharger ROrganizer](https://img.shields.io/badge/%E2%AC%87%EF%B8%8F_T%C3%A9l%C3%A9charger_ROrganizer-97c459?style=for-the-badge&labelColor=1c1c1a)](https://github.com/Loulouw/ROrganizer/releases/latest)

</div>

Compatible Windows 10 et Windows 11.

> 💡 **Tu préfères compiler depuis les sources ?**
> Avec le toolchain Rust `stable-x86_64-pc-windows-msvc` (via [rustup](https://rustup.rs)) :
> ```bash
> cargo build --release
> .\target\release\rorganizer.exe
> ```

## FAQ

**Faut-il configurer chaque compte un par un ?**
Non. ROrganizer **détecte automatiquement** tous tes comptes Dofus
dès qu'ils sont lancés. Tu n'as qu'à leur attribuer un raccourci en
cliquant sur « définir » à côté de chacun — et c'est mémorisé.

**Ça consomme quoi quand je joue ?**
Quasiment rien : **~58 Mo de RAM et 0 % de CPU au repos**. L'app
reste silencieuse en arrière-plan jusqu'à ce que tu presses un de
tes raccourcis.

**Est-ce que je risque de me faire bannir ?**
ROrganizer **n'interagit pas avec Dofus**. Il ne lit pas le jeu, ne
le modifie pas, ne lui envoie aucune commande. Il se contente de
faire passer une fenêtre devant — exactement comme `Alt+Tab` le fait
dans Windows. Cela dit, l'utilisation d'outils tiers reste à tes
risques au regard des conditions générales d'utilisation d'Ankama.

**Mes raccourcis marchent-ils pendant que je suis en jeu ?**
Oui. Les raccourcis sont écoutés **au niveau global de Windows**,
donc ils fonctionnent peu importe la fenêtre au premier plan
(Dofus, navigateur, autre app…). À une nuance près : si Dofus tourne
en **administrateur**, lance ROrganizer également en administrateur.

**Windows affiche un avertissement bleu au premier lancement, c'est normal ?**
Oui. ROrganizer n'est pas signé avec un **certificat code-signing**
(qui coûte ~300 €/an et n'est pas justifiable pour un projet
open-source gratuit). Windows SmartScreen affiche donc une alerte
par précaution sur tout exécutable peu connu. Clique sur
**« Informations complémentaires »** puis **« Exécuter quand même »**.
Pour vérifier que tu as bien le bon binaire, compare son **SHA256**
avec celui publié dans les notes de la
[release](https://github.com/Loulouw/ROrganizer/releases) — toute
différence signifierait que l'exe a été modifié.

**Comment je désinstalle l'application ?**
Supprime simplement l'`.exe`. Tes préférences sont stockées dans
`%APPDATA%\rorganizer\` — tu peux supprimer ce dossier pour un
nettoyage complet. **Aucun registre Windows modifié, aucun service
installé.**

## Disclaimer

Outil tiers non affilié à Ankama Games ni au jeu Dofus. À utiliser à
tes risques et à consulter conformément aux conditions générales
d'utilisation de Dofus.

## Licence

Distribué sous double licence, au choix :

- [MIT](LICENSE-MIT)
- [Apache 2.0](LICENSE-APACHE)

Toute contribution est implicitement couverte par cette double
licence, sauf mention contraire.
