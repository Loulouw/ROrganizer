# CLAUDE.md

Notes techniques sur le projet pour les contributeurs et les sessions
Claude Code. Pas de prose marketing — uniquement les pièges et décisions
non-évidentes à la lecture du code.

## Projet

Gestionnaire de fenêtres Dofus Unity en Rust (Windows). Permet de focus
n'importe quel compte lancé via raccourcis clavier/souris globaux.

## État actuel

Application fonctionnelle. `cargo test` doit rester vert et `cargo build`
sans warning. À jour à la phase 17 (icônes de classe), deps à jour au
2026-08-05.

## Conventions

- **Crate name** : `rorganizer` (le dossier reste `ROrganizer`).
- **License** : double `MIT OR Apache-2.0`.
- **Manifeste Windows** : `asInvoker`. Pas de `requireAdministrator`
  malgré la phase 5 — les hooks LL marchent en user-process tant que
  Dofus tourne aussi en user. Élever le manifeste forcerait l'utilisateur
  à passer une UAC à chaque lancement sans gain.
- **Pas de logging fichier**. L'app n'écrit que dans
  `%APPDATA%\rorganizer\config.json`. Aucun temp file, aucun log keystrokes.
- **Pas de réseau** : zéro socket sortant.

## Stack & deps clés

- `eframe 0.35` avec `default-features = false, features = ["glow", "default_fonts"]`.
  ⚠ Ne PAS retirer plus de features — tester `default-features = false`
  sur `egui` casse la création de fenêtre (font lookup).
- `egui_extras 0.35` avec uniquement le feature `svg`. Le PNG loader n'est
  PAS compilé : un `Image::from_bytes("bytes://*.png", ...)` rend le
  placeholder rouge d'erreur. Décoder les PNGs via la crate `image` puis
  `ctx.load_texture(...)` à la place. Voir `App::about_icon`.
- `egui_dnd 0.16` — suit strictement la version d'egui, pas de combinaison
  intermédiaire compilable.
- `tray-icon 0.24` en `default-features = false`. Les features par défaut
  (`gtk`, `libxdo`) ne servent que le backend Linux/BSD, jamais compilé
  ici : leurs deps sont target-gated. Les garder n'entretenait que la
  famille gtk-rs non maintenue dans le lockfile et 9 advisories dans
  chaque rapport d'audit.
- `windows 0.62` features : `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`,
  `Win32_UI_Input_KeyboardAndMouse`, `Win32_System_Threading`,
  `Win32_System_ProcessStatus`, `Win32_Globalization`, `Win32_Security`,
  `Win32_UI_Accessibility`.
- `raw-window-handle 0.6`.
- `arc-swap 1` pour `WindowsSnapshot` (lecture lock-free hot path).
- `webbrowser 1` pour ouvrir les URLs (évite la console qui flashe avec
  `cmd /C start`).
- `rust-i18n 4` — la macro `i18n!` tourne au build et lit `locales/`.
- `image 0.25` (png only) + `ico 0.5` en build-deps pour générer le `.ico`
  multi-résolution.
- Pas de `tokio`, pas d'`async-runtime`. Threads natifs uniquement.
- Aucun client HTTP dans l'arbre (`reqwest` / `hyper` / `tokio` absents du
  lockfile) — c'est la traduction concrète de la règle « pas de réseau ».

## Build & run

Toolchain : `stable-x86_64-pc-windows-msvc`.

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build --release          # ~50 s, ne pas distribuer un debug build
.\target\release\rorganizer.exe
```

`windows_subsystem = "windows"` en release retire la console — `eprintln!`
est perdu. Pour du debug rapide, build sans le flag (`cargo build`) et
lancer depuis un terminal. Pas de log fichier en prod.

## Publication d'une release

`.github/workflows/release.yml` se déclenche sur un tag `v*` : tests,
build, attestation de provenance, création de la release **en brouillon**.
**Ne jamais builder et uploader un exe à la main** — un binaire local n'a
pas d'attestation.

### Pourquoi un brouillon et pas une publication directe

L'exe déclenche un faux positif Defender (`Wacatac`) tant qu'il n'a pas
été soumis à Microsoft. La soumission blanchit **une empreinte précise** :
recompiler entre la soumission et la publication produit un autre hash et
annule le bénéfice. D'où le cycle en deux temps — build unique, soumission,
puis `gh release edit <tag> --draft=false`.

L'attestation de provenance est rattachée à l'empreinte de l'artefact, pas
à la release : elle est vérifiable pendant toute la durée du brouillon.

Le workflow écrit les étapes restantes dans le résumé du run Actions.

Le corps de la release est le **message du tag annoté**, dans lequel le
workflow insère le bloc SHA256 / provenance juste avant la section
SmartScreen.

Deux pièges, tous deux rencontrés en vrai à la v1.3.0 :

- **Créer le tag avec `--cleanup=verbatim`.** Par défaut `git tag -F`
  applique `--cleanup=strip`, qui supprime toute ligne commençant par `#`
  en la prenant pour un commentaire : les titres `###` disparaissent tous,
  y compris l'ancre SmartScreen dont dépend l'insertion.
- **Lire le message par l'API, pas par `git`.** `actions/checkout` résout
  le tag vers son SHA de commit et recrée la ref : sur le runner c'est un
  tag *léger*, et `git tag --format='%(contents)'` retourne alors
  silencieusement le message du **commit**. La v1.3.0 a ainsi publié un
  corps de release constitué du message « chore: bump 1.3.0 ». Le workflow
  passe donc par `git/ref/tags` puis `git/tags/<sha>`, et échoue
  explicitement si le tag n'est pas annoté.

Le skill local `/github-release` fait les contrôles pré-vol (version
bumpée, tag inexistant, arbre propre, tests verts) et le format des
notes. Il est dans `.claude/`, donc non versionné.

### Remaps de chemins

Le binaire embarque les chemins absolus de la machine de build — sources
de std, du registre cargo, emplacements de panic. Le build local de la
v1.2.0 publiait ainsi `C:\Users\<nom>\.cargo\registry\…` 92 fois dans
l'exe distribué.

```
--remap-path-prefix=$CARGO_HOME=/cargo
--remap-path-prefix=$RUSTUP_HOME=/rustup
--remap-path-prefix=$GITHUB_WORKSPACE=/src
-Clink-arg=/Brepro        # sinon l'horodatage PE change à chaque build
```

Oublier le remap `rustup` laisse passer les chemins de la std — c'est
celui qu'on rate en premier. L'étape *Fail on leaked absolute paths* du
workflow échoue si un `X:\Users\` survit dans le binaire.

### La reproductibilité ne tient PAS entre chemins différents

Mesuré, pas supposé. Avec les flags ci-dessus, même rustc, `cargo clean`
entre les deux :

- deux builds **au même chemin** → hash bit-identique ;
- deux builds à des **chemins différents** → 2 158 038 octets diffèrent.

Le diff est trompeur : `.rsrc` diffère à 98 % et `.rdata` à 74 %, mais
`.text` à 0,5 % seulement. En regardant les octets, ce sont des **RVA**,
pas des données — `icon.ico` et `app.rc` générés sont byte-identiques,
les 6 PNG de l'icône aussi, tout est juste décalé de 24 octets.

Cause : `--remap-path-prefix` n'agit que sur ce que **rustc** enregistre.
L'éditeur de liens MSVC reçoit les vrais chemins absolus des `.obj` et
`.lib`, et son ordonnancement en dépend. Non contournable raisonnablement.

**Conséquence à ne pas oublier** : ne jamais promettre à un utilisateur
qu'il retrouvera le SHA256 en recompilant. Le README l'affirmait avant la
v1.3.0 — c'était faux. La garantie offerte est **l'attestation de
provenance**, qui ne dépend d'aucune condition de build.

Le CI compile toujours dans `D:\a\ROrganizer\ROrganizer`, donc la
condition du chemin identique y est remplie. **Vérifié à la v1.3.0** :
deux exécutions du workflow, sur deux commits différents mais à sources
identiques et même rustc, ont produit le même binaire au bit près
(`aa2fbd71…`). La reproductibilité entre runs CI est donc réelle, celle
sur une machine tierce ne l'est pas.

Pas de `rust-toolchain.toml` : ça forcerait la version de rustc à tous
les contributeurs pour un bénéfice qui ne concerne que la vérification
d'une release.

## Site vitrine

`site/` est publié sur `https://rorganizer.loulouw-labs.fr` par
`.github/workflows/pages.yml` (GitHub Pages, source « GitHub Actions »).
DNS : `CNAME rorganizer → Loulouw.github.io` — le nom du dépôt ne figure
jamais dans l'enregistrement.

**HTML et CSS écrits à la main, aucun build, aucune dépendance.** Ce n'est
pas de la paresse : la règle supply-chain ci-dessous interdit d'exécuter du
code de build non audité, et une chaîne npm ou un générateur de site en est
exactement un. La source est la sortie, donc aucun écart où un bug se cache.
Condition de réexamen : au-delà d'une dizaine de pages, la duplication des
blocs `<head>` / nav / `hreflang` devient une fabrique de bugs SEO
silencieux — passer alors à Zola (binaire préconstruit, pas de `cargo
install`).

### Indexation

Search Console : propriété **Domaine** sur `loulouw-labs.fr`, vérifiée par un
TXT à la racine, à côté du SPF — plusieurs TXT sur le même nom sont valides, la
règle du « un seul » ne concerne que le SPF. La propriété couvre donc tous les
futurs sous-domaines sans nouvelle vérification.

IndexNow est actif pour Bing et Yandex. La clé **est** le nom du fichier déposé
à la racine du site (`site/<clé>.txt`, contenant la clé elle-même). Pour
resoumettre après ajout de pages :

```powershell
$key = (Get-ChildItem site\*.txt | Select-Object -First 1).BaseName
Invoke-WebRequest -Uri https://api.indexnow.org/indexnow -Method Post `
  -ContentType 'application/json; charset=utf-8' -Body (@{
    host = 'rorganizer.loulouw-labs.fr'; key = $key
    keyLocation = "https://rorganizer.loulouw-labs.fr/$key.txt"
    urlList = @('https://rorganizer.loulouw-labs.fr/fr/')
  } | ConvertTo-Json)
```

`202 Accepted` = pris en compte. `403` = clé non validée, vérifier que le
fichier est bien servi. **Déployer le fichier de clé avant de pinguer**, sinon
la validation échoue.

### Multilingue

`/` est un redirecteur en `noindex` qui lit `navigator.languages` ; les trois
versions réelles sont `/fr/`, `/en/`, `/es/`, avec `hreflang` réciproques et
`x-default` sur `/en/`. Deux pièges :

- La racine est exclue par son `noindex`, **pas** par `robots.txt` : une page
  interdite au crawl ne transmet jamais son `noindex`.
- La détection compare la **sous-étiquette primaire** en parcourant la liste
  **dans l'ordre**. Un `navigator.language.startsWith('fr')` raterait
  `es-419` / `es-MX`, et des préférences `["de-DE", "fr"]` doivent mener au
  français, pas au repli anglais.
- La redirection utilise `location.replace()`. Avec `location.href`, la racine
  reste dans l'historique et le bouton Retour reboucle sans fin.

### Grilles CSS : `minmax(0, 1fr)`, jamais `1fr` nu

Les enfants de grille gardent `min-width: auto`, donc un contenu non
compressible élargit toute la grille au-delà du viewport. Mesuré sur ce
site : 842 px de `scrollWidth` pour 500 px de fenêtre, à cause des blocs
`<pre class="term">`. `overflow-x: auto` sur le `<pre>` ne suffit pas.

### Polices

Impossible de sous-grouper localement : ça demande `pyftsubset` ou
`glyphhanger`, donc Python ou Node — qu'on n'installe pas. L'API CSS de
Google Fonts sert déjà des `.woff2` découpés par plage ; on récupère celui de
`latin`, qui couvre FR et ES (`¿` U+00BF et `¡` U+00A1 inclus). Les
`OFL-*.txt` sont versionnés à côté : l'OFL exige que la licence accompagne
les fichiers redistribués.

Literata et Archivo sont servis en **variable** : un seul fichier couvre
400→600, d'où `font-weight: 400 600` dans le `@font-face`.

### Le vert de l'app est inutilisable sur fond clair

`#97c459` sur `#f4f5f0` donne **1,85:1**. Le site est clair, donc l'accent
textuel est assombri à `#4f7020` (5,2:1, conforme AA) et un second token
`#67932e` (3,4:1) sert les surfaces sans texte. En thème sombre, `#97c459`
retrouve sa valeur exacte et passe à 8,9:1.

Corollaire de profondeur : en clair les panneaux se détachent par **ombre
portée**, en sombre l'ombre ne se lit pas et c'est la luminosité relative
plus le filet qui prennent le relais. Le token d'ombre tombe donc à `none`
en sombre.

### Le héros est démonstratif, pas décoratif

Un repère vert descend les lignes de comptes de la capture, en phase avec des
touches qui s'allument sous le texte : c'est le mécanisme du produit, pas un
ornement. Trois points à respecter si on y touche :

- Les coordonnées du repère sont **relevées au pixel** sur `app-*.png` — lignes
  à y=167/257/347/437 sur 968, de x=24 à x=656 sur 680 — puis exprimées en
  pourcentages pour suivre le redimensionnement. **Si l'UI de l'app change, il
  faut les re-mesurer**, pas les ajuster à l'œil.
- L'animation est **finie (trois passages)**, pas en boucle. Une animation
  perpétuelle dans un héros finit par distraire au lieu d'expliquer.
- `steps(1, end)` : le repère saute d'une ligne à l'autre. Un appui sur une
  touche est instantané ; un glissement mentirait sur le comportement réel.

### Régénérer les captures de l'app

Les captures du site font **680×968**, soit le double de celles des README.
Procédure, entièrement scriptable, avec ses quatre pièges :

1. Mettre l'écran à **200 %** et lancer les clients Dofus voulus — sans eux
   l'app affiche « 0 compte lancé » et la capture ne montre rien.
2. Le process qui capture doit être **DPI-aware avant tout appel graphique**
   (`SetProcessDpiAwarenessContext(-4)`), sinon `GetWindowRect` et
   `CopyFromScreen` travaillent dans un espace virtuel à 96 DPI.
3. `SetWindowPos` HWND_TOPMOST + `SetForegroundWindow` avant de copier, sinon
   DWM rend du noir pour les surfaces accélérées.
4. Les fenêtres Windows 11 ont des **coins arrondis** mais `GetWindowRect`
   renvoie un rectangle : la capture contient quatre éclats de ce qui était
   derrière. Il faut découper selon un rectangle arrondi (rayon 16 px à 200 %)
   et laisser les coins transparents.

La mise en scène des raccourcis passe par `config.json` (quatre comptes + les
deux sens de cycle) : **sauvegarder la config de l'utilisateur et la restaurer
à l'identique**. Changer de langue se fait par le champ `lang`, ce qui évite de
piloter le menu déroulant de l'app. Single-instance : attendre la sortie
complète du process entre deux lancements.

### Ce que le site ne réutilise pas, et pourquoi

- **`perf-*.svg`** : 720×200 à libellés de 12 px, illisibles une fois mis à
  l'échelle sur un téléphone (~6,5 px). Ces SVG existent parce que le
  markdown de GitHub ne sait pas mettre en page ; le site a du CSS, les
  métriques y sont donc rebâties en HTML.
- **Les icônes de classe** : propriété d'Ankama. Les afficher *dans l'app*
  est couvert par le disclaimer ; les sortir comme ornement autonome sur un
  site public de promotion est une exposition bien plus grande pour un gain
  décoratif. Elles apparaissent déjà dans la capture de l'app, c'est
  suffisant. Le motif de touches de clavier les remplace : générique, aucun
  ayant droit.
- **Aucun SHA256 en dur** : il change à chaque release. Le site montre les
  commandes, GitHub affiche les empreintes.

En revanche `docs/gen-social-preview.ps1` lit bien
`site/assets/img/app-<lang>.png` et **pas** `resources/screenshots/` : la carte
de partage doit montrer le même état que le héros du site. Elle réduit le
680×968 à 340×484, ce qui donne un rendu plus net que l'ancienne source à
taille native.

### Compression des PNG

Les images du site sont compressées à la main hors dépôt puis réinjectées :
**417 Ko au total au lieu de 787**, soit 75 % de gagné sur les images seules.

Ce n'est **pas** sans perte, contrairement à ce que laissent croire les
outils : les cartes `og-*` passent de 32 bits à 8 bits indexés, `icon.png` voit
64 % de ses pixels modifiés et 1 531 pixels d'alpha changés. En revanche c'est
**visuellement équivalent aux tailles d'affichage** — mesuré sur le rendu de la
page : écart maximal 42/255 et seulement 0,011 % des pixels au-delà de 16/255,
parce que la réduction à l'affichage moyenne la quantification. Vérifié aussi
sur les deux cas à risque : la carte à 640 px (rendu Discord) et l'icône à
56 px sur fond sombre, sans halo.

⚠ **`gen-social-preview.ps1` produit des PNG non compressés.** Après avoir
régénéré les cartes, il faut les repasser à la compression, sinon on remonte de
~180 Ko sans s'en apercevoir.

### Pièges de cascade rencontrés en vrai, deux fois

Deux sélecteurs descendants trop larges ont écrasé une règle de classe, parce
qu'une classe plus un élément l'emporte sur une classe seule :

- `.gate img { width: 56px }`, écrite pour l'icône de la page racine, sortait
  les drapeaux des boutons en 56×56. Corrigé en `.gate > img`.
- `.features li span { display: block }` attrapait les `span.key` imbriqués
  dans les descriptions : les touches s'affichaient en blocs pleine largeur, en
  desktop comme en mobile. Corrigé en `.features li > span`.

Leçon générale : sur ce site, **préférer l'enfant direct au descendant** dès
qu'une règle ne vise qu'un élément précis. Et diagnostiquer en injectant un
script qui affiche `getComputedStyle`, plutôt qu'en raisonnant sur la
spécificité.

Diagnostic utile pour ce genre de cas : injecter dans une copie de la page un
script qui affiche `getComputedStyle` des éléments suspects et le rendre en
headless. Mesurer, plutôt que raisonner sur la spécificité.

### Capture d'écran du site en headless

Brave (Chromium) est utilisé pour vérifier le rendu. Trois pièges :

- Sans `--user-data-dir` distinct, la commande **délègue à l'instance déjà
  ouverte** et ne fait rien. Avec un profil neuf, elle peut aussi bloquer :
  la combinaison qui marche ici est `--headless=new --disable-gpu
  --no-sandbox --virtual-time-budget=3000`, sans `--user-data-dir`.
- Le fichier est **écrit après** que le processus a rendu la main — vérifier
  son existence dans un appel séparé, pas dans le même.
- La fenêtre a une **largeur minimale d'environ 500 px** ;
  `--force-device-scale-factor` ne réduit pas le viewport CSS. Pour un vrai
  rendu à 390 px, charger la page dans une `<iframe width="390">` : le
  document interne obtient alors un viewport CSS de 390 px.
- `--force-dark-mode` n'est **pas** `prefers-color-scheme: dark` — c'est
  l'inversion automatique de Chromium. Pour tester le thème sombre, poser
  `data-theme="dark"` sur `<html>`, ce qui est le chemin de code réel du
  basculeur.

## Gotchas critiques

### eframe + viewport caché

Quand `Visible(false)`, eframe **parque sa main loop**. Conséquences :
- `Context::request_repaint()` ne réveille PAS `update()`.
- `send_viewport_cmd` s'accumule sans être traité.
- `mpsc::channel` consommés dans `update()` ne sont jamais drainés.

**Toute action depuis un handler tray doit être faite en Win32 direct**
(les handlers `MenuEvent::set_event_handler` et
`TrayIconEvent::set_event_handler` tournent automatiquement sur le
thread main) :
- **Show** : `ShowWindow(hwnd, SW_SHOW)` + `SetForegroundWindow(hwnd)`.
- **Close** : `ShowWindow(hwnd, SW_SHOW)` PUIS
  `PostMessageW(hwnd, WM_CLOSE)` (sans le ShowWindow préalable, WM_CLOSE
  reste dans la queue jusqu'au prochain réveil).

Le HWND principal est capturé en `App::new` via `cc.window_handle()` →
`tray::MAIN_HWND`.

### Tray Activer ↔ état viewport eframe désynchronisé

Quand on toggle Activer/Désactiver depuis le tray, le handler fait un
Win32 ShowWindow direct (pour réveiller la loop parquée). Mais eframe ne
sait pas que la fenêtre est maintenant visible. Si on lui envoie
`Visible(false)` ensuite, il dédupe (croit déjà caché) → no-op → fenêtre
reste visible.

**Fix** : toujours envoyer `Visible(true)` AVANT `Visible(false)` dans le
handler `TrayEvent::ToggleRequested` pour resync l'état. Voir
`App::update`.

### Single-instance — race au démarrage

Le `CreateEventW` doit être fait **avant** `eframe::run_native`, pas dans
`App::new`. Sinon une 2ème instance lancée pendant les ~50-200ms de boot
fait `OpenEventW` qui échoue silencieusement. La 1ère instance n'est pas
réveillée.

Pattern correct : `main.rs` fait `acquire_or_signal_existing()` qui crée
le mutex ET l'event, puis passe le HANDLE à `App::new` qui spawn le
waiter thread.

Sur `CreateMutexW` failure (rare), faire **exit(1)**, pas de fake
`First` qui contournerait silencieusement la protection.

### LL hooks et focus de notre propre process

Les LL keyboard hooks installés sur un thread secondaire (notre `runner`)
ne tirent **pas** quand notre propre fenêtre est foreground. Reproduit
avec compteurs : `KB_FIRES=0` quand l'app est focused, `KB_FIRES=1` quand
une autre fenêtre est focused.

Conséquence : on ne peut pas utiliser le LL hook pour la **capture de
bindings** (modal "press a key") quand notre fenêtre a le focus.
Workarounds essayés et abandonnés :
- Déplacer le hook sur le thread main → ne fixe pas + cause un freeze.
- Defocus la fenêtre via `SetForegroundWindow(GetShellWindow())` pendant
  la capture → marche mais introduit un focus flicker visible.

**Décision finale** : la capture passe par les events egui
(`Event::Key` → `key_to_vk`), avec ses limitations connues :
- `egui::Key` est un set fini → touches multimédia / F21+ / Pause non
  capturables.
- `egui::Key::NumX` confond numpad et rangée du haut (les deux mappent VK
  0x36-0x39).
- Pour les touches OEM (², ù, etc.) sur layouts non-US, egui retourne un
  `Key` basé sur la position physique (Backtick pour ²~) qui mappe à un
  VK US (0xC0). Au runtime le LL hook reçoit le vrai VK FR (0xDE) →
  mismatch.

Ces limitations sont assumées plutôt qu'un workaround visible côté UI.

### eframe `ctx.used_rect()` ne capture pas les widgets qui débordent

Tentative : `apply_dynamic_height` basée sur `ctx.used_rect().height()`.
**Boucle bas** : quand la viewport est trop petite, le bouton Activer
déborde, n'est pas alloué, donc absent de `used_rect`. La fenêtre reste
petite indéfiniment.

**Fix** : constantes magiques tunées (TITLE_BAR, ROW, etc. dans
`App::apply_dynamic_height`). Solide même si moins élégant. Quand on
ajoute un nouvel élément UI conditionnel (modal, banner...), penser à
ajouter sa contribution dans le calcul.

### `.exe` icon dans Explorer

Dans `app.rc` généré par `build.rs`, utiliser **`1 ICON "icon.ico"`** (ID
numérique littéral), PAS `IDI_ICON1 ICON ...`. `IDI_ICON1` est un
identifiant string non défini dans winuser.h ; Explorer cherche l'icône
par numeric ID minimum et n'en trouve aucun.

### Tray menu items et changement de langue

Les `MenuItem` du tray ont leur label défini à la création
(`MenuItem::new(t!(...))`). Quand l'utilisateur change la langue au
runtime, les items existants ne sont **pas** re-traduits automatiquement.

**Fix** : `TrayController::relocalize()` met à jour show / quit / toggle /
status items explicitement. Le statut est rebuild depuis un cache
`last_status: (bool, usize)` côté `TrayController` pour ne pas dépendre
d'un re-push immédiat depuis `App`.

### `save_atomic` doit `sync_all` avant le rename

Sans flush kernel→disque, une perte d'alimentation entre `write` et
`rename` peut laisser un `.tmp` zéro-byte. Pattern correct dans
`config::save_atomic` :
```rust
let mut file = File::create(&tmp)?;
file.write_all(text.as_bytes())?;
file.sync_all()?;  // ← nécessaire
drop(file);
fs::rename(tmp, path)?;
```

### Hot path hook : AtomicBool pour le check enabled

`HOOK_ENABLED: AtomicBool` lu en `Ordering::Relaxed` au début de
`kb_proc` / `mouse_proc`, **avant** de prendre le mutex `HOOK_STATE`.
Économise une lock par keystroke quand l'app est inactive. Set via
`HookHandle::set_enabled` côté UI. Garder cette fonction lean est
critique : un blocage du Mutex sur le hot path keyboard introduirait un
input lag global sur la machine.

### egui : les API dépréciées sautent à la version suivante

egui 0.35 a supprimé d'un coup tout ce qui était `#[deprecated]`. Un
`#[allow(deprecated)]` laissé dans le code n'est donc pas une dette
neutre, c'est une casse de compilation programmée au prochain bump.
Corollaire : traiter les warnings de dépréciation au moment où ils
apparaissent, pas quand ils bloquent.

Le dropdown de langue utilisait `popup_below_widget` ; il est passé au
builder `Popup::from_toggle_button_response`. ⚠ Ce constructeur gère
lui-même le toggle sur clic — garder en plus un `Popup::toggle_id`
inverserait deux fois et le popup ne s'ouvrirait jamais.

### `title_regex` invalide en config = pas de crash

Le watcher panique si la regex utilisateur ne compile pas. La config
peut être éditée à la main, donc `App::new` valide via
`validate_title_regex` au load et fallback silencieux sur
`DEFAULT_TITLE_REGEX` en mémoire (la prochaine sauvegarde flush `null`
sur disque). Pas de logging fichier, pas de popup — cohérent avec le
"fail soft" du `config::load`.

## Architecture des hooks

`src/hooks/` :
- `state.rs` : `HOOK_STATE: OnceLock<Mutex<HookState>>`,
  `HOOK_ENABLED: AtomicBool`.
- `keyboard.rs` : `kb_proc` (extern "system" callback), `resolve_action`.
- `mouse.rs` : `mouse_proc`.
- `runner.rs` : thread dédié qui appelle `SetWindowsHookExW` + message pump.
- `mod.rs` : `HookHandle` pour exposer une API typée à `App`
  (`set_enabled`, `set_bindings_and_cycle`, `set_cycle_index`).

L'`App` détient un champ `hooks: HookHandle`. La logique pure de remap
du `cycle_index` après reorder est extraite en
`remap_cycle_index_after_reorder` (testable sans toucher aux statics).

## Architecture watcher

`src/win/watcher.rs` est event-driven via `SetWinEventHook`
(`EVENT_OBJECT_CREATE..EVENT_OBJECT_NAMECHANGE`, flag
`WINEVENT_OUTOFCONTEXT`). Coalesce 150 ms pour éviter de rescanner 30×
pendant qu'un Dofus démarre.

`WindowsSnapshot = Arc<ArcSwap<Vec<DetectedWindow>>>` pour des reads
lock-free côté UI / hook callback. Le watcher publie via
`snapshot.store(Arc::new(buf.clone()))`.

### Clients détectés

Deux familles, deux formats de titre, deux exécutables :

| Client | Exécutable | Titre |
|---|---|---|
| Dofus / Expérimental | `Dofus.exe` | `Pseudo - Iop - 2.75.4.13 - Release` |
| Dofus Rétro | `Dofus Retro.exe` | `Pseudo - Dofus Retro v1.48.21` |

**Le nom de l'exe Rétro contient une espace.** `is_dofus_exe` teste les deux
noms ; filtrer sur `Dofus.exe` seul écarte Rétro *avant* la lecture du titre,
donc ajouter une branche à la regex sans toucher au filtre ne produit
strictement rien. C'est le piège de ce module.

Le titre Rétro **ne contient pas la classe** — `DetectedWindow.class` vaut
`None`, et `draw_account_row` réserve alors les 32 px de l'icône sans en
dessiner. Ne pas « optimiser » cette réservation : la frame de la ligne a une
marge verticale de 4 px dimensionnée pour l'icône (contre 11 px pour les
lignes de cycle, qui n'en ont pas), donc une ligne sans réservation tombe à
~26 px au lieu de 40 et fait mentir `ROW` dans `apply_dynamic_height`.

Rétro est une app Electron : ~8 processus par client, mais un seul possède
une fenêtre visible et titrée. Les auxiliaires sont invisibles ou sans titre,
`enum_proc` les écarte sans coût. Avant la sélection d'un personnage, la
fenêtre s'appelle `Dofus Retro v1.48.21` — pas de ` - `, donc pas de match,
donc pas de ligne fantôme.

## Performance baseline

| Métrique | Valeur stable |
|---|---|
| Binary release | ~8.1 Mio |
| RAM working set idle | ~73 Mo |
| RAM private idle | ~66-67 Mo |
| CPU idle | 0 % (mesuré sur 40 s) |
| Latence détection nouvelle fenêtre Dofus | ~150-200 ms |

⚠ Mesurer la RAM **au moins 15 s après le lancement**. Plus tôt, l'atlas
de polices et les textures ne sont pas encore réalisés et on lit ~60 Mo,
ce qui fait conclure à tort à une régression au bump suivant.

La cible originale `< 30 Mo RAM` est **inatteignable** avec
eframe+glow+ICU. Pour viser plus bas il faudrait changer de techno
(Win32+Direct2D natif, ou slint minimaliste).

## Audit supply-chain avant tout bump de deps

crates.io a été touché en 2026 par des campagnes réelles (TrapDoor, ver
Miasma). Le vecteur est le code exécuté **pendant le build** — `build.rs`
et proc-macros — pas le code shippé. Un `cargo update` à l'aveugle n'est
donc pas acceptable.

```powershell
cargo install cargo-audit cargo-deny --locked   # --locked obligatoire
cargo deny check advisories bans sources licenses
cargo audit
```

`deny.toml` est versionné. Sa clé structurante est
`[graph] targets = ["x86_64-pc-windows-msvc"]` : sans elle, l'audit porte
sur le lockfile entier et remonte des advisories de crates jamais
compilés ici — du bruit qui finirait par masquer un vrai problème. Le
lockfile complet fait ~390 paquets, le graphe Windows ~245.

`[sources] unknown-registry/unknown-git = "deny"` est le garde-fou
central : il garantit que toute dépendance vient de crates.io.

Ce que les outils ne couvrent pas et qui doit être fait à la main sur le
diff du lockfile :
- Lister les `build.rs` **nouveaux ou dont la version a changé** dans le
  graphe Windows, et les lire. Les crates déjà présents à version
  inchangée n'ont pas à être relus : Cargo vérifie le checksum du
  tarball au download.
- Grep d'IoC sur ces fichiers : `std::net`, `TcpStream`, `reqwest`,
  `ureq`, `Command::new`, `home_dir`, `.ssh`, `.env`, `keystore`,
  `wallet`, `AWS_`, `GITHUB_TOKEN`, blobs base64, écriture hors
  `OUT_DIR`. Faux positif fréquent : `Command::new(rustc) --version`,
  le sondage de version standard (serde, thiserror, proc-macro2, libc…).
- Exclure `tests/`, `benches/`, `examples/` du scan : jamais compilés
  quand le crate est une dépendance. C'est pourquoi `syn` embarque un
  appel `reqwest` sans que ce soit un problème (dev-dependency, tests de
  round-trip).
- Vérifier la provenance des crates réellement nouveaux via
  `crates.io/api/v1/crates/<nom>` + `/owners` : propriétaire, repo,
  ancienneté, volume de téléchargements. Un crate récent à faible volume
  dont le nom ressemble à un crate populaire = typosquat.

Utile en cross-check sans rien installer : POST sur
`https://api.osv.dev/v1/querybatch` avec la liste extraite de
`cargo tree --target x86_64-pc-windows-msvc`. Deux bases valent mieux
qu'une.

## Pattern de test programmatique

- Trouver les HWNDs : `EnumWindows` + filtre par
  `GetWindowThreadProcessId`. Classe de la tray hidden window =
  `tray_icon_app`, titre du main = `ROrganizer`.
- Simuler un keystroke programmatique : `keybd_event` (P/Invoke)
  **fonctionne** pour fire le LL hook. `SendKeys.SendWait` PowerShell
  **ne déclenche pas** notre hook (mécanisme journal-based bypassé).
- Capturer une fenêtre : `Add-Type` System.Drawing + `CopyFromScreen`
  après `SetWindowPos` HWND_TOPMOST + `SetForegroundWindow` (sinon DWM
  rend du noir pour les surfaces hardware-accelerated).
- Piloter le menu tray : sous Windows 11 la zone de notification n'est
  plus une `ToolbarWindow32`, l'API `TB_GETBUTTON` ne trouve rien. Passer
  par UI Automation — bouton nommé `ROrganizer` dans `Shell_TrayWnd`,
  clic droit dessus, puis énumérer les `MenuItem` du popup.
  Comparer les libellés en **`-ceq` exact** : `-like "*Activer*"` matche
  aussi `Désactiver` (comparaison insensible à la casse).
- Comparer deux screenshots : tester plusieurs décalages verticaux avant
  de conclure. Un bump d'egui peut décaler tout le contenu de 1 px, ce
  qui donne ~11 % de pixels différents pour un rendu par ailleurs
  identique — à `dy=+1` l'écart retombe sous 1 %.

## Tests unitaires

`cargo test` couvre les fonctions pures via des modules
`#[cfg(test)] mod tests` inline (convention Rust standard, pas dans
`tests/`). Cibles principales :
- `triggers` : `vk_to_label`, `key_to_vk` round-trip, `display_label`.
- `config` : round-trip JSON, version mismatch, écriture atomique.
- `hooks::keyboard::resolve_action` : empty bindings, focus aligns
  cursor, cycle wrap, etc.
- `hooks::remap_cycle_index_after_reorder` : remap après reorder /
  shrink / current absent.
- `app::compute_conflicts` : 0/1/N bindings.
- `app::validate_title_regex` : drop d'un regex invalide.
- `app::binding_target_sort_key` : ordre déterministe pour la
  persistence stable.
- `tray::format_status_label` : interpolation et changement de locale.
- `ui::main_view::mix` : mélange gamma-correct (t=0/0.5/1, alpha,
  clamp).
