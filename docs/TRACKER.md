# TRACKER — Coverly ROMs

> Documento vivo. **Si retomas tras un compact, empieza aquí.**
> Fuente de verdad del diseño: [`docs/superpowers/specs/2026-10-15-coverly-roms-design.md`](superpowers/specs/2026-10-15-coverly-roms-design.md).
> Regla: una tarea solo pasa a ✅ con **evidencia** (commit, comando + salida, archivo, URL).
> Estados: ⬜ pendiente · 🟡 en curso · ✅ hecho · ⛔ bloqueado · ⏸️ pospuesto

## Estado actual (actualizar en cada sesión)

- **Fecha:** 2026-10-16
- **Fase:** M4 implementado en `feat/m4-fill-app` (pendiente de PR). M3 mergeado ([PR #4](https://github.com/MAECLY/coverly-roms/pull/4), `910618e`).
- **Siguiente paso:** PR M4 + verificación independiente. Pendiente de Miguel (no bloqueante): en la app real (Tauri), abrir una **copia** de la SD, rellenar portadas desde el asistente y deshacer («Evidencia M4»); y lo pendiente de M3 (diálogo nativo, scroll, drawer, Reveal).
- **En curso:** cierre de M4 (tarea 13 hecha: E2E, docs y tracker).
- **Fuente de ROMs para pruebas:** copia local completa `/Users/miguelangelesparzacalero/Documents/ROMS` (55 sistemas, 91 GB; la SD puede estar expulsada). Solo lectura; escrituras siempre sobre copias en el scratchpad.

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
| D16 | El árbol de carpetas se deriva en la UI desde `entry.dir`; `LibraryView` no lleva `folders` | 2026-10-16 | Claude (plan M3) |
| D17 | Escaneo progresivo = eventos de progreso por carpeta de sistema; las entradas llegan al final (abrir la biblioteca real: 367 ms en frío / 140 ms en caliente, release) | 2026-10-16 | Claude (plan M3) |
| D18 | El arte llega por IPC como bytes, pedido por `(generación, id)`; sin asset protocol; CSP `img-src 'self' data: blob:` | 2026-10-16 | Claude (plan M3) |
| D19 | Elegir directamente la carpeta `ROMS` funciona (`fit_to_root`); idioma en `localStorage` hasta M5 | 2026-10-16 | Claude (plan M3) |
| D20 | En la app se escriben solo los ítems auto y los de revisión aceptados/elegidos; los de revisión sin decidir se omiten (la CLI mantiene D14) | 2026-10-16 | Claude (plan M4) |
| D21 | Reanudar un job pausado continúa el mismo journal (Writer::resume): un único Deshacer revierte ambas tandas. En la app, Deshacer actúa solo sobre journals de la biblioteca abierta y se rechaza mientras un job está pausado | 2026-10-16 | Claude (plan M4) |
| D22 | M2-11: una variante beta/proto/demo/hack/sin licencia nunca es auto salvo coincidencia canónica o de nombre exacto, o si la ROM es esa variante; se antepone la primera release ≥ umbral de revisión | 2026-10-16 | Claude (plan M4) |
| D23 | Los diálogos de «Desde archivo…» y de exportar se abren desde Rust: la webview nunca envía rutas y no hay capacidades nuevas | 2026-10-16 | Claude (plan M4) |

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
| M2-10 | Batocera/Knulli: fusionar `gamelist.xml` al rellenar (diferido de M2; hoy `fill` avisa) | ⬜ | M5. En M4: aviso en el asistente para batocera (como la CLI) |
| M2-11 | Selección de variantes: hoy un hack/beta puede ganar como `auto` cuando es el único título exacto o el mejor fuzzy (GBA `Final Fantasy IV` → caja «Sound Restoration hack», `Contra Advance` → `(Beta)`): penalizar o mandar a `review` | ✅ | `526e3a9` (D22). Ejecución real M4: `054 Contra Advance` → `Review Some(UnreleasedVariant)` → `Contra Advance (USA) (Beta)`; `057 Final Fantasy IV` → `Review Some(UnreleasedVariant)` → `Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)` («Evidencia M4») |
| M2-12 | Perfil Anbernic stock: las portadas existentes de la SD miden 285×184, fuera de la caja 250×360 del perfil → revisar el tamaño máximo con la consola real | ⬜ | Visto en el E2E (22 portadas GBA originales). Sigue abierto tras M4 (necesita la consola real) |
| M2-13 | Emparejado por título: quitar prefijos numéricos de las ROMs (`001 Golden Sun`, habituales en SD Anbernic). Hoy `001 Golden Sun` cae a fuzzy 71 (`review` en vez de `auto`) aun con DAT, y `002 Gldn Rsh` queda sin candidatos (sin prefijo: fuzzy 72) | ⬜ | Visto en la revisión de la tarea 3 de M4 (fixture de `fill_env.rs`) |

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
| M3-01 | Pantalla inicio + librerías recientes + drag&drop | ✅ | Core: `be519c4` (progreso), `f17b801` (`fit_to_root`), `0345817` (ignorados + recientes). App: `206886f`, `8f4e1f2`, `90dbcef`, `892db67`, `23f94e1`. UI: `ab80981`, `016b05f`, `7d53fe0`, `65c4413`, `75172c1`, `4ee52cb`, `9c4473a`, `060c651`, `4a1746c` (recuentos con formato del idioma, hallado en el E2E). Doc: [desktop-app.md](04-architecture/desktop-app.md) |
| M3-02 | Vistas: Carpetas / Sistemas / Fecha / Todo | ✅ | `673e5c1` (árbol, ámbitos, orden), `3a100b9`, `e8d23c9` |
| M3-03 | Grid virtualizado + tabla | ✅ | `2d0309f`, `c0dbe8d`, `a20aef2`, `ee00ef8`, `9041e1d`; 48 celdas montadas como máximo con 12k (presupuesto 120) |
| M3-04 | Filtros y búsqueda | ✅ | `673e5c1` (búsqueda sin acentos, filtros, facetas), `3a100b9`, `07f665c`, `de3a458`, `b381df2` (campos con `name`, hallado en el E2E) |
| M3-05 | Panel de detalle con acciones | ✅ | `2808f63` (identificar bajo demanda, sin conexión → por nombre), `5fc856e` (panel: arte, ficha, identidad, «Mostrar en Finder») |
| M3-06 | Inicio: volúmenes extraíbles detectados (spec §8) | ⏸️ | Diferido a M5 (plan M3) |
| M3-07 | Filtro por método/confianza de identificación (spec §8) | ⏸️ | Diferido a M4: en M3 la identidad es bajo demanda. Tras M4 sigue diferido (identidad bajo demanda) → M5 |
| M3-08 | Elegir/confirmar el perfil al abrir (spec §7.1); hoy `no-profile` si no hay marcadores | ⏸️ | Diferido a M5 (editor de perfiles); el comando ya acepta `profileId` |

### Evidencia M3 — explorador (2026-10-16)

Evidencia en `$S` = `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/m3-e2e/` (no versionada). **Nada se ejecutó contra `/Volumes`.**

- **Biblioteca real en solo lectura** (`COVERLY_REAL_ROMS=~/Documents/ROMS cargo test -p coverly-roms --release --test real_library -- --ignored`, camino real de comandos `open_session`): `profile=anbernic-stock fitted=true systems_with_roms=34 folders=55 entries=7473 missing_cover=3080 warnings=0 progress_events=56 json_bytes=1777055`; **`open=367 ms` en frío** (scan 176 ms), **140 ms en caliente** (scan 90 ms). Mismos totales que la auditoría M1-07 de la SD (7473 / 3080). Muy por debajo de 3 s → no hace falta enviar entradas por sistema.
- **Origen intacto:** `find ~/Documents/ROMS -cnewer snapshot | wc -l` → **0** (antes y después de todo el E2E). `-newer` (mtime) lista 3 ficheros de `SFC` (`Star Trek-The Next Generation.smc`, `Super Buster Bros..smc`, `Battle Tycoon.smc`) porque ya traían fecha de modificación de **2099** en la copia; su ctime es anterior al snapshot, no se tocaron.
- **Árbol generado de 12.000 ROMs** (`library_scale`, release): **151 ms**, 2.913.204 bytes de JSON.
- **E2E en Chrome** (Chrome DevTools MCP, `pnpm dev`, viewport 1440×900, mock 12k): inicio en/es con recientes y `EJECTED` desactivado; progreso capturado por sondeo (18 estados, «Scanning DREAMCAST… 1 of 18 folders» … «Scanning SMS… 17 of 18 folders · 11,867 ROMs found»; demasiado rápido para captura); Sistemas → Super Nintendo 1,333; Carpetas → SFC › Hacks 95; Fecha → This week 435 (Modified descendente); búsqueda `pokemon edicion` → `Pokémon - Edición Oro (Spain)`; `<img` → el nombre hostil como texto, sin `img[src=x]` ni `[onerror]`; filtros Missing cover + Super Nintendo + .sfc → «Filters (3)», estado y facetas 267; tabla Size ascendente/descendente (`aria-sort`); teclado (flechas, End, Home, Enter abre, flecha sigue, Escape cierra, `/` busca); panel con hash 100 % y con la nota sin conexión, acciones de M4 desactivadas, «Reveal» sin error; español («12.000 de 12.000 ROMs»); soltar `Broken` → «Esta carpeta no existe»; `Empty library` → «No se han encontrado ROMs en esta carpeta.».
- **Scroll 12k** (120 saltos de 600 px): **48 celdas** como máximo (presupuesto 120), tabla 42 filas (presupuesto 60). Tiempo de frame (media / p95): build de producción (`vite preview`) **16,6–19,4 / 20–30 ms** (dentro de ≤ 20 / ≤ 50); `pnpm dev` 22–32 / 42–58 ms por el modo desarrollo de React.
- **Consola y red:** tras los fixes, 0 errores, 0 avisos, 0 issues (`console.txt`); 1068 peticiones, todas 200, `favicon.png` 200 (`network.txt`).
- **Bugs del E2E:** recuentos de recientes y del progreso sin formato del idioma («12000 ROMs») → test + fix `4a1746c`; issue de DevTools «A form field element should have an id or name attribute» (13 campos) → test + fix `b381df2`.
- **App Tauri real:** `pnpm tauri build --debug --no-bundle` OK (chequeo de versiones de plugins, capabilities, CSP); `target/debug/coverly-roms` arrancó y siguió vivo (`pgrep`), inicio renderizado en `17-tauri-start.png`; parado.
- **Comprobación manual pendiente (Miguel, solo lectura, nunca bajo `/Volumes`):** en la app real, abrir `~/Documents/ROMS` con el diálogo nativo, desplazarse por las portadas reales, abrir el panel y «Mostrar en Finder».
- **Capturas:** `01-start-en.png`, `02-scanning.png`, `03-grid-12k.png`, `04-grid-scrolled.png`, `05-systems.png`, `06-folders.png`, `07-date.png`, `08-search.png`, `09-filters.png`, `10-table.png`, `11-keyboard.png`, `12-drawer.png`, `12b-drawer-offline.png`, `13-library-es.png`, `14-start-es.png`, `15-error.png`, `16-empty.png`, `17-tauri-start.png`; logs `real-library-{1,2}.log`, `scale-12k.log`, `tauri-build.log`; notas `e2e-notes.md`.
- **Tests:** `cargo test --workspace` → 312 pasan; `pnpm test` → 156 pasan (28 ficheros). fmt, clippy `-D warnings`, lint, typecheck, format y build en verde.

## M4 — Relleno masivo en la app

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M4-01 | Asistente de relleno (alcance, media, estrategia, sobrescritura, dry-run) | ✅ | Core: `ed93ede` (fachada `FillEnv` + decisiones), `178dcb3`, `669fca4`, `f70749f`. App: `b6be996`, `81af28d`, `b976768`, `936af46`, `5be5ab2`. UI: `c8afb15`, `9541e09`, `aeb995b` (el recuento de «Ejecutar» no cuenta las omitidas, hallado en el E2E). «Evidencia M4» |
| M4-02 | Cola de revisión lado a lado | ✅ | `73a93d7`, `941b8ed`, `8893f4a`; mock: `43f54a7`. «Evidencia M4» |
| M4-03 | Jobs: progreso, cancelar, reanudar, deshacer | ✅ | Core: `8b7a9d7` (pausas, journal reanudable, borrado journaled), `fcd594b`, `343b6e7`, `5b1968e`. App: `81af28d`, `17a4e35`, `5be5ab2`, `9571040`, `bc3d659`, `88f6458`. UI: `6d0715e`, `88107be`, `b6e7668`, `76dfebf` (eventos del job perdidos con StrictMode en el mock, hallado en el E2E), `80684b5` («1 archivo»). Ejecución real por el camino de la app: `c778dc0`. «Evidencia M4» |
| M4-04 | Reportes CSV/JSON | ✅ | `409911f` (CSV del job a prueba de fórmulas), `ff179a2` (acciones del panel + exportaciones con diálogo en Rust), `3c0387a`, `2cb9171`, `d1aa5ea`, `8efad85` (panel de informes). «Evidencia M4» |
| M4-05 | Buscar por título en el panel (spec §8) | ⏸️ | Diferido a M5: necesita una identidad manual que no cambie el nombre del destino |

### Evidencia M4 — relleno en la app (2026-10-16)

Evidencia en `$SC/m4-e2e/` con `$SC` = `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad` (no versionada). **Nada se ejecutó contra `/Volumes`.** `~/Documents/ROMS` solo se leyó (`cp -X`).

- **Ejecución real por el camino de la app** (test ignorado `src-tauri/tests/real_fill.rs`, `COVERLY_FILL_COPY=$SC/m4-copy COVERLY_FILL_WORK=$SC/m4-work cargo test -p coverly-roms --release --test real_fill -- --ignored --nocapture`, servidores reales; es el mismo código que usa la UI: `AppState::open` → `start_fill` → `jobs().run` → `undo_last_job`, con `refresh_entries` en cada escritura). Log: `real-fill.log`.
  - **Copia:** `head -60` de `ROMS/GBA` (incluye `054 Contra Advance` y `057 Final Fantasy IV`) + `head -150` de `ROMS/FC` + sus `Imgs` + `anbernic/` → **684 MB, 273 ficheros**; abierta como `anbernic-stock`: **209 ROMs, 146 sin portada**.
  - **Dry-run** (caché fría): **5,9 s**, auto **122** / review **14** / none **10** (FC 95 / 6 / 8, GBA 27 / 8 / 2). El árbol de la copia no cambia.
  - **M2-11:** `054 Contra Advance: Review Some(UnreleasedVariant) -> Some("Contra Advance (USA) (Beta)")`; `057 Final Fantasy IV: Review Some(UnreleasedVariant) -> Some("Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)")`.
  - **Apply (solo auto, sin decisiones; D20):** **19,3 s**, filled **122**, replaced 0, skipped 24, no-art 0, failed 0, journal `20261016T105644Z-757c-0000`. Un fichero nuevo por ítem rellenado; todos PNG ≤ 250×360; ningún `._*` ni `.coverly-*`.
  - **Sin portada en la sesión** (refresco de las entradas tocadas, sin reabrir): **146 → 24** (146 − 122).
  - **Undo** (`undo_last_job`): removed 122, restored 0, kept 0 → árbol SHA-1 de la copia **idéntico** al de antes.
  - **Origen intacto:** `find ~/Documents/ROMS/GBA ~/Documents/ROMS/FC -cnewer snapshot | wc -l` → **0** (`source-untouched.txt`).
- **E2E en Chrome** (Chrome DevTools MCP, `pnpm dev`, viewport 1440×900, mock 12k; notas paso a paso en `e2e-notes.md`):
  - Asistente (paso 1 de 5 → toda la biblioteca, Portada + Título, Región de la ROM, «Move USA up», solo lo que falta) → progreso → resumen por sistema con Total **8.717 auto / 2.437 review / 1.247 none** y 4 muestras.
  - Cola de revisión: arte actual + 3 candidatos con fuente, región y puntuación; `2`, `Enter` («2 of 2,437 · 1 decided»), `s`, `a` («2,437 decided»), `Esc` → «Run (11,153)» (8.717 + 2.436 aceptadas).
  - Ejecutar → pausa del mock tras 25 ítems: «Paused: the art server is limiting requests…» + detalle HTTP 429 + Resume → «Finished»: filled **10.481**, skipped 1.248, no art 186, failed 486, «Problems (672)» con red / nombre no válido / sin arte / imagen no válida. Barra de estado **4.553 → 697** sin portada; las celdas muestran el arte nuevo.
  - Exportar informe CSV y JSON → «Saved as coverly-job-1.csv.» / «.json.» (descargados, copiados a la evidencia). Deshacer → confirmación → «Undone: 10,481 removed, 0 restored, 0 kept…», historial «Undone», barra **4.553**.
  - Panel de detalle: «Change…» → 3 candidatos → el 2.º → «Art updated…» (4.552); «From file…» → actualizado; «Remove» → confirmación → «Remove image» → sin portada (4.553); Jobs → Deshacer → «0 removed, 1 restored» y la portada vuelve (4.552).
  - Informes: auditoría por sistema con Total 12.000; «Export audit (CSV)» → «Saved as coverly-audit.csv.» (12.001 líneas).
  - Nombres hostiles: `<img` → panel de `<img src=x onerror=alert(1)>` → «Change…»: candidatos como texto; `document.querySelectorAll('img[src="x"], [onerror]').length` → **0**.
  - Español: «Rellenar arte que falta», «Paso 1 de 5», «Revisar coincidencias dudosas», «Trabajos», «Deshacer el último trabajo», «1 archivo».
  - Pasada limpia tras recargar (en español: dry-run, `s`/`a`, «Ejecutar (4098)» = 3193 + 905, pausa, reanudar, 3855 rellenadas, 4553 → 698 → deshacer → 4553): consola **0 errores, 0 avisos** (`console.txt`); red: 117 peticiones, todas 200/304 (`network.txt`).
  - **Bugs del E2E (test primero + fix):** (1) el resumen del dry-run no aparecía nunca: el `listen` del mock identificaba la suscripción por callback y la doble conexión de React StrictMode borraba la buena → `76dfebf`; (2) «Run (n)» contaba también las de revisión omitidas → `aeb995b`; (3) historial «1 files» → «1 file» / «1 archivo» → `80684b5`.
  - Observaciones (sin cambio): mientras el job está en pausa, «Skipped» incluye los ítems aún no intentados (igual que `JobReport` en Rust); los textos «{n} ROMs» dicen «1 ROMs» para una sola ROM (redacción de M3); las celdas del grid no aparecen como clicables en el árbol de accesibilidad (se abrió el panel con `element.click()`); el estado «aplicando» del mock dura menos que una captura (`05-applying.png` ya muestra la pausa; el texto «Escribiendo arte: 81 de 3168» se leyó del DOM).
  - **Capturas:** `01-library.png`, `02-gathering.png`, `03-summary.png`, `04-review.png`, `05-applying.png`, `06-paused.png`, `07-done.png`, `07b-library-filled.png`, `08-undone.png`, `09-drawer-candidates.png`, `10-reports.png`, `11-es-wizard.png`, `11b-es-review.png`, `12-es-jobs.png`; exportaciones `coverly-job-1.csv`, `coverly-job-1.json`, `coverly-audit.csv`.
- **Comprobación manual pendiente (Miguel):** en la ventana Tauri, abrir una copia de la SD (nunca `/Volumes` sin respaldo), rellenar portadas desde el asistente y deshacer.
- **Tests:** `cargo test --workspace` → **389 pasan**, 0 fallan, 2 ignorados (`real_library`, `real_fill`); `pnpm test` → **241 pasan** (41 ficheros). `cargo fmt --check`, `clippy -D warnings`, `pnpm lint`, `typecheck`, `format:check` y `build` en verde.

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
- **2026-10-16** — M2 mergeado (PR #3, `2a1a8e1`; Actions bloqueado por facturación → gates locales: 265 tests Rust + 11 front, clippy/fmt/lint limpios). Verificación independiente sobre copia de 61 ROMs SFC: 61/61 rellenadas (59 por hash, 2 por nombre), todas el juego correcto revisadas a mano (`Digital Devil Story X` → *Shin Megami Tensei if...*), 12 s en frío; undo quitó las 61. Fix menor `create_dirs` solo NotFound.
- **2026-10-16** — M2 implementado en `feat/m2-fill` (plan tareas 1–14): identificación (hash → DATs, arcade, nombre), libretro-thumbnails, estrategias, plan, imagen, escritura segura, `coverly fill`/`undo`. E2E sobre copia de la SD: 109/126 portadas rellenadas, undo y respaldos byte a byte; imagen FAT32 sin `._*`. El E2E destapó y arregló 3 bugs (rutas DAT, `&amp;` en listados, `._*` en FAT). Docs: identification, art-sources, fill-pipeline, ADR-0009/0010; D12–D15.
- **2026-10-16** — M3 implementado en `feat/m3-explorer` (plan tareas 1–13): explorador con vistas, grid/tabla virtualizados, filtros y búsqueda, panel de detalle, mock de navegador; E2E en Chrome (12k entradas) y biblioteca real en solo lectura (7473 ROMs, 367 ms en frío). El E2E destapó 2 bugs menores (recuentos sin formato, campos sin `name`). D16–D19.
- **2026-10-16** — M3 mergeado (PR #4, `910618e`; Actions bloqueado por facturación → gates locales: 312 tests Rust + 156 Vitest, clippy/fmt/lint limpios). Comprobación independiente con Chrome DevTools MCP sobre `pnpm dev`: inicio con recientes, fixture 12k abre el grid virtualizado («12.000 de 12.000 ROMs · 4553 sin portada»), 0 errores/avisos de consola.
- **2026-10-16** — M4 implementado en `feat/m4-fill-app` (plan tareas 1–13): asistente de relleno, cola de revisión por teclado, jobs con pausa/reanudación/cancelación/deshacer, acciones del panel (cambiar, desde archivo, quitar), informes CSV/JSON; M2-11 resuelto (D22). Ejecución real por el camino de la app sobre copia (122 portadas, undo idéntico) y E2E en Chrome (3 bugs arreglados). D20–D23.
