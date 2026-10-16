# TRACKER — Coverly ROMs

> Documento vivo. **Si retomas tras un compact, empieza aquí.**
> Fuente de verdad del diseño: [`docs/superpowers/specs/2026-10-15-coverly-roms-design.md`](superpowers/specs/2026-10-15-coverly-roms-design.md).
> Regla: una tarea solo pasa a ✅ con **evidencia** (commit, comando + salida, archivo, URL).
> Estados: ⬜ pendiente · 🟡 en curso · ✅ hecho · ⛔ bloqueado · ⏸️ pospuesto

## Estado actual (actualizar en cada sesión)

- **Fecha:** 2026-10-15
- **Fase:** Brainstorming → spec escrita + refinada con investigación (§16), **pendiente de revisión de Miguel** y de decidir pregunta abierta 5 (estrategia Popularidad).
- **Siguiente paso:** Miguel revisa la spec → plan de implementación (skill writing-plans) → M0.
- **En curso:** nada en segundo plano. Investigación terminada y en `docs/02-research/`.

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
| P0-10 | Completar docs 01–09 + ADRs desde la spec | ⬜ | |
| P0-11 | Revisión de spec por Miguel | ⬜ | |
| P0-12 | Plan de implementación (writing-plans) | ⬜ | |
| P0-13 | Diseño de marca (wordmark + mark/icono) | ⬜ | |

## M0 — Fundación del repo

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M0-01 | LICENSE (AGPL-3.0), LICENSES/, TRADEMARKS.md, CLA.md, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, THIRD-PARTY-NOTICES | ⬜ | |
| M0-02 | Workspace Cargo (`coverly-core`, `coverly-cli`) + `src-tauri` + React/Vite/Tailwind (plantilla Pluryprint) | ⬜ | |
| M0-03 | Tooling: pnpm, eslint, prettier, commitlint, husky, Makefile | ⬜ | |
| M0-04 | CI (`ci.yml`) matriz 3 SO: fmt, clippy, tests, lint | ⬜ | |
| M0-05 | i18n base en/es | ⬜ | |

## M1 — Escaneo y auditoría

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M1-01 | `systems.toml` (alias, extensiones, playlist libretro, DATs, header-skip) | ⬜ | |
| M1-02 | Perfiles built-in + autodetección + TOML de usuario | ⬜ | |
| M1-03 | Walker + clasificación ROM (multi-archivo, m3u, cue/bin, carpetas) + ignorar `._*` | ⬜ | |
| M1-04 | ArtStatus por media (present/missing/orphan) | ⬜ | |
| M1-05 | Caché SQLite de escaneo | ⬜ | |
| M1-06 | `coverly audit` (tabla, CSV, JSON) | ⬜ | |
| M1-07 | Validar contra SD real (solo lectura) | ⬜ | |

## M2 — Identificación, fuentes y relleno

| ID | Tarea | Estado | Evidencia |
|---|---|---|---|
| M2-01 | Hash CRC32 + header skip + límites de tamaño | ⬜ | |
| M2-02 | Índice DAT (libretro-database) + mapa arcade MAME/FBNeo | ⬜ | |
| M2-03 | Normalización de nombres + parseo de región + fuzzy | ⬜ | |
| M2-04 | Fuente libretro (índice por sistema, caché, descarga) | ⬜ | |
| M2-05 | Estrategias: Prioridad, Región ROM, Popularidad | ⬜ | |
| M2-06 | Plan (auto/revisar/sin resultado) + dry-run | ⬜ | |
| M2-07 | Imagen: decode/resize/encode por perfil | ⬜ | |
| M2-08 | Escritura atómica, respaldo, journal, undo, nombres FAT-safe | ⬜ | |
| M2-09 | `coverly fill` (dry-run/apply/--auto-only) | ⬜ | |

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
