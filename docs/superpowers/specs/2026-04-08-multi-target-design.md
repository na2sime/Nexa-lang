# Nexa — Multi-cibles & multi-types d'applications

> **Spec validée — 2026-04-08**

---

## 1. Vision

Nexa passe d'un compilateur web-only à un langage universel produisant des artefacts natifs selon le type déclaré dans `module.json`.

| Type | Artefact produit | Statut |
|---|---|---|
| `web` | Bundle `.nexa` (ZIP + AST + sig Ed25519) | ✅ Existant |
| `package` | Bundle `.nexa` distribué via registry | ✅ Existant |
| `backend` | Binaire Rust natif | Phase 1 |
| `cli` | Binaire Rust natif | Phase 1 |
| `desktop` | Shell Rust + CEF → `.app` / `.exe` / AppImage | Phase 2-3 |
| `mobile` | À définir | Phase 6 |

### Décisions architecturales définitives

- `type` et `platforms` vivent dans `module.json`, jamais dans le `.nx`
- `backend`/`cli` → binaire Rust natif (pas Node.js)
- `desktop` → CEF (Chromium embarqué, rendu 100% identique cross-platform)
- `type` absent dans `module.json` → `web` par défaut (rétro-compat totale)
- Source `.nx` : aucun changement de syntaxe dans aucune phase

---

## 2. Références partagées

### Platforms disponibles

| Valeur | Cible | Types concernés |
|---|---|---|
| `browser` | Web (bundle .nexa) | `web` |
| `native` | OS courant au build | `backend`, `cli` |
| `native-macos` | Cross-compile macOS | `backend`, `cli` |
| `native-windows` | Cross-compile Windows | `backend`, `cli` |
| `native-linux` | Cross-compile Linux | `backend`, `cli` |
| `macos` | App macOS (CEF) | `desktop` |
| `windows` | App Windows (CEF) | `desktop` |
| `linux` | AppImage/deb (CEF) | `desktop` |
| `ios` | Mobile (futur) | `mobile` |
| `android` | Mobile (futur) | `mobile` |

**Defaults si `platforms` absent dans `module.json` :**

| Type | Default |
|---|---|
| `web` | `["browser"]` |
| `backend` | `["native"]` |
| `cli` | `["native"]` |
| `desktop` | `["macos"]` |
| `package` | aucune (agnostique) |

### Disponibilité stdlib par type de module

| Module | `web` | `backend` | `cli` | `desktop` | `package` |
|---|---|---|---|---|---|
| `std.io.Console` | ✅ | ✅ | ✅ | ✅ | ❌ |
| `std.io.File` | ❌ | ✅ | ✅ | ✅ (IPC) | ❌ |
| `std.math.*` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `std.str.*` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `std.collections.*` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `std.async.*` | ✅ | ✅ | ✅ | ✅ | ❌ |
| `std.net.HttpClient` | ✅ | ✅ | ✅ | ✅ | ❌ |
| `std.net.Socket` | ✅ | ✅ | ❌ | ✅ | ❌ |
| `std.server.HttpServer` | ❌ | ✅ | ❌ | ❌ | ❌ |
| `std.process.Process` | ❌ | ✅ | ✅ | ❌ | ❌ |
| `std.process.Env` | ❌ | ✅ | ✅ | ❌ | ❌ |
| `std.desktop.*` | ❌ | ❌ | ❌ | ✅ | ❌ |

**Implémentation stdlib selon la cible :**
- `web` / `desktop` → JS helpers (runtime string dans `codegen.rs`)
- `backend` / `cli` → Rust natif via crates (axum, reqwest, tokio…)
- `package` → dépend du type du module consommateur

### Mapping types Nexa → Rust

| Nexa | Rust |
|---|---|
| `Int` | `i64` |
| `String` | `String` |
| `Bool` | `bool` |
| `Void` | `()` |
| `List<T>` | `Vec<T>` |
| `(A) => B` | `impl Fn(A) -> B` |
| classe Nexa | `struct` + `impl` |
| `async fn` | `async fn` + tokio |

### Crates Rust pour stdlib native

| Stdlib Nexa | Crate Rust | Rôle |
|---|---|---|
| `std.server.HttpServer` | `axum` | Serveur HTTP async |
| `std.net.HttpClient` | `reqwest` | Client HTTP |
| `std.net.Socket` | `tokio-tungstenite` | WebSocket |
| `std.io.File` | `tokio::fs` | I/O fichier async |
| `std.io.Console` | `println!` | Output terminal |
| `std.process.Process` | `std::process` | Exit, PID… |
| `std.process.Env` | `std::env` | Variables d'environnement |
| `std.async.*` | `tokio::sync` | Future, channel |
| `std.collections.*` | `std::collections` | HashMap, Vec… |

---

## 3. Phase 0 — Acquis (déjà implémenté)

- Web → HTML+JS → bundle `.nexa` (`nexa package` / `nexa publish` / registry)
- WASM codegen
- Stdlib complète (io, math, str, collections, async, net, server, process)
- Système de modules (`modules/<name>/module.json`)
- Build incrémental (lockfile SHA-256)
- 273 tests, 0 failures

---

## 4. Phase 1 — Extension `module.json` + cible Backend/CLI Rust

**Scope :** Étendre `ModuleConfig` avec `type`/`platforms`, brancher un dispatcher de build, et produire un binaire Rust natif pour les modules `backend` et `cli`.

**Prérequis :** Phase 0 complète (✅ déjà fait).

### Changements

**`crates/cli/src/application/project.rs`**
- Ajouter `AppType` (`Web` default, `Backend`, `Cli`, `Desktop`, `Package`)
- Ajouter `Platform` (voir Section 2)
- Ajouter `DesktopConfig` (utilisé Phase 2, défini ici)
- Étendre `ModuleConfig` avec `type`, `platforms`, `desktop`, `version`
- Ajouter `ModuleConfig::effective_platforms()`

**`crates/cli/src/application/commands/build.rs`**
- Remplacer l'appel direct `compile_project_file` par `build_module(proj, mod_name)`
- `build_module` : compile vers IR une fois, dispatch par platform via `effective_platforms()`
- Sortie : `dist/<module>/<platform>/` — **breaking change sur le chemin de sortie** : `web` passe de `dist/<module>/` à `dist/<module>/browser/`. Les sources `.nx` sont inchangées, mais les scripts CI ou tooling qui référencent `dist/<module>/` directement devront être mis à jour.

**Nouveau : `crates/cli/src/application/targets/`**
- `dispatcher.rs` — boucle `par_iter` sur platforms (Rayon)
- `web.rs` — extrait de `build.rs` actuel (bundle `.nexa`)
- `rust.rs` — nouveau : `codegen_rust` + `cargo build`
- `desktop.rs` — stub Phase 1 (implémenté Phase 2)
- `package.rs` — extrait de `build.rs` actuel

**Nouveau : `crates/compiler/src/application/services/codegen_rust.rs`**
- Transpile IR → `main.rs` Rust
- Le champ `routes` de `IrModule` est ignoré pour `backend`/`cli` (web-specific) — `main()` dans l'app block devient le `fn main()` Rust avec runtime tokio
- Génère `Cargo.toml` avec crates stdlib selon les imports détectés
- Mapping types et crates : voir Section 2

**`crates/cli/src/application/commands/init.rs` + `module.rs`**
- `nexa new --type backend` → génère `module.json` avec `type` + `platforms` pré-remplis
- `nexa module add --type backend --platforms native-linux,native-macos`

### Sortie attendue

```
dist/
  api/
    native-linux/       ← binaire Rust
  web/
    browser/            ← bundle .nexa (identique à aujourd'hui)

.nexa/
  nex_out/              ← source Rust intermédiaire (gitignored)
    api/
      native-linux/
        src/main.rs
        Cargo.toml
  compile/
    logs/               ← logs de compilation par module × platform (gitignored)
      api-native-linux.log
      web-browser.log
```

### Done when...
- [ ] `nexa build` sur un module `type: backend` produit un binaire exécutable dans `dist/api/native-linux/`
- [ ] `nexa build` sur un module `type: web` existant fonctionne sans aucune modification du projet (rétro-compat)
- [ ] `nexa new my-api --type backend` génère un `module.json` correct
- [ ] Logs de compilation écrits dans `.nexa/compile/logs/<module>-<platform>.log`
- [ ] Les tests existants (273) passent toujours

---

## 5. Phase 2 — Desktop macOS (CEF)

**Scope :** Produire une application `.app` macOS à partir d'un module `type: desktop` en embarquant Chromium (CEF) comme moteur de rendu.

**Prérequis :** Phase 1 complète.

### Changements

**`crates/cli/src/application/targets/desktop.rs`** (stub Phase 1 → implémentation)
- Flow complet : Nexa → HTML+JS → shell Rust CEF → `.app`
- Télécharge/cache les binaires CEF dans `~/.nexa/cef/<version>/` au premier build

**Nouveau : `crates/compiler/src/application/services/codegen_desktop.rs`**
- Génère le shell Rust CEF : scheme `nexa://` + assets HTML+JS embarqués via `include_dir!`
- Génère `Cargo.toml` avec dépendance `cef-sys`
- Injecte le bridge IPC (`window.__NEXA_IPC__`) dans le bundle web

**`module.json` — champ `desktop`** (struct déjà définie Phase 1)
```json
{
  "type": "desktop",
  "platforms": ["macos"],
  "desktop": {
    "title": "MonApp",
    "width": 1200,
    "height": 800,
    "resizable": true,
    "icon": "assets/icon.png"
  }
}
```

### Sortie attendue

```
dist/
  desktop-app/
    macos/
      MonApp.app/
        Contents/
          MacOS/MonApp  ← binaire
          Resources/    ← libcef.dylib + Chromium resources
          Info.plist

.nexa/
  nex_out/
    desktop-app/
      macos/
        src/main.rs
        Cargo.toml
  compile/
    logs/
      desktop-app-macos.log
```

### Done when...
- [ ] `nexa build` sur un module `type: desktop, platforms: ["macos"]` produit un `.app` ouvrable sur macOS
- [ ] La fenêtre affiche le bundle HTML+JS Nexa via le scheme `nexa://`
- [ ] Les assets sont embarqués dans le binaire (pas de serveur HTTP local)
- [ ] Le cache CEF est réutilisé entre builds (`~/.nexa/cef/<version>/`)
- [ ] Phase 1 reste entièrement fonctionnelle

---

## 6. Phase 3 — Desktop Windows + Linux

**Scope :** Étendre la cible desktop aux plateformes Windows et Linux avec packaging natif pour chacune.

**Prérequis :** Phase 2 complète.

### Changements

**`crates/cli/src/application/targets/desktop.rs`**
- Ajoute le packaging Windows : `.exe` + `libcef.dll` + NSIS installer
- Ajoute le packaging Linux : AppImage + `.deb`
- Cross-compilation via Rust targets + SDK Xcode (macOS) / MSVC (Windows)

**`.github/workflows/`**
- Ajoute une matrix CI `[macos-latest, windows-latest, ubuntu-latest]` pour les builds desktop
- Artefacts uploadés par plateforme

### Sortie attendue

```
dist/
  desktop-app/
    macos/
      MonApp.app
    windows/
      MonApp.exe
      libcef.dll
    linux/
      MonApp.AppImage
      MonApp.deb
```

### Done when...
- [ ] `nexa build` avec `platforms: ["macos", "windows", "linux"]` produit les 3 artefacts
- [ ] L'AppImage Linux est auto-exécutable (pas d'installation requise)
- [ ] L'installer Windows fonctionne via NSIS
- [ ] La CI GitHub Actions build et upload les 3 artefacts en parallèle

---

## 7. Phase 4 — Native Bridge + std.desktop

**Scope :** Implémenter le bridge IPC complet JS ↔ Rust et exposer les APIs natives via `std.desktop.*`.

**Prérequis :** Phase 3 complète.

### Changements

**`codegen_desktop.rs`** — IPC dispatcher Rust complet

```rust
match cmd {
    "notify.show"     => { /* notification native OS */ }
    "fs.read"         => { /* tokio::fs::read */ }
    "fs.write"        => { /* tokio::fs::write */ }
    "dialog.open"     => { /* file picker natif */ }
    "dialog.save"     => { /* save dialog natif */ }
    "clipboard.read"  => { /* lecture presse-papier */ }
    "clipboard.write" => { /* écriture presse-papier */ }
    "shell.openUrl"   => { /* ouvre URL dans browser système */ }
    "shell.openPath"  => { /* ouvre fichier/dossier */ }
    "window.setTitle" => { /* titre de la fenêtre CEF */ }
    "window.minimize" => { /* minimiser */ }
    "window.maximize" => { /* maximiser */ }
    "tray.setIcon"    => { /* icône dans la barre système */ }
    _ => { /* cmd inconnue — log dans .nexa/compile/logs */ }
}
```

**Nouveau : `stdlib/src/desktop/`**
- `notify.nx` — `std.desktop.Notify` : `show(title, body)`
- `dialog.nx` — `std.desktop.Dialog` : `openFile()`, `saveFile()`, `pickFolder()`
- `clipboard.nx` — `std.desktop.Clipboard` : `write(text)`, `read() => String`
- `tray.nx` — `std.desktop.Tray` : `setIcon(path)`, `onClick(fn)`
- `window.nx` — `std.desktop.NativeWindow` : `setTitle(s)`, `minimize()`, `maximize()`
- `shell.nx` — `std.desktop.Shell` : `openUrl(url)`, `openPath(path)`

**`codegen.rs`** — helper IPC injecté dans tous les bundles desktop :
```javascript
function _ipcInvoke(cmd, payload) {
    if (window.__NEXA_IPC__) {
        window.__NEXA_IPC__.postMessage(JSON.stringify({ cmd, payload }));
    }
}
```

### Done when...
- [ ] Un module desktop peut appeler `std.desktop.Notify.show("titre", "corps")` et déclenche une notification OS native
- [ ] `std.desktop.Dialog.openFile()` retourne le chemin sélectionné via IPC
- [ ] `std.desktop.Clipboard` lit et écrit le presse-papier système
- [ ] Les cmds inconnues sont loggées sans crash
- [ ] La stdlib desktop est documentée dans `stdlib/src/desktop/`

---

## 8. Phase 5 — Package registry étendu

**Scope :** Étendre `nexa publish` / `nexa install` pour supporter `type: package` avec versioning sémantique et résolution de dépendances transitives.

**Prérequis :** Phase 1 complète (`type: package` dans `module.json`).

### Changements

**`crates/cli/src/application/commands/registry.rs`**
- `nexa package` : lit `version` depuis `module.json`, génère le bundle `.nexa`
- `nexa publish` : envoie le bundle avec `type: package` + `version` dans les métadonnées
- `nexa install <name>@<version>` : supporte la syntaxe `@version` explicite
- `nexa install` (sans version) : résout la dernière version compatible semver

**`crates/registry/`** — extensions backend
- Stocke `type` et `version` dans les métadonnées du bundle
- API : `GET /packages/<name>/versions` → liste des versions disponibles
- Résolution semver côté registry : `^1.0.0`, `~1.2.0`, `>=1.0.0`

**`crates/compiler/src/application/services/resolver.rs`**
- Résolution des dépendances transitives : si `my-lib` dépend de `utils@^2.0`, `utils` est installé automatiquement
- Ordre de résolution (inchangé) :
  1. `stdlib/` — packages officiels `std.*`
  2. `~/.nexa/packages/` — installés depuis le registry
  3. `modules/<name>/lib/` — dépendances locales du module
  4. `lib/` — dépendances projet-level

### Done when...
- [ ] `nexa publish` sur un module `type: package` uploade le bundle avec version et métadonnées correctes
- [ ] `nexa install my-lib@1.2.0` installe la version exacte
- [ ] `nexa install my-lib` installe la dernière version compatible
- [ ] Les dépendances transitives sont résolues et installées automatiquement
- [ ] `nexa install` sur un projet existant `type: web` fonctionne toujours sans changement

---

## 9. Phase 6 — Mobile iOS & Android (vision future)

**Scope :** Cibler iOS et Android. Phase en vision — aucune décision technique définitive, pas d'implémentation planifiée.

**Prérequis :** Phase 4 complète (patterns IPC établis).

### Approches envisagées

| Plateforme | Shell | Rendu | Bridge IPC |
|---|---|---|---|
| iOS | Swift | WKWebView | `WKScriptMessageHandler` |
| Android | Kotlin | WebView | `addJavascriptInterface` |

Les patterns établis en Phase 4 (IPC dispatcher, assets embarqués, scheme custom) seront réutilisés tels quels.

### Done when...
- [ ] *(À définir lors du démarrage de cette phase)*
