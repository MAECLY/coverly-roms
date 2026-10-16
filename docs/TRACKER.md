# TRACKER — Coverly ROMs

> Documento vivo. **Si retomas tras un compact, empieza aquí.**
> Fuente de verdad del diseño: [`docs/superpowers/specs/2026-10-15-coverly-roms-design.md`](superpowers/specs/2026-10-15-coverly-roms-design.md).
> Regla: una tarea solo pasa a ✅ con **evidencia** (commit, comando + salida, archivo, URL).
> Estados: ⬜ pendiente · 🟡 en curso · ✅ hecho · ⛔ bloqueado · ⏸️ pospuesto

## Estado actual (actualizar en cada sesión)

- **Fecha:** 2026-10-16
- **Fase:** M2 terminado en `feat/m2-fill` (plan `2026-10-16-m2-identify-and-fill.md`, tareas 1–14). `coverly fill` + `coverly undo` validados de punta a punta sobre una copia de la SD y sobre una imagen FAT32 (ver «Evidencia M2»).
- **Siguiente paso:** PR de `feat/m2-fill` → CI → merge; después plan M3 (explorador en la app).
- **En curso:** nada en segundo plano.

## Contexto clave (para no perderlo)

- Repo local: `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms` · remoto privado `MAECLY/coverly-roms` (hacer público antes de la 1ª release, ADR-0008).
- SD real de pruebas: `/Volumes/System/ROMS` (firmware **Anbernic stock**, portadas en `ROMS/<SIS>/Imgs/<stem>.png`, ~8.5k PNG existentes). **Nunca escribir en la SD sin respaldo y aprobación.** Probar sobre una copia.
- Repos de referencia MAECLY: `pluryprint` (React, release/updater completos — plantilla principal) y `versorium-app` (Svelte, i18n es/en, keychain).
- Licencia/marca: AGPL-3.0-only + §7(e) + CLA + TRADEMARKS (igual que Pluryprint/Versorium).
- Herramientas existentes verificadas (2026-10-15): mini-scraper / mini-scraper-cfw, ROMCover, BoxArt (Mac); Skraper, ARRM, Retro CardMaker (Windows); Skyscraper (Linux/CLI). Detalle: `docs/02-research/`.

## Decisiones tomadas (con fecha)

| # | Decisión | Fecha | Origen |
|---|---|---|---|
| D1 | Nombre **Coverly ROMs**, repo `coverly-roms` en org MAECLY | 2026-10-15 | Miguel |
| D2 | Uso público, misma licencia y esquema de marca que Pluryprint/Versorium | 2026-10-15 | Miguel |
| D3 | Stack React como Pluryprint (Tauri 2 + Rust + React/TS) | 2026-10-15 | Miguel |
| D4 | Estrategias bulk seleccionables: Prioridad configurable, Región de la ROM, Popularidad real | 2026-10-15 | Miguel |
| D5 | Media v1: portada + screenshot + título | 2026-10-15 | Miguel |
| D6 | UI en español + inglés | 2026-10-15 | Miguel |
| D7 | Arquitectura A: núcleo Rust compartido + app Tauri + CLI | 2026-10-15 | Miguel |
| D8 | Documentar todo primero; TRACKER como memoria anti-compact | 2026-10-15 | Miguel |
| D9 | Estrategia Popularidad = (a) votos SteamGridDB donde existan + (b) consenso entre fuentes/regiones para consolas; fallback a Prioridad | 2026-10-15 | Claude (recomendación; Miguel dijo EMPIEZA sin objetar) |
| D10 | PRs en inglés, sin firma Claude; auto-merge autorizado tras verificar; E2E con Chrome DevTools MCP / Claude-in-Chrome | 2026-10-15 | Miguel |
| D11 | Caché SQLite (M1-05) se mueve a M2 (no necesaria para auditar) | 2026-10-15 | Claude |
| D12 | Respaldos y journals de los jobs en el directorio de datos de la app (no en la SD), para que la consola no vea carpetas extra ([ADR-0010](adr/0010-safe-writes-backups-and-journal.md)) | 2026-10-16 | Claude (plan M2) |
| D13 | ROMs `.7z` se identifican por nombre en M2 (CRC interno de 7z diferido: dependencia pesada) | 2026-10-16 | Claude (plan M2) |
| D14 | `coverly fill --apply` escribe también los ítems `review` salvo `--auto-only` (spec §7.7); el dry-run siempre muestra cuántos son `review` | 2026-10-16 | Claude (plan M2) |
| D15 | Sin conexión / timeout pausa como un 403/429 (spec §10). Popularidad: consenso = listados de media de libretro hasta M6 (votos SteamGridDB y consenso entre fuentes llegan con M6) | 2026-10-16 | Claude (plan M2) |

## Fase 0 — Descubrimiento y diseño

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| P0-01 | Investigar herramientas existentes para Mac | ✅ | Workflow `wf_d783f84f-48a` (69 agentes, 63 herramientas verificadas) |
| P0-02 | Crear carpeta y repo privado vacío | ✅ | `gh repo view MAECLY/coverly-roms` → PRIVATE, isEmpty:true |
| P0-03 | Estudiar Top1 Windows (Skraper) y Linux (Skyscraper) con evidencia | ✅ | `docs/02-research/skraper.md` (30 claims), `skyscraper-architecture.md` (34), `skyscraper-output-config.md` (33) |
| P0-04 | Documentar fuentes: libretro-thumbnails, ScreenScraper API, SteamGridDB | ✅ | `docs/02-research/libretro-thumbnails.md`, `screenscraper-api.md` |
| P0-05 | Documentar layouts de firmwares/frontends | ✅ | `docs/02-research/layouts.md` |
| P0-06 | Stack de updater cross-platform (repo privado vs público) | ✅ | `docs/02-research/desktop-stack-updater.md` → releases requieren repo público |
| P0-07 | Escribir spec maestra | ✅ | `docs/superpowers/specs/2026-10-15-coverly-roms-design.md` |
| P0-08 | Mostrar evidencia de investigación a Miguel | 🟡 | resumen en chat 2026-10-15 |
| P0-09 | Volcar investigación en `docs/02-research/*.md` | ✅ | commit `3d6e1f1`; spec §16 con refinamientos |
| P0-10 | Completar docs 01–09 + ADRs desde la spec | 🟡 | `1455f8a`: `docs/README.md`, `docs/04-architecture/system-architecture.md`, ADRs 0001–0008 (`docs/adr/`). Faltan 01, 03, 05, 06, 08 |
| P0-11 | Revisión de spec por Miguel | ✅ | "EMPIEZA" 2026-10-15 |
| P0-12 | Plan de implementación M0+M1 (writing-plans) | ✅ | `docs/superpowers/plans/2026-10-15-m0-m1-foundation-and-audit.md` |
| P0-13 | Diseño de marca (wordmark + mark/icono) | ⬜ | |

## M0 — Fundación del repo

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M0-01 | LICENSE (AGPL-3.0), LICENSES/, TRADEMARKS.md, CLA.md, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, THIRD-PARTY-NOTICES | ✅ | `f594529` (licencia, marca, CLA, comunidad); `a4cd744` (plantilla PR con confirmación CLA) |
| M0-02 | Workspace Cargo (`coverly-core`, `coverly-cli`) + `src-tauri` + React/Vite/Tailwind (plantilla Pluryprint) | ✅ | `85c5dbc` (workspace `coverly-core` + `coverly-cli`); `29c59e0` (Tauri 2 + React, comando `app_info`) |
| M0-03 | Tooling: pnpm, eslint, prettier, commitlint, husky, Makefile | ✅ | `122c1b8` (vite, eslint, prettier, commitlint, husky); `216feab` (Makefile) |
| M0-04 | CI (`ci.yml`) matriz 3 SO: fmt, clippy, tests, lint | ✅ | `fb47c00` (`ci.yml`: frontend en ubuntu; fmt/clippy/tests Rust en Linux, Windows, macOS) |
| M0-05 | i18n base en/es | ✅ | `5ca59a2` (locales en/es); `2d08171` (atributo `lang` sincronizado) |

## M1 — Escaneo y auditoría

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M1-01 | `systems.toml` (alias, extensiones, playlist libretro, DATs, header-skip) | ✅ | `3aeb19f`, `9580295`, `bf567d6` — 54 sistemas, alias case-insensitive, overrides TOML de usuario, playlist libretro. DATs y header-skip → M2-01/M2-02 |
| M1-02 | Perfiles built-in + autodetección + TOML de usuario | ✅ | `fe1913d` — 8 perfiles built-in, plantillas de ruta, autodetección (SD real → `anbernic-stock`). Perfiles TOML de usuario → M5-03 |
| M1-03 | Walker + clasificación ROM (multi-archivo, m3u, cue/bin, carpetas) + ignorar `._*` | ✅ | `b091445` — sistemas + entradas, cue/bin, m3u, gdi, ignora `._*`/ocultos, avisos sin abortar |
| M1-04 | ArtStatus por media (present/missing/orphan) | ✅ | `25be5d8`, `b26b870`, `acb9b7a`, `4209ab3`, `474cbed`, `85c72f5` — present/missing/unsupported + huérfanas, NFC, sufijos Batocera |
| M1-05 | Caché SQLite de escaneo | ✅ | Hecha en M2 (D11): `a6381ae` (caché SQLite: quick ids, índices, filas DAT, base corrupta apartada y recreada), `be5eff0` (helpers de tiempo). Auditar no la necesita (1,69 s la SD entera, ver M1-07); el hashing de M2 sí |
| M1-06 | `coverly audit` (tabla, CSV, JSON) | ✅ | `53076bb` (informe JSON/CSV); `3f573e3` (`coverly audit`, `profiles`, `systems`; códigos 0/2/3/4) |
| M1-07 | Validar contra SD real (solo lectura) | ✅ | Ver «Evidencia M1-07» abajo: 7473 ROMs, 52 sistemas, recuento manual de SFC/GB/WS/PS coincide exactamente; SD sin tocar (montada `noatime`, solo lecturas) |

### Evidencia M1-07 — auditoría de la SD real (solo lectura, 2026-10-15)

Comandos (binario release; salidas en el scratchpad, nunca en la SD):

```
cargo build -p coverly-cli --release
/usr/bin/time -p target/release/coverly audit /Volumes/System --format json --output <scratchpad>/sd-audit.json
/usr/bin/time -p target/release/coverly audit /Volumes/System --format table
```

- SD: `/Volumes/System` (volumen `msdos`, montado `nodev,nosuid,noowners,noatime`). Ninguna ruta de `scan`/`art`/`audit` abre ficheros para escribir (solo los tests escriben, en temp dirs).
- Perfil autodetectado: `anbernic-stock (auto-detected from anbernic/ folder, Imgs folders)`. 0 avisos.
- Totales: **52 sistemas conocidos, 3 desconocidos omitidos (`APPS`, `JAVA`, `PORTS`), 7473 ROMs, 3080 sin portada, 4393 completas, 2061 imágenes huérfanas**. Screenshot/título: `-` (Anbernic stock tiene un único hueco, portada).
- Tiempos: 1ª ejecución de la sesión **1,69 s** reales; repeticiones 0,17 s (tabla), 0,43 / 0,35 / 0,31 s (JSON). `find ROMS -type f` = 22 607 ficheros.
- Sistemas con portadas faltantes: FC 590/631, GB 18/363, GBA 738/760, GG 67/107, MD 421/460, PS 77/124, SFC 991/1021, SMS 134/182, WS 44/63.

Contraste manual con `ls` (ROMs = ficheros no ocultos fuera de `Imgs`; portadas = stems de `Imgs/*.png|jpg`; `comm` entre ambos):

| Sistema | ROMs (ls) | Imgs | Sin portada (ls) | Coverly ROMs / sin portada |
|---|---|---|---|---|
| SFC | 1021 (967 `.smc` + 54 `.sfc`) | 30 | 991 | 1021 / 991 ✅ |
| GB | 363 `.gb` | 739 | 18 (394 imágenes sin ROM) | 363 / 18 ✅ |
| WS | 63 `.wsc` | 19 | 44 | 63 / 44 ✅ |
| PS | 124 (88 `.PBP` + 19 `.bin` + 16 `.img` + 1 `.iso`; 6 `.srm` excluidos) | — | — | 124 ✅ |

## M2 — Identificación, fuentes y relleno

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M2-01 | Hash CRC32 + header skip + límites de tamaño | ✅ | `20f16cf` (CRC32/MD5/SHA1 en una pasada, header skip, CRC de zip desde el directorio central, tope 64 MiB), `697f69d` (regla de cabecera por sistema). Doc: [identification.md](04-architecture/identification.md) |
| M2-02 | Índice DAT (libretro-database) + mapa arcade MAME/FBNeo | ✅ | `b89892d`, `e78c59b`, `4ae1e6d`, `ef9632c`, `e2e16f8` (rutas reales de Atari 8-bit y Game & Watch, halladas con el chequeo HEAD del E2E) |
| M2-03 | Normalización de nombres + parseo de región + fuzzy | ✅ | `21a2e62` (golden tests en `names_golden.rs`) |
| M2-04 | Fuente libretro (índice por sistema, caché, descarga) | ✅ | HTTP: `edc4d15`, `655f982`, `25c9c5c`; fuente: `946385c`, `0bd4eab` (entidades HTML en los href de Apache: `Tom &amp; Jerry`, hallado en el E2E); gather (tarea 7): `8507ef1`, `8827dd3`, `7d9b3bb`, `1d3b15e`, `a71c851`, `0dc1800`, `ce7bcfb`. Doc: [art-sources.md](04-architecture/art-sources.md) |
| M2-05 | Estrategias: Prioridad, Región ROM, Popularidad | ✅ | `b14a055` (D9/D15) |
| M2-06 | Plan (auto/revisar/sin resultado) + dry-run | ✅ | `5a38eb9` |
| M2-07 | Imagen: decode/resize/encode por perfil | ✅ | `67e4bca` (límites: 20 MiB, 8192 px, 256 MiB de asignación; formato por sniffing) |
| M2-08 | Escritura atómica, respaldo, journal, undo, nombres FAT-safe | ✅ | `c594bf4`, `90874a2` (borra los `._*` que macOS crea en FAT por `com.apple.provenance`, hallado en el E2E sobre FAT32). [ADR-0010](adr/0010-safe-writes-backups-and-journal.md) |
| M2-09 | `coverly fill` (dry-run/apply/--auto-only) | ✅ | `321e001` (aplicar plan: art store, fallbacks, pausa, cancelar), `63896d4` (`coverly fill` y `coverly undo`). E2E: «Evidencia M2» abajo. Doc: [fill-pipeline.md](04-architecture/fill-pipeline.md) |
| M2-10 | Batocera/Knulli: fusionar `gamelist.xml` al rellenar (diferido de M2; hoy `fill` avisa) | ⬜ | M4/M5 |
| M2-11 | Selección de variantes: hoy un hack/beta puede ganar como `auto` cuando es el único título exacto o el mejor fuzzy (GBA `Final Fantasy IV` → caja «Sound Restoration hack», `Contra Advance` → `(Beta)`): penalizar o mandar a `review` | ⬜ | Visto en el dry-run del E2E (abajo) |
| M2-12 | Perfil Anbernic stock: las portadas existentes de la SD miden 285×184, fuera de la caja 250×360 del perfil → revisar el tamaño máximo con la consola real | ⬜ | Visto en el E2E (22 portadas GBA originales) |

### Evidencia M2 — relleno E2E sobre copia de la SD (2026-10-16)

Binario release, servidores reales, todo en el scratchpad (`$SC` = `/private/tmp/claude-501/…/scratchpad`). Origen: copia local completa de la SD en `~/Documents/ROMS`, solo leída con `cp -X`. **Nada escrito bajo `/Volumes`.**

- **Copia:** `head -40` de `ROMS/GBA` + `head -150` de `ROMS/FC` + todas sus `Imgs` → `$SC/sd-copy` (con `anbernic/`): **475 MB, 253 ficheros**; auditoría: 2 sistemas, **189 ROMs, 126 sin portada** (FC 109, GBA 17), 63 completas.
- **Chequeo de rutas DAT (HEAD a libretro-database):** `dat/HBMAME.dat` 200 · `metadat/fbneo-split/FBNeo - Arcade Games.dat` 200 · `metadat/mame/MAME.dat` 200 · `metadat/no-intro/Nintendo - Game Boy Advance.dat` 200 · `metadat/no-intro/Nintendo - Nintendo Entertainment System.dat` 200 (los dos 404 de `<libretro>.dat` son la plantilla del comentario de `systems.toml`, no rutas). Repetido para las rutas derivadas de **todos** los sistemas: 404 en `Atari - 8-bit` y `Handheld Electronic Game` → corregido (`e2e16f8`: `Atari - 8-bit Family.dat`, `dat/Handheld Electronic Game.dat`); DOS, RPG Maker y ScummVM no tienen DAT de contenido útil → 404 recordado, se identifican por nombre.
- **Dry-run** (`--concurrency 4`, caché fría, **5,65 s**): salió `Tom &amp; Jerry 3` como `review` fuzzy → bug de entidades HTML en los href de Apache (278 nombres con `&amp;` solo en NES Boxarts) → test + fix `0bd4eab` (namespaces de caché versionados). Tras el fix (3,26 s): **FC 96 auto / 5 review / 8 none; GBA 13 / 3 / 1; total 109 / 8 / 9**. Identificación: FC **74 por hash** + 35 por nombre; GBA 1 por hash + 16 por nombre (las ROMs GBA de la SD están recortadas/trimmed y no casan con No-Intro). `diff` del árbol tras el dry-run: vacío. Revisión manual: hash → caja canónica (`Chip 'n Dale` → `Chip to Dale no Daisakusen (Japan)`, `Amagon` → `Totsuzen! Macchoman (Japan)`, `Lode Runner` → `Championship Lode Runner`); por nombre, título exacto (`Mortal Kombat Advance (USA)`, `Megaman Zero 2 (USA)`); `review` razonables (`Megaman Zero 1` → `Megaman Zero (USA) (Virtual Console)`, `MegaMan & Bass` → `Megaman _ Bass (USA)`); ninguna secuela ni juego equivocado. Variantes mejorables → M2-11.
- **Apply `--auto-only`:** job `20261016T050401Z-1bfb-0000`: **filled 109, replaced 0, skipped 17, no_art 0, failed 0, paused none**; **17,48 s** con descargas; repetido desde el art store **0,97 s**. **109 PNG escritos, 0 por encima de 250×360**, todos `PNG image data`; **0 `._*`**, 0 temporales `.coverly-*`. Auditoría después: **17 sin portada** (126 − 109), 172 completas. Peticiones HTTP: no se registran (sin contador en M2).
- **Imagen FAT32** (`hdiutil create -fs "MS-DOS FAT32"` 512 MB — 128 MB no cabían 15 ROMs GBA, 150 MB —, montada con `-nobrowse -mountpoint $SC/e2e/fat`): 1ª pasada **12 `._*` nuevos por 12 PNG** (`com.apple.provenance`, imborrable con `xattr -d`/`-c`) → fix `90874a2`; 2ª pasada en imagen nueva: **filled 12, `._*` nuevos 0** (los 18 existentes los crearon `mkdir`/`cp` de la preparación), `Imgs` con 12 entradas, 0 temporales; `coverly undo` → removed 12, **left after undo: 0**; `hdiutil detach` OK.
- **Undo:** `Undid job …: removed 109, restored 0` → `shasum` de todo el árbol vs snapshot: **diff vacío**.
- **Respaldo ida y vuelta** (`--system GBA --scope all --overwrite backup --apply --auto-only`): **filled 13, replaced 19, skipped 7**, 19 respaldos en `$SC/e2e/data/backups/<job>/ROMS/GBA/Imgs`; `undo` → removed 13, restored 19 → **diff vacío** (bytes originales restaurados).
- **Origen intacto:** `find ~/Documents/ROMS/GBA ~/Documents/ROMS/FC -newer snapshot | wc -l` → **0**. `/Volumes` sin la imagen de pruebas (`mount | grep -c COVERLYT` → 0).
- **Checks globales** tras cada fix: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` → todo verde.

## M3 — Explorador (app)

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M3-01 | Pantalla inicio + librerías recientes + drag&drop | ⬜ | |
| M3-02 | Vistas: Carpetas / Sistemas / Fecha / Todo | ⬜ | |
| M3-03 | Grid virtualizado + tabla | ⬜ | |
| M3-04 | Filtros y búsqueda | ⬜ | |
| M3-05 | Panel de detalle con acciones | ⬜ | |

## M4 — Relleno masivo en la app

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M4-01 | Asistente de relleno (alcance, media, estrategia, sobrescritura, dry-run) | ⬜ | |
| M4-02 | Cola de revisión lado a lado | ⬜ | |
| M4-03 | Jobs: progreso, cancelar, reanudar, deshacer | ⬜ | |
| M4-04 | Reportes CSV/JSON | ⬜ | |

## M5 — Configuración e i18n

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M5-01 | Settings tipados + persistencia + migración | ⬜ | |
| M5-02 | Pantalla de configuración (todos los grupos de la spec §9) | ⬜ | |
| M5-03 | Editor de perfiles personalizados | ⬜ | |
| M5-04 | Credenciales en keychain | ⬜ | |

## M6 — Fuentes secundarias

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M6-01 | SteamGridDB (API key) + estrategia Popularidad | ⬜ | |
| M6-02 | ScreenScraper (credenciales dev) — v1.1 | ⬜ | |

## M7 — Release y actualizaciones

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M7-01 | Par de claves minisign (secreto en GitHub, nunca en git) | ⬜ | |
| M7-02 | `release.yml` + `release-channels.yml` (de Pluryprint) | ⬜ | |
| M7-03 | Updater en app y CLI | ⬜ | |
| M7-04 | Marca e iconos | ⬜ | |
| M7-05 | Repo público + v0.1.0 | ⬜ | |

## Bitácora

- **2026-10-15** — Investigación de herramientas Mac; repo creado; decisiones D1–D8; spec maestra y tracker escritos; investigación Top1 en curso.
- **2026-10-15** — Investigación Top1 terminada (14 agentes, ~270 claims verificados) → `docs/02-research/`; spec §16 refinamientos + pregunta abierta 5 (Popularidad/SteamGridDB solo PC).
- **2026-10-15** — M0 + M1 implementados (plan tareas 1–11, rama `feat/m1-audit`). `coverly audit` validado en solo lectura contra la SD real: 7473 ROMs, 3080 sin portada, 1,69 s en la 1ª ejecución; recuento manual de SFC/GB/WS/PS coincide. ADRs 0001–0008 + arquitectura + índice de docs (`1455f8a`). M1-05 (caché SQLite) movida a M2.
- **2026-10-16** — M2 implementado en `feat/m2-fill` (plan tareas 1–14): identificación (hash → DATs, arcade, nombre), libretro-thumbnails, estrategias, plan, imagen, escritura segura, `coverly fill`/`undo`. E2E sobre copia de la SD: 109/126 portadas rellenadas, undo y respaldos byte a byte; imagen FAT32 sin `._*`. El E2E destapó y arregló 3 bugs (rutas DAT, `&amp;` en listados, `._*` en FAT). Docs: identification, art-sources, fill-pipeline, ADR-0009/0010; D12–D15.
