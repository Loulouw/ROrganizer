# CLAUDE.md

Notes techniques sur le projet pour les contributeurs et les sessions
Claude Code. Pas de prose marketing — uniquement les pièges et décisions
non-évidentes à la lecture du code.

## Projet

Gestionnaire de fenêtres Dofus Unity en Rust (Windows). Permet de focus
n'importe quel compte lancé via raccourcis clavier/souris globaux.

## État actuel

Application fonctionnelle. `cargo test` doit rester vert et `cargo build`
sans warning. À jour à la phase 15 (modal "À propos").

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

- `eframe 0.29` avec `default-features = false, features = ["glow", "default_fonts"]`.
  ⚠ Ne PAS retirer plus de features — tester `default-features = false`
  sur `egui` casse la création de fenêtre (font lookup).
- `egui_extras 0.29` avec uniquement le feature `svg`. Le PNG loader n'est
  PAS compilé : un `Image::from_bytes("bytes://*.png", ...)` rend le
  placeholder rouge d'erreur. Décoder les PNGs via la crate `image` puis
  `ctx.load_texture(...)` à la place. Voir `App::about_icon`.
- `tray-icon 0.19`.
- `windows 0.58` features : `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`,
  `Win32_UI_Input_KeyboardAndMouse`, `Win32_System_Threading`,
  `Win32_System_ProcessStatus`, `Win32_Globalization`, `Win32_Security`,
  `Win32_UI_Accessibility`.
- `raw-window-handle 0.6`.
- `arc-swap 1` pour `WindowsSnapshot` (lecture lock-free hot path).
- `webbrowser 1` pour ouvrir les URLs (évite la console qui flashe avec
  `cmd /C start`).
- `image 0.25` (png only) + `ico 0.3` en build-deps pour générer le `.ico`
  multi-résolution.
- Pas de `tokio`, pas d'`async-runtime`. Threads natifs uniquement.

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

## Performance baseline

| Métrique | Valeur stable |
|---|---|
| Binary release | ~6.8 Mo |
| RAM working set idle | ~65-67 Mo |
| RAM private idle | ~50-53 Mo |
| CPU idle | 0 % (mesuré sur 45 s) |
| Latence détection nouvelle fenêtre Dofus | ~150-200 ms |

La cible originale `< 30 Mo RAM` est **inatteignable** avec
eframe+glow+ICU. Pour viser plus bas il faudrait changer de techno
(Win32+Direct2D natif, ou slint minimaliste).

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
