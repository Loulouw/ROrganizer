# Sécurité

*[English version below](#security-english)*

## Distribution officielle

ROrganizer est distribué **uniquement** depuis la page Releases de ce
dépôt :

**https://github.com/Loulouw/ROrganizer/releases**

Une release officielle contient **un seul fichier** : un exécutable
nommé `ROrganizer-<version>-x86_64-windows.exe`.

Il n'existe **aucune** distribution officielle sous une autre forme.
En particulier, ROrganizer n'est jamais distribué :

- dans une **archive** (`.zip`, `.rar`, `.7z`), et encore moins dans une
  archive protégée par mot de passe ;
- sous forme de **script** (`.bat`, `.cmd`, `.ps1`, `.vbs`) ;
- via un **installeur** ou un `setup.exe` ;
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

Si les deux ne correspondent pas, le binaire que tu as n'est pas celui
publié ici — supprime-le.

**À partir de la version 1.3.0**, chaque exécutable est accompagné d'une
**attestation de provenance** GitHub, qui le relie de façon
cryptographique à ce dépôt, au tag publié et au workflow qui l'a
compilé. Avec le [GitHub CLI](https://cli.github.com) :

```powershell
gh attestation verify .\ROrganizer-1.3.0-x86_64-windows.exe --repo Loulouw/ROrganizer
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
que l'éditeur de liens utilise.

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
- établir de lui-même une connexion réseau (les liens de la fenêtre
  « À propos » ouvrent ton navigateur, sur ton clic — c'est le seul cas
  où quelque chose s'ouvre vers l'extérieur) ;
- écrire ailleurs que dans `%APPDATA%\rorganizer\` ;
- installer un service, une tâche planifiée ou une clé de registre.

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

An official release contains **a single file**: an executable named
`ROrganizer-<version>-x86_64-windows.exe`.

No other distribution channel or packaging is official. ROrganizer is
never distributed as an archive (`.zip`, `.rar`, `.7z` — least of all a
password-protected one), as a script (`.bat`, `.cmd`, `.ps1`, `.vbs`),
as an installer, or from a file host (Google Drive, MediaFire, Mega,
Dropbox…), a Discord link, a forum or a mirror site.

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

If they differ, the binary you have is not the one published here —
delete it.

**From version 1.3.0 onwards**, every executable ships with a GitHub
**build provenance attestation**, cryptographically binding it to this
repository, the published tag and the workflow that compiled it. Using
the [GitHub CLI](https://cli.github.com):

```powershell
gh attestation verify .\ROrganizer-1.3.0-x86_64-windows.exe --repo Loulouw/ROrganizer
```

A valid attestation proves the file was built by GitHub from this
repository's public source. Nobody else can produce one.

If `gh` returns an authentication error, run `gh auth login` first.

> Releases **before 1.3.0** were built on a personal machine and
> published by hand. They carry no attestation, so the command above
> answers `HTTP 404` on them — that is expected, not a sign of
> tampering. Their only check is the SHA256 shown by GitHub.

## What about rebuilding from source?

You can of course build ROrganizer yourself — the README explains how —
but **the resulting binary will not have the same SHA256 as the
release**, and that is normal. The Windows linker lays the file out
according to the absolute paths of the intermediate files, so building
in a different directory shifts the layout: same code, same data,
different addresses, different hash.

A hash mismatch between your own build and the release therefore proves
nothing either way. **The reliable check is the provenance attestation**
described above: it is cryptographic, it depends on no build conditions,
and nobody outside this repository can produce one.

## What ROrganizer never does

If you observe any of these, you are not running ROrganizer: disabling
antivirus or adding a Windows Defender exclusion; requesting
administrator elevation at startup; opening a network connection of its
own accord (the links in the About window open your browser, on your
click — that is the only outbound action); writing outside
`%APPDATA%\rorganizer\`; installing a service, a scheduled task or a
registry key.

## Reporting a fraudulent distribution

If you come across ROrganizer distributed anywhere other than the
Releases page, please open an
[issue](https://github.com/Loulouw/ROrganizer/issues) with the link.
Do not download it, do not run it.

## Reporting a vulnerability

Use GitHub's private reporting: the repository's **Security** tab →
**Report a vulnerability**. Please do not open a public issue for a
security flaw.
