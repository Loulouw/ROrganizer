# Format des notes de release

Ce fichier fait autorité sur le format. L'ajuster ici suffit — le `SKILL.md`
n'a pas à être touché.

Le corps décrit ici est celui **écrit à la main**. Le workflow y insère
automatiquement un bloc `### 🔐 Provenance et intégrité` — SHA256 et commande
de vérification d'attestation — **juste avant la section SmartScreen**, pour
respecter l'ordre maison : positif d'abord, avertissements Windows ensuite.
**Ne jamais l'écrire soi-même** : il serait publié en double.

Conséquence : le gabarit ci-dessous doit toujours contenir la section
`### ⚠️ Avertissement Windows SmartScreen`. Sans elle le workflow n'a pas de
point d'insertion et ajoute le bloc en fin de corps, après
`Full Changelog`.

## Gabarit

```markdown
### ROrganizer <version> — <sujet en trois mots>

<Un paragraphe qui dit ce que la version apporte, en une phrase ou deux, du point de vue du joueur.>

### ✨ Nouveautés

- <Ce qui change pour l'utilisateur, et pourquoi ça compte.>
- <Une ligne par point.>

### 🔧 Sous le capot

- <Changements sans effet visible, formulés en langage courant.>

### ⚠️ Avertissement Windows SmartScreen

Le binaire n'est pas signé avec un certificat code-signing. Au premier lancement, Windows peut afficher un avertissement bleu. Clique sur **« Informations complémentaires »** → **« Exécuter quand même »**. Si ton antivirus signale quelque chose, la FAQ du README explique pourquoi.

### 🍎 macOS : premier lancement

L'app n'est pas signée par Apple, ce qui demande un abonnement payant. Au premier lancement de chaque nouvelle version, macOS la bloque : ouvre **Réglages Système → Confidentialité et sécurité**, descends jusqu'au message sur ROrganizer et clique sur **« Ouvrir quand même »**. Au tout premier lancement, ROrganizer te demande aussi l'autorisation **Accessibilité** : le bouton de son bandeau t'y emmène. Elle est conservée d'une version à l'autre.

🌐 **Site officiel** : [rorganizer.loulouw-labs.fr](https://rorganizer.loulouw-labs.fr/fr/)

**Full Changelog**: https://github.com/Loulouw/ROrganizer/compare/v<précédente>...v<version>
```

## Règles

**Titre de la release** : exactement `v<version>`, préfixe `v` compris. La
release 1.0.0 avait été titrée `1.0.0` sans le `v` — c'est la dérive à
bloquer.

**Niveau de titre** : `###` partout, y compris la ligne de tête. La 1.1.0
utilisait `##`, la 1.2.0 `###` ; on fige sur `###`.

**Pas de hard-wrap.** Contrairement au reste du projet qui wrappe à ~70
caractères, GitHub gère le retour à la ligne. Un paragraphe = une ligne
logique, un bullet = une ligne. Une ligne cassée au milieu d'un paragraphe
rend mal dans le navigateur.

**Ton grand public.** Le lectorat, ce sont des joueurs de Dofus, pas des
contributeurs. Bannir : noms de modules, `include_bytes!`, « embarqué dans le
binaire », noms de crates, numéros de version de dépendances, « regex sur le
titre ». Décrire ce que l'utilisateur voit changer.

**Pas de section « Téléchargement ».** GitHub liste les assets sous la
release ; une liste manuelle est redondante et se désynchronise.

**Pas de SHA256 en dur** écrit à la main : c'est le workflow qui l'ajoute.

**Ligne « Site officiel » toujours présente, toujours vers `/fr/`.** Pas
vers la racine `https://rorganizer.loulouw-labs.fr` : c'est un redirecteur
en `noindex`, un lien vers lui transmet peu de valeur au site. En français
parce que les notes le sont et que le public l'est majoritairement. Les
releases v1.0.0 à v1.3.2 ont été complétées après coup sur GitHub : leur
message de tag ne contient pas cette ligne, c'est voulu (un tag ne se
réécrit pas).

**Sections optionnelles.** `✨ Nouveautés` et `🔧 Sous le capot` sautent si
elles sont vides. `🔧 Polish` est une variante acceptée de `Sous le capot`
pour une version cosmétique. Le bloc **SmartScreen est toujours présent** :
l'avertissement est certain puisque le binaire n'est pas signé, et cette
section sert de **point d'insertion** au bloc provenance du workflow. La
retirer casserait le placement. Le bloc **macOS est toujours présent** lui
aussi : Gatekeeper bloque chaque nouvelle version téléchargée, sans
exception, faute de signature Apple.

**Pas de section antivirus.** Elle a été retirée après la 1.3.0 : chaque
release est désormais soumise à Microsoft avant publication, donc la
détection ne devrait plus survenir. Un pavé « ton antivirus va peut-être
crier au trojan » en tête de téléchargement installe le doute chez des gens
qui n'auraient rien vu. L'explication complète reste dans la FAQ du README,
pour les cas restants — la soumission ne couvre que Defender, pas les autres
éditeurs.

**Ordre final publié** : accroche → nouveautés → sous le capot → *provenance
et intégrité (injecté)* → SmartScreen → macOS → site officiel → `Full
Changelog`. Les sections positives d'abord ; les avertissements de
plateforme viennent en fin de note pour ne pas anxiogéniser l'entrée,
suivis seulement des deux liens.

## Référence

Les releases `v1.1.0` et `v1.2.0` sont les modèles :

```powershell
gh release view v1.2.0 --json body --jq .body
```
