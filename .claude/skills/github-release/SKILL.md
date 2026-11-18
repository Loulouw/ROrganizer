---
name: github-release
description: Publier une release GitHub de ROrganizer — contrôles de version, rédaction et validation des notes au format du projet, pose du tag annoté, puis cycle brouillon → soumission antivirus Microsoft → publication. À utiliser quand l'utilisateur demande de sortir une version, publier une release, taguer une version, ou rendre publique une release en brouillon.
---

# Publier une release ROrganizer

Le workflow `.github/workflows/release.yml` fait le build, l'attestation de
provenance et la publication. **Ce skill s'occupe de tout ce qui doit être
correct avant que le tag parte** — un tag poussé ne se rattrape pas
proprement.

Le corps de la release est le **message du tag annoté**. Le workflow le lit
et y ajoute le bloc SHA256 / provenance. Il n'y a donc rien à écrire côté
GitHub une fois le tag poussé.

## 1. Contrôles pré-vol

Tous bloquants. **Au premier échec, arrête-toi et affiche le correctif** — ne
tente pas de corriger toi-même sans validation.

| # | Contrôle | Commande |
|---|---|---|
| 1 | Branche `main` | `git branch --show-current` |
| 2 | Arbre propre | `git status --porcelain` doit être vide |
| 3 | À jour avec `origin` | `git fetch origin && git status -sb` |
| 4 | Version du manifeste | `version` dans `Cargo.toml` |
| 5 | Version > dernière release | `gh release list --limit 1` |
| 6 | `Cargo.lock` cohérent | la ligne `name = "rorganizer"` doit porter la même version, et le fichier être commité |
| 7 | Tag absent en local | `git tag -l v<version>` doit être vide |
| 8 | Tag absent sur `origin` | `git ls-remote --tags origin v<version>` doit être vide |
| 9 | Tests verts | `cargo test` |
| 10 | Build sans warning | `cargo build --release` |

Notes :

- Le contrôle 5 compare en **semver**, pas en texte : `1.10.0` est supérieur
  à `1.9.0`. Refuse une version égale ou inférieure à la dernière publiée.
- Si `Cargo.toml` n'a pas été bumpé, c'est l'erreur la plus fréquente :
  dis-le explicitement et propose la version suivante selon la nature des
  changements (patch / mineure / majeure), sans l'appliquer d'office.
- Les contrôles 9 et 10 tournent avec le `PATH` cargo. Windows :
  `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`. Mac :
  `source ~/.cargo/env`.

## 2. Rédaction des notes

Lis `release-notes-template.md`, à côté de ce fichier — il porte le format,
les règles et un exemple complet. Il fait autorité, pas ce qui suit.

Construis le contenu à partir des commits depuis le dernier tag :

```powershell
git log v<précédente>..HEAD --oneline
```

Traduis chaque changement en **ce que l'utilisateur voit changer**, jamais en
termes d'implémentation. Le lectorat, ce sont des joueurs de Dofus.

## 3. Validation

Affiche en clair, dans la conversation :

- le titre : `v<version>`
- le corps complet en markdown

**Attends un OK explicite.** Aucun `git tag`, aucun `git push`, aucun
`gh release` avant cette validation. C'est une règle du projet, pas une
préférence de ce skill.

## 4. Pose du tag

Le corps validé va dans le message du tag annoté :

```powershell
git tag -a v<version> --cleanup=verbatim -F <fichier-notes.md>
git push origin v<version>
```

⚠️ **`--cleanup=verbatim` n'est pas optionnel.** Par défaut `git tag -F`
applique `--cleanup=strip`, qui supprime toute ligne commençant par `#` en
la prenant pour un commentaire. Sans ce drapeau, **tous les titres `###`
disparaissent** du corps — y compris la section SmartScreen, qui sert de
point d'insertion au bloc provenance du workflow.

Après création, relis toujours ce que le workflow lira :

```powershell
git tag -l --format='%(contents)' v<version> | Select-String '^###'
```

```zsh
git tag -l --format='%(contents)' v<version> | grep '^###'
```

Tous les titres du gabarit doivent être là, SmartScreen et macOS compris.
S'ils manquent, `git tag -d` et recommence.

Écris le fichier de notes dans le répertoire scratchpad de la session, pas
dans le dépôt.

## 5. Vérification du brouillon

Le workflow crée la release **en brouillon**, pas publiée. Il met une
quinzaine de minutes : le job macOS compile deux architectures. Ensuite :

```powershell
gh run list --workflow=release.yml --limit 1
gh release view v<version> --json name,isDraft,assets,body
```

Vérifie :

- le workflow est en succès ;
- `isDraft` vaut `true` ;
- le titre est exactement `v<version>` ;
- deux assets, nommés `ROrganizer-<version>-x86_64-windows.exe` et
  `ROrganizer-<version>-universal-macos.dmg` ;
- le corps contient le bloc `🔐 Provenance et intégrité` avec un SHA256
  par fichier, **placé avant la section SmartScreen** ;
- le corps contient la ligne `🌐 **Site officiel**` pointant vers
  `https://rorganizer.loulouw-labs.fr/fr/`, juste avant `Full Changelog` ;
- l'attestation des deux fichiers passe déjà, sans attendre la
  publication :
  ```powershell
  gh release download v<version>
  gh attestation verify ROrganizer-<version>-x86_64-windows.exe --repo Loulouw/ROrganizer
  gh attestation verify ROrganizer-<version>-universal-macos.dmg --repo Loulouw/ROrganizer
  ```
  Les mêmes commandes passent telles quelles sous zsh.

Si le workflow échoue, ne force rien : supprime le tag distant
(`git push origin :refs/tags/v<version>`), corrige, recommence. Tant que la
release est en brouillon, le tag est jetable.

## 6. Soumission antivirus, puis publication

**Ne jamais publier avant ce passage.** L'exécutable déclenche un faux
positif Defender (`Wacatac`) tant qu'il n'a pas été soumis. Un joueur qui
télécharge dans l'intervalle voit une alerte de virus.

1. Soumettre l'exécutable téléchargé à l'étape 5 sur
   <https://www.microsoft.com/en-us/wdsi/filesubmission>, en tant
   qu'éditeur du logiciel, motif « faux positif ». Le `.dmg` n'a rien à
   soumettre : Apple n'a pas d'équivalent sans compte développeur payant.
   Defender peut mettre le fichier en quarantaine au téléchargement : dans
   ce cas, exclusion temporaire du dossier le temps de la soumission.
2. Attendre le verdict — 24 à 72 h en général.
3. Publier :
   ```powershell
   gh release edit v<version> --draft=false
   ```

⚠️ **Ne jamais recompiler entre la soumission et la publication.** Microsoft
blanchit une empreinte précise. Un binaire rebuildé a un autre hash, la
soumission ne s'y applique plus, et le faux positif revient. Le fichier
publié doit être exactement celui qui a été soumis — c'est toute la raison
d'être du brouillon.

## 7. Après publication

```powershell
gh release view v<version> --json isDraft,isLatest
```

`isDraft` doit être `false` et `isLatest` `true`.

## En cas de doute

Ce skill ne publie jamais sans validation humaine, et n'écrit jamais dans le
dépôt. S'il manque une information (numéro de version voulu, nature des
changements), demande — ne devine pas.
