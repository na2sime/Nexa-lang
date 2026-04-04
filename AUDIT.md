# Audit Global — Nexa-lang
> Généré le 2026-04-03 — NE PAS COMMITER

---

## Score global : 62 / 100

| Dimension | Score |
|---|---|
| Sécurité | 55 / 100 |
| Qualité du code | 60 / 100 |
| Architecture | 80 / 100 |
| Tests | 25 / 100 |
| Infrastructure / CI | 85 / 100 |
| Complétude | 45 / 100 |

---

## 1. Sécurité

### ✅ Ce qui est bien fait

- **Hachage bcrypt** des mots de passe (coût 12) — `crates/registry/src/application/services/auth.rs:43`
- **Tokens API permanents** : 32 bytes aléatoires (`rand`), stockés en SHA-256, format `nxt_<hex>` — `auth.rs:131–138`
- **JWT HS256** avec secret en variable d'environnement — pas hardcodé
- **Parameterized queries** partout via `sqlx` — pas d'injection SQL possible
- **Credentials CLI** jamais loggés — `crates/cli/src/application/credentials.rs:30–36`
- **Images Docker** avec utilisateur non-root (`nexa`) et base pinée (`debian:12-slim`)

---

### 🔴 Critiques

#### S1 — Signature des packages : SHA-256 seulement (pas asymétrique)
**Fichier** : `crates/registry/src/application/services/packages.rs:31–34`

Le hash SHA-256 prouve l'intégrité mais pas l'**authenticité**. N'importe qui avec accès au serveur (ou un serveur compromis) peut recréer un hash valide pour un package malveillant. Le TODO dans le fichier `/TODO` liste "signature asymétrique (Ed25519)" comme item futur — c'est critique pour la sécurité de la supply chain.

**Impact** : compromission silencieuse possible de tous les utilisateurs qui `nexa install`.

**Fix** : Ed25519 — la clé privée signe côté publisher, la clé publique est distribuée avec le compilateur et vérifie à l'install.

---

#### S2 — Pas de rate limiting sur l'API registry
**Fichier** : `crates/registry/src/interfaces/http.rs:109, 122`

Les endpoints `POST /auth/register` et `POST /auth/login` n'ont aucune limitation. Un attaquant peut :
- Bruteforcer des mots de passe
- Flooder la base de données avec des comptes fictifs
- Créer des milliers de tokens API

**Fix** : middleware `tower-governor` ou `axum-governor` — max 10 req/min par IP sur `/auth/*`.

---

#### S3 — Taille des bundles non limitée (DoS)
**Fichier** : `crates/registry/src/interfaces/http.rs:166`

```rust
bundle_bytes: Vec<u8>  // lu entièrement en mémoire, aucune limite
```

Un attaquant peut uploader un ZIP de plusieurs Go, saturant la RAM du serveur.

**Fix** : vérifier `content-length` avant lecture, rejeter > 500 MB.

---

#### S4 — Noms de packages non validés (path traversal potentiel)
**Fichier** : `crates/registry/src/interfaces/http.rs:192`

Le `name` extrait du path URL n'est pas validé avant utilisation. Si le nom est utilisé pour construire des chemins filesystem côté serveur, un `../` peut traverser l'arborescence.

**Fix** : regex `^[a-zA-Z0-9][a-zA-Z0-9._-]{0,213}$` sur tous les inputs `name`.

---

### ⚠️ À revoir

#### S5 — Pas de CORS configuré
`crates/registry/src/interfaces/http.rs` n'a pas de `CorsLayer`. Par défaut Axum ne bloque rien — n'importe quelle origine peut appeler l'API depuis un browser.

#### S6 — Validation email absente
`auth.rs:38` — `register(email, password)` accepte n'importe quelle chaîne comme email, y compris `"a"`.

#### S7 — Messages d'erreur trop verbeux vers le client
`http.rs:152` — `&e.to_string()` renvoie des détails internes (stack, nom de champ DB, format JWT). Loguer en interne, réponse générique au client.

#### S8 — Durée JWT très courte (24h)
Ligne `auth.rs:114`. Pour un développeur qui travaille sur plusieurs jours, oblige à se reconnecter souvent. Passer à 7j ou 30j avec refresh token.

#### S9 — Secret JWT sans validation de longueur
Si `JWT_SECRET=abc` en prod, le token est trivially brute-forceable. Ajouter un check au démarrage : longueur >= 32 caractères.

---

## 2. Qualité du code

### ✅ Ce qui est bien fait

- Architecture Clean (domain / application / infrastructure / interfaces) respectée dans toutes les crates
- Ports & adapters correctement abstraits (`UserStore`, `PackageStore`, `TokenStore`, `SourceProvider`)
- `tracing` utilisé partout pour les logs structurés
- UI CLI propre avec spinner et `ui::die()` centralisé

---

### 🔴 Critiques

#### Q1 — `commands.rs` : fichier monolithique de ~1400 lignes
**Fichier** : `crates/cli/src/application/commands.rs`

Tout y est : init, build, run, publish, install, search, login, register, token, module, config, doctor, theme... C'est une violation directe du principe de responsabilité unique.

**Fix** : scinder en :
```
commands/
  init.rs
  build.rs
  dev.rs        (run, watch)
  registry.rs   (login, register, publish, install, search)
  token.rs
  config.rs
  doctor.rs
```

---

#### Q2 — ~70 `.unwrap()` dans la CLI
Exemples :
- `commands.rs:454` — `.unwrap()` sur extraction ZIP → panic si bundle corrompu
- `commands.rs:635` — `.unwrap()` sur `archive.by_name()`

Un bundle malformé ou un réseau instable fait crasher la CLI au lieu d'afficher une erreur propre.

**Fix** : remplacer par `?` ou `.unwrap_or_else(|e| ui::die(format!("...: {e}")))`.

---

### ⚠️ À revoir

#### Q3 — Pattern bearer token dupliqué 4 fois dans http.rs
Lignes `406–408`, `433–435`, `463–465`... L'extraction du bearer token est répétée inline dans chaque handler. À extraire en middleware ou en extractor Axum.

#### Q4 — Format `.nexa` (NXB) sans versioning évolutif
`crates/compiler/src/lib.rs` — header `b"NXB\x01"`, version hardcodée. Si le format du `Program` AST change, tous les anciens bundles deviennent illisibles sans message d'erreur utile.

**Fix** : ajouter un champ `format_version` dans le manifest du bundle, avec logique de migration ou erreur explicite "bundle trop ancien, recompile".

#### Q5 — Pas de timeout sur les téléchargements CLI
`commands.rs:801–807` — `try_download()` sans timeout. Si le serveur répond lentement, la CLI bloque indéfiniment.

---

## 3. Architecture

### ✅ Ce qui est bien fait

- **Clean Architecture** cohérente sur toutes les crates
- **Workspace Cargo** bien structuré — 4 crates indépendantes
- **Système de modules Nexa** bien pensé : `modules/<name>/src/main|test`, `module.json`, cross-module imports
- **Resolver 5-étapes** pour les imports (relatif → module → lib module → lib projet → cross-module)
- **Pipeline CI/CD** solide avec 3 workflows intelligents (snapshot, release, deploy-registry)

---

### 🔴 Mal conçu

#### A1 — Codegen JS-only sans IR intermédiaire
**Fichier** : `crates/compiler/src/application/services/codegen.rs`

Le compilateur génère directement du JavaScript sans passer par une représentation intermédiaire (IR). Résultat :
- Ajouter WASM = réécrire le codegen entier
- Ajouter du code natif = idem
- Optimisations cross-target impossibles

**Impact** : limitation architecturale bloquante pour la vision long terme.

**Fix** : introduire un IR (type `enum Instruction { ... }`) entre le semantic analyzer et le codegen. Le codegen JS consomme l'IR, et un futur codegen WASM aussi.

---

#### A2 — CLI directement couplée à `nexa_compiler`
`commands.rs` importe `nexa_compiler::*` directement. Tout changement d'API publique du compilateur casse la CLI immédiatement.

**Fix** : fine — acceptable pour l'instant mais à surveiller. À documenter comme "internal API, semver exempt".

---

#### A3 — Multi-module build non implémenté
`project.rs:active_modules()` existe (`#[allow(dead_code)]`) mais n'est appelée nulle part dans le build. Actuellement `nexa build` ne compile que le module principal.

**Conséquence** : un projet multi-module ne compile pas les autres modules.

---

### ⚠️ À revoir

#### A4 — Server et CLI dupliquent la logique HMR/watch
Les deux implémentent du file watching. À extraire dans un crate `nexa-watcher` partagé.

#### A5 — Pas de versioning des endpoints API registry
`/auth/login`, `/packages/:name`... Aucun prefix `/v1/`. Premier breaking change = impact tous les clients CLI déployés.

---

## 4. Reste à faire (roadmap visible)

D'après `/TODO` et l'état du code :

### Fonctionnalités manquantes (critiques)

| Item | Priorité | Notes |
|---|---|---|
| **Multi-module build** | 🔴 P0 | `active_modules()` stub — ne compile qu'un seul module |
| **Signature Ed25519** | 🔴 P0 | SHA-256 seulement en prod — supply chain non sécurisée |
| **Garbage collector** | 🔴 P0 | Pas de GC = fuites mémoire dans les programmes Nexa runtime |
| **Thread / async / coroutine** | 🔴 P0 | Pas de concurrence dans le langage |
| **Standard library (`std`)** | 🔴 P1 | Pas de stdlib → impossible d'écrire des apps réelles |
| **Type system complet** | ⚠️ P1 | Semantic analyzer basique, pas de type inference |
| **Lazy loading** | ⚠️ P1 | Pas d'import dynamique |
| **Encryption** | ⚠️ P1 | Aucune primitive crypto dans le langage |

### Librairies officielles manquantes

| Lib | Rôle |
|---|---|
| `std` | I/O, collections, strings, math |
| `ui-kit` | Composants natifs cross-platform |
| `sql` | Abstraction SQL générique |
| `postgres` | Driver PostgreSQL |
| `supabase` | Client Supabase |
| `mongo` | Driver MongoDB |
| `nexus-orm` | ORM type-safe |

### Infra manquante

- Frontend registry (en Nexa lui-même)
- Dashboard Docker / K8s
- CDN pour packages populaires
- Backup database automatique

---

## 5. Points à finir

| Composant | État actuel | Ce qui manque |
|---|---|---|
| **Lexer** | ✅ Complet | — |
| **Parser** | ✅ Complet | — |
| **Semantic analyzer** | ⚠️ Partiel | Type inference, generics |
| **Optimizer** | ✅ 4 passes | Multi-module optimization |
| **Codegen** | ⚠️ JS seulement | IR, WASM |
| **Package system** | ✅ Fonctionnel | Signature Ed25519 |
| **Module system** | ✅ Fonctionnel | Build multi-module |
| **Registry** | ✅ Fonctionnel | Rate limiting, CORS, bundle size |
| **CLI** | ✅ Fonctionnel | Error handling, tests |
| **Server (dev)** | ✅ Basique | HMR amélioré |

---

## 6. Tests

### État actuel

| Crate | Tests | Verdict |
|---|---|---|
| `nexa-compiler` | 11 tests (optimizer, packager, lib) | Suffisant pour les passes, absent pour lexer/parser |
| `nexa` (CLI) | 24 tests (project.rs, updater) | Zéro test sur les commands elles-mêmes |
| `nexa-registry` | 0 tests | **Critique** — auth, tokens, packages non testés |
| `nexa-server` | 0 tests | Acceptable (dev only) |

### Manques critiques

1. Pas de tests pour `AuthService::register/login/verify_token`
2. Pas de tests pour `PackagesService::publish/install`
3. Pas de tests d'intégration `nexa init → nexa build → nexa publish`
4. Pas de tests de sécurité (bundle malformé, token invalide, input injection)
5. Pas de tests pour la résolution d'imports cross-module

---

## 7. Futur — Limitations architecturales à anticiper

### F1 — Le compilateur JS-only va bloquer l'adoption
Sans IR et sans support WASM/natif, Nexa ne peut viser que le web front-end. Pour des apps serveur, mobile ou CLI natives, il faudra une refonte majeure du codegen.

### F2 — Absence de GC va devenir bloquante
Actuellement les programmes Nexa compilés en JS utilisent le GC de V8. Mais si Nexa vise un runtime propriétaire (WASM bare metal, natif), il faut un GC. C'est un chantier de 6–12 mois minimum.

### F3 — Pas de type inference = langage limité
Le semantic analyzer actuel fait des vérifications de base mais pas de vrai type inference (Hindley-Milner ou similaire). Pour des génériques, des closures typées, ou des traits, il faut refactorer le système de types.

### F4 — Registry monolithique sans CDN
Quand les packages populaires auront 10 000+ downloads/jour, un seul VPS ne tiendra pas. Prévoir un CDN (Cloudflare R2, S3) pour le stockage des bundles.

### F5 — Pas de lockfile au niveau compilateur
`nexa install` crée un lockfile de dépendances, mais le compilateur lui-même n'a pas de mécanisme pour épingler la version de résolution d'un build reproductible.

### F6 — Versioning API registry
Aucun prefix `/v1/`. Le premier breaking change forcera une migration douloureuse de tous les clients CLI en production.

---

## Priorités recommandées

### Immédiat (avant toute communication publique)
1. **Rate limiting** sur `/auth/*` — 1 jour
2. **Validation des noms de packages** — quelques heures
3. **Limite taille bundle** upload — 1h
4. **CORS** sur la registry — 2h

### Court terme (v0.2)
1. **Multi-module build** — finir l'implémentation `active_modules()`
2. **Tests registry** — AuthService, PackagesService
3. **Error handling CLI** — remplacer `.unwrap()` par gestion propre
4. **Signature Ed25519** pour les packages

### Moyen terme (v0.3–0.5)
1. **Standard library** minimale (std)
2. **Type inference** (Hindley-Milner)
3. **IR intermédiaire** dans le compilateur
4. **Lazy loading**

### Long terme (v1.0)
1. Garbage collector
2. Thread / async / coroutines
3. WASM target
4. Registry frontend en Nexa
