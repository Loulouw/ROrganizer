# Sécurité

*[English version below](#security-english)*

## Distribution officielle

ROrganizer est distribué **uniquement** depuis la page Releases de ce
dépôt :

**https://github.com/Loulouw/ROrganizer/releases**

Une release officielle contient **un fichier par système** : un
exécutable nommé `ROrganizer-<version>-x86_64-windows.exe` pour Windows,
et une image disque nommée `ROrganizer-<version>-universal-macos.dmg`
pour macOS.

Il n'existe **aucune** distribution officielle sous une autre forme.
En particulier, ROrganizer n'est jamais distribué :

- dans une **archive** (`.zip`, `.rar`, `.7z`), et encore moins dans une
  archive protégée par mot de passe ;
- sous forme de **script** (`.bat`, `.cmd`, `.ps1`, `.vbs`) ;
- via un **installeur** (`setup.exe`, `.msi`, `.pkg`) ;
- depuis un hébergeur de fichiers (Google Drive, MediaFire, Mega,
  Dropbox…), un lien Discord, un forum ou un site miroir.

Tout fichier portant le nom ROrganizer et ne correspondant pas à la
description ci-dessus **ne vient pas de moi**, quelle que soit la source
qui le propose.

## Le seul site officiel

**https://rorganizer.loulouw-labs.fr**

Ce site présente l'application ; il **n'héberge aucun fichier**. Son bouton
de téléchargement renvoie vers la page Releases ci-dessus, et c'est le seul
comportement légitime.

Autrement dit : un site qui propose ROrganizer en téléchargement direct n'est
pas le mien, même s'il en reprend le nom, les captures ou l'apparence.

Un mot de passe d'archive communiqué à part — dans une vidéo, un
commentaire, un message privé — sert à empêcher l'analyse antivirus
automatique de l'hébergeur. C'est un signal d'alerte à lui seul.

## Vérifier ce que tu as téléchargé

Le **SHA256** de chaque fichier publié est affiché par GitHub à côté de
son nom sur la page de la release. Compare-le au tien :

```powershell
Get-FileHash .\ROrganizer-1.3.0-x86_64-windows.exe -Algorithm SHA256
```

Sur Mac :

```bash
shasum -a 256 ROrganizer-<version>-universal-macos.dmg
```

Si les deux ne correspondent pas, le fichier que tu as n'est pas celui
publié ici — supprime-le.

**À partir de la version 1.3.0**, chaque fichier publié est accompagné
d'une **attestation de provenance** GitHub, qui le relie de façon
cryptographique à ce dépôt, au tag publié et au workflow qui l'a
compilé. Les `.dmg` macOS en ont une dès la première version Mac. Avec
le [GitHub CLI](https://cli.github.com) :

```powershell
gh attestation verify .\ROrganizer-1.3.0-x86_64-windows.exe --repo Loulouw/ROrganizer
gh attestation verify ROrganizer-<version>-universal-macos.dmg --repo Loulouw/ROrganizer
```

Une attestation valide prouve que le fichier a été compilé par GitHub
depuis le code source public de ce dépôt. Personne d'autre ne peut en
produire une.

Si `gh` te répond une erreur d'authentification, connecte-toi d'abord
avec `gh auth login`.

> Les releases **antérieures à la 1.3.0** ont été compilées sur une
> machine personnelle et publiées à la main. Elles n'ont pas
> d'attestation : la commande ci-dessus répond alors `HTTP 404`. C'est
> attendu, ce n'est pas le signe d'une falsification. Leur seul moyen de
> contrôle est le SHA256 affiché par GitHub.

Sur Mac, l'app est en plus signée avec le certificat de ROrganizer (pas
avec un certificat Apple : macOS te demande donc « Ouvrir quand même »
au premier lancement). Ce certificat ne change jamais, et c'est lui qui
te permet de garder l'autorisation Accessibilité d'une version à
l'autre. Pour vérifier qu'une app installée le porte, son empreinte
SHA-1 doit être `9005666d41da362dea45bb462fa8b658c8251a7b` :

```bash
codesign -d --extract-certificates=/tmp/rorganizer-cert /Applications/ROrganizer.app
shasum /tmp/rorganizer-cert0
```

## Et une recompilation depuis les sources ?

Tu peux évidemment compiler ROrganizer toi-même — c'est décrit dans le
README — mais **le binaire obtenu n'aura pas le même SHA256 que celui de
la release**, et c'est normal.

En cause : l'éditeur de liens de Windows dispose le contenu du fichier
selon les chemins absolus des fichiers intermédiaires. Compiler dans
`C:\dev\ROrganizer` plutôt que dans le dossier utilisé par
l'intégration continue décale la mise en page du binaire — même code,
mêmes données, adresses différentes, donc hash différent. Ça se corrige
pour les chemins que le compilateur Rust enregistre, mais pas pour ceux
que l'éditeur de liens utilise. Sur Mac, la signature et l'image disque
ajoutent leurs propres différences.

Autrement dit : une différence de hash entre ta recompilation et la
release ne prouve rien, ni dans un sens ni dans l'autre. **Le moyen de
contrôle fiable, c'est l'attestation de provenance** décrite plus haut :
elle est cryptographique, elle ne dépend d'aucune condition de
compilation, et personne d'autre que ce dépôt ne peut en produire une.

## Ce que ROrganizer ne fait jamais

Ces comportements ne font pas partie de l'application. S'ils
apparaissent, tu n'exécutes pas ROrganizer :

- désactiver l'antivirus ou ajouter une exclusion Windows Defender ;
- demander une élévation en administrateur au lancement ;
- demander sur Mac une autre autorisation que l'**Accessibilité**
  (enregistrement de l'écran, accès complet au disque…) ;
- établir de lui-même une connexion réseau (les liens de la fenêtre
  « À propos » ouvrent ton navigateur, sur ton clic — c'est le seul cas
  où quelque chose s'ouvre vers l'extérieur) ;
- écrire ailleurs que dans `%APPDATA%\rorganizer\` (Windows) ou
  `~/Library/Application Support/rorganizer/` (Mac) ;
- installer un service, une tâche planifiée, une clé de registre ou un
  agent de démarrage.

## Signaler une distribution frauduleuse

Si tu croises un ROrganizer distribué ailleurs que sur la page Releases,
ouvre une [issue](https://github.com/Loulouw/ROrganizer/issues) avec le
lien. Ne le télécharge pas, ne l'exécute pas.

## Signaler une vulnérabilité

Utilise la fonction de signalement privé de GitHub, onglet **Security**
du dépôt → **Report a vulnerability**. Merci de ne pas ouvrir d'issue
publique pour une faille de sécurité.

---

# Security (English)

## Official distribution

ROrganizer is distributed **exclusively** from the Releases page of this
repository:

**https://github.com/Loulouw/ROrganizer/releases**

An official release contains **one file per system**: an executable
named `ROrganizer-<version>-x86_64-windows.exe` for Windows, and a disk
image named `ROrganizer-<version>-universal-macos.dmg` for macOS.

No other distribution channel or packaging is official. ROrganizer is
never distributed as an archive (`.zip`, `.rar`, `.7z` — least of all a
password-protected one), as a script (`.bat`, `.cmd`, `.ps1`, `.vbs`),
as an installer (`setup.exe`, `.msi`, `.pkg`), or from a file host
(Google Drive, MediaFire, Mega, Dropbox…), a Discord link, a forum or a
mirror site.

Any file carrying the ROrganizer name that does not match the above
**did not come from me**, whatever the source offering it.

## The only official website

**https://rorganizer.loulouw-labs.fr**

That site presents the application; it **hosts no files**. Its download button
points to the Releases page above, and that is the only legitimate behaviour.

In other words: a website offering ROrganizer as a direct download is not mine,
even if it reuses the name, the screenshots or the look.

An archive password shared separately — in a video, a comment, a private
message — exists to defeat the file host's automated malware scanning.
That alone is a red flag.

## Verifying your download

GitHub displays the **SHA256** of every published asset next to its name
on the release page. Compare it with yours:

```powershell
Get-FileHash .\ROrganizer-1.3.0-x86_64-windows.exe -Algorithm SHA256
```

On a Mac:

```bash
shasum -a 256 ROrganizer-<version>-universal-macos.dmg
```

If they differ, the file you have is not the one published here —
delete it.

**From version 1.3.0 onwards**, every published file ships with a GitHub
**build provenance attestation**, cryptographically binding it to this
repository, the published tag and the workflow that compiled it. macOS
`.dmg` files have one from the first Mac release. Using the
[GitHub CLI](https://cli.github.com):

```powershell
gh attestation verify .\ROrganizer-1.3.0-x86_64-windows.exe --repo Loulouw/ROrganizer
gh attestation verify ROrganizer-<version>-universal-macos.dmg --repo Loulouw/ROrganizer
```

A valid attestation proves the file was built by GitHub from this
repository's public source. Nobody else can produce one.

If `gh` returns an authentication error, run `gh auth login` first.

> Releases **before 1.3.0** were built on a personal machine and
> published by hand. They carry no attestation, so the command above
> answers `HTTP 404` on them — that is expected, not a sign of
> tampering. Their only check is the SHA256 shown by GitHub.

On a Mac, the app is also signed with ROrganizer's own certificate (not
an Apple one, which is why macOS asks for "Open Anyway" on first
launch). That certificate never changes, and it is what lets you keep
the Accessibility permission from one version to the next. To check an
installed app carries it, its SHA-1 fingerprint must be
`9005666d41da362dea45bb462fa8b658c8251a7b`:

```bash
codesign -d --extract-certificates=/tmp/rorganizer-cert /Applications/ROrganizer.app
shasum /tmp/rorganizer-cert0
```

## What about rebuilding from source?

You can of course build ROrganizer yourself — the README explains how —
but **the resulting binary will not have the same SHA256 as the
release**, and that is normal. The Windows linker lays the file out
according to the absolute paths of the intermediate files, so building
in a different directory shifts the layout: same code, same data,
different addresses, different hash. On a Mac, the signature and the
disk image add differences of their own.

A hash mismatch between your own build and the release therefore proves
nothing either way. **The reliable check is the provenance attestation**
described above: it is cryptographic, it depends on no build conditions,
and nobody outside this repository can produce one.

## What ROrganizer never does

If you observe any of these, you are not running ROrganizer: disabling
antivirus or adding a Windows Defender exclusion; requesting
administrator elevation at startup; asking on a Mac for any permission
other than **Accessibility** (screen recording, full disk access…);
opening a network connection of its own accord (the
links in the About window open your browser, on your click — that is
the only outbound action); writing outside `%APPDATA%\rorganizer\`
(Windows) or `~/Library/Application Support/rorganizer/` (Mac);
installing a service, a scheduled task, a registry key or a login item.

## Reporting a fraudulent distribution

If you come across ROrganizer distributed anywhere other than the
Releases page, please open an
[issue](https://github.com/Loulouw/ROrganizer/issues) with the link.
Do not download it, do not run it.

## Reporting a vulnerability

Use GitHub's private reporting: the repository's **Security** tab →
**Report a vulnerability**. Please do not open a public issue for a
security flaw.
