# Agradecimientos y créditos de Coverly: inventario para el TODO y la pestaña de Ajustes

Base: `coverly-roms` en `main` (`77289ed`) y la rama `coverly-roms-systems-3` (`895a2b6`). La rama no añade ningún host, ni dependencias Rust ni JS. Todas las rutas `crates/...` cuelgan de `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/`.

Cómo se marca cada dato:
- **[SIN VERIFICAR]**: no lo pude comprobar en la fuente original.
- **Obligatorio**: lo exige la licencia o los términos de uso.
- **Cortesía**: lo ponemos porque es lo correcto, no porque lo exijan.

Dos dudas de la petición original:
- «screensaver» se interpreta como **ScreenScraper**.
- «linbreto tumbnails» se interpreta como **libretro-thumbnails**.
- «ahora mismoa» era «ahora mismo», no una fuente.

---

## Ítem propuesto para TODO.md (antes del 7.1)

> **7.0 Ajustes → pestaña «Agradecimientos»**
> - Añadir una pestaña en Ajustes que acredite, en tarjetas por categoría, las fuentes de datos y APIs, los proyectos de origen de los DAT y las convenciones, los recursos incluidos (iconos y fuentes) y las bibliotecas. Cada tarjeta lleva un enlace oficial, la licencia y el texto de crédito.
> - Al final, un enlace «Avisos legales completos» que abra un `THIRD-PARTY-NOTICES` generado al compilar e incluido en la app. Sustituye o reutiliza `LinkId::ThirdPartyNotices` (`src-tauri/src/prefs.rs:86-93`).
> - Corregir a la vez los fallos de cumplimiento de §6.
> - Al integrar titledb y las fuentes del 7.1, añadir sus tarjetas (§5).

---

## 1. Fuentes en línea, APIs y datos

### 1.1 Lo que se usa hoy

| Fuente | Para qué la usa Coverly | Enlace | Licencia | Crédito | Texto propuesto | Código |
|---|---|---|---|---|---|---|
| **libretro-thumbnails** | Portadas, capturas, pantallas de título y logos, buscados por nombre | https://github.com/libretro-thumbnails/libretro-thumbnails | Ninguna declarada (`license: null`) | Cortesía | «Portadas y capturas: libretro-thumbnails. Capturas vía MobyGames, Fandom y aportaciones de voluntarios. El arte pertenece a los desarrolladores y editores de cada juego.» (adaptado del README, líneas 157-165) | `crates/coverly-core/src/sources/libretro.rs:23-24`, `fill/mod.rs:343-345`, `systems/systems.toml:8` |
| **libretro-database** | Identificar ROMs por hash, serie o nombre corto de arcade, y sacar el nombre del catálogo | https://github.com/libretro/libretro-database | CC-BY-SA-4.0 (comprobado en LICENSE) | Cortesía, porque solo se descarga a la caché del usuario (`dat.rs:1-8`). Pasa a obligatorio (BY + SA) si algún día se empaquetan los DAT | «Datos de identificación: libretro-database, © libretro contributors, CC BY-SA 4.0.» | `identify/dat.rs:20-21`, `:93-106`; `coverly-cli/src/organize.rs:431-434`; `coverly-cli/src/dedupe.rs:33` |
| **SteamGridDB** | Portadas verticales hechas por la comunidad, con votos. Usa la clave de API del propio usuario | https://www.steamgriddb.com | **[SIN VERIFICAR]**: la página `/terms` se monta con JavaScript y no se pudo leer | Sin verificar; recomendado | «Arte de la comunidad de SteamGridDB. Cada imagen es de su autor.» | `sources/steamgriddb.rs:1-6`, `:24`, filtro de hosts en `:324-333`; `fill/mod.rs:315` |
| **ScreenScraper** | Medios (`box-2D`, `flyer`, `ss`, `sstitle`) e identificación por hash en Organizar. Solo se activa con credenciales de desarrollador y la cuenta del usuario | https://www.screenscraper.fr | CC-BY-NC-SA-4.0 (comprobado en `webapi2.php`) | **Obligatorio** por la cláusula BY. No dan una frase exacta. Su condición es que la app sea gratuita: «ne peut être intégré que dans les applications entièrement gratuites et distribuées, ou… avec l'autorisation préalable…». Coverly es AGPL y gratis. La gestión de cuotas también es obligatoria | «Datos y medios de ScreenScraper (https://www.screenscraper.fr), CC BY-NC-SA 4.0.» | `sources/screenscraper.rs:28`, `:30`, `:40`, `:71-74`, endpoints en `:496`, `:556`, `:641`; `src-tauri/src/state.rs:1460-1464`; `coverly-cli/src/organize.rs:444-458` |
| **GitHub Releases** | El actualizador lee `latest.json` y baja las versiones firmadas | https://github.com | Términos de GitHub | Cortesía | «Actualizaciones distribuidas a través de GitHub Releases.» | `crates/coverly-core/src/update/mod.rs:19-24`; `src-tauri/tauri.conf.json:60-61` |

Datos incluidos en el binario. `systems.toml` se compila dentro de la app con `include_str!` (`systems/mod.rs:13`). Contiene:
- 110 nombres de carpeta de libretro-thumbnails (111 en la rama);
- 102 IDs de sistema de ScreenScraper;
- 49 rutas `dats =`.

Son hechos, así que basta con la atribución de cortesía de las filas de arriba.

### 1.2 Proyectos de origen de los DAT (llegan a través de libretro-database)

| Proyecto | Qué aporta | Enlace | Licencia o términos | Crédito |
|---|---|---|---|---|
| **No-Intro** | DAT de cartuchos. 20 entradas explícitas, más la ruta por defecto en `dat.rs:93-106` | https://no-intro.org | **[SIN VERIFICAR]**: DAT-o-MATIC tiene condiciones que no se pudieron leer | Cortesía (sin verificar) |
| **Redump** | DAT de discos. 8 entradas, más la ruta por defecto | http://redump.org | **[SIN VERIFICAR]** en la fuente original | Cortesía |
| **TOSEC** | DAT de ordenadores clásicos (9 entradas) | https://www.tosecdev.org | **[SIN VERIFICAR]**: la web solo dice «All Rights Reserved» | Cortesía |
| **MAME** | Nombres cortos y DAT de arcade (8 entradas) | https://www.mamedev.org | El programa es GPL. Licencia del DAT **[SIN VERIFICAR]** | Cortesía |
| **FinalBurn Neo** | DAT `fbneo-split` (5 entradas) | https://neo-source.com | Licencia propia (`src/license.txt`), aplicable solo a su «original material». No usamos su código | Probablemente cortesía; conviene revisarlo con criterio legal |
| **GameTDB** | ID6/ID4 de GameCube, Wii y Wii U, y serial de PS3 | https://www.gametdb.com (vía https://github.com/RobLoach/libretro-database-gametdb) | No hay licencia formal. Su FAQ dice: «Yes, please add a link to GameTDB so that users know where to add new game entries and artwork» | **Piden un enlace**. Poner el enlace a gametdb.com |
| **RobLoach/libretro-dats** | Prepara la importación de la mayoría de DAT de No-Intro, Redump y TOSEC | https://github.com/robloach/libretro-dats | **[SIN VERIFICAR]** | Cortesía |
| **CaveStory.org** | `dat/Cave Story.dat` | http://www.cavestory.org | **[SIN VERIFICAR]** | Cortesía |
| World of Spectrum, Clean CPC DB, tic80.com, WASM-4 | DAT de `dat/` | (hay que confirmar la URL oficial) | **[SIN VERIFICAR]** | Cortesía |

Sin fuente conocida: Doom, Quake, Wolf3D, Dinothawr, Flashback, MrBoom, Rick Dangerous, Lutro, Handheld Electronic Game, Uzebox, Vircon32 y LowRes NX. Para ellos basta con el crédito a libretro-database.

---

## 2. Especificaciones, wikis y convenciones de las que aprendimos

| Convención | Uso en Coverly | Enlace | Crédito | Código |
|---|---|---|---|---|
| Nomenclatura de No-Intro (regiones, revisiones, etiquetas) | Entender las etiquetas de los nombres de archivo | https://no-intro.org | Cortesía | `identify/names.rs:185`, `:416`, `:436` |
| Convenciones de Redump (pistas y hojas CUE) | Patrón de pistas y lectura de `.cue` | http://redump.org | Cortesía | `disc/sheets.rs:3`, `:366` |
| Convenciones de TOSEC y GoodTools | Clasificar y emparejar | https://www.tosecdev.org | Cortesía | `scan/classify.rs:67`, `matching/mod.rs:104`, `:121` |
| Formato de los DAT clrmamepro / Logiqx | Analizar los DAT | **[SIN VERIFICAR]**: confirmar la URL oficial | Cortesía | `identify/dat.rs` |
| Fuentes secundarias de ScreenScraper (Wikipedia, GameFAQs, MobyGames, The Cover Project, GameTDB…) | Las nombra su apartado «General credits» | https://www.screenscraper.fr | Opcional: «ScreenScraper agradece a sus propias fuentes» | — |

**[SIN VERIFICAR]**: no se revisaron `docs/02-research/` ni las wikis de emuladores de forma sistemática. Antes de cerrar esta sección hay que hacer un barrido de URLs externas en `docs/`.

---

## 3. Bibliotecas, fuentes tipográficas e iconos destacados

### 3.1 Recursos incluidos en la app (crédito obligatorio o con aviso)

| Recurso | Para qué | Enlace | Licencia | Aviso exigido | Código |
|---|---|---|---|---|---|
| **RetroArch Assets**, XMB «Monochrome» (commit `d9f9690`), 86 SVG | Siluetas de consola | https://github.com/libretro/retroarch-assets | CC-BY-4.0 | **Obligatorio en la interfaz**: «Iconos de sistemas adaptados de RetroArch Assets (tema XMB Monochrome), © libretro contributors. CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/), sin garantías. Original: https://github.com/libretro/retroarch-assets. Cambios: recoloreados, optimizados con SVGO y recortados.» | `src/components/system/systemIcons.generated.ts:2-4`, `src/assets/system-icons/README.md:9-13` |
| **Archivo** | Tipografía de títulos | https://github.com/Omnibus-Type/Archivo | OFL-1.1 | «Copyright 2020 The Archivo Project Authors» + texto OFL junto a la fuente | `src/styles/globals.css:15-17` |
| **Atkinson Hyperlegible Next** | Tipografía de texto | https://github.com/googlefonts/atkinson-hyperlegible-next | OFL-1.1 | «Copyright 2020-2024 The Atkinson Hyperlegible Next Project Authors» + OFL | `globals.css:24-27` |
| **Atkinson Hyperlegible Mono** | Tipografía monoespaciada | https://github.com/googlefonts/atkinson-hyperlegible-next-mono | OFL-1.1 | «Copyright 2020-2024 The Atkinson Hyperlegible Mono Project Authors» + OFL | `globals.css:33-36` |
| **Lucide** (lucide-react 1.59.0) | Iconos de la interfaz (57 archivos) | https://lucide.dev | ISC, más MIT para los iconos que derivan de Feather | «Copyright (c) 2026 Lucide Icons and Contributors» y «Copyright (c) 2013-present Cole Bemis» + texto | `package.json:58`, p. ej. `src/screens/StartScreen.tsx:1` |
| **shadcn/ui** | Base de los componentes `src/components/ui/*` | https://ui.shadcn.com | MIT («Copyright (c) 2023 shadcn») | **[SIN VERIFICAR]** cuánto código original queda. Como mínimo, cortesía | `components.json:2`, `src/components/ui/button.tsx:1-20` |

Ninguna de las tres fuentes OFL declara Reserved Font Name.

### 3.2 JavaScript (dentro del bundle)

Todo es MIT salvo donde se indica. La licencia exige incluir el aviso y el texto con la app, no mencionarlo en la interfaz.

| Paquete | Para qué | Enlace | Copyright / licencia | Código |
|---|---|---|---|---|
| React 19.3.1, react-dom, scheduler | Motor de la interfaz | https://react.dev | Meta Platforms, Inc. and affiliates | `src/main.tsx:2` |
| Radix UI (19 paquetes directos y 29 internos) | Componentes accesibles | https://www.radix-ui.com/primitives | Copyright (c) 2022 WorkOS | `src/components/ui/*` |
| Floating UI | Posiciona popovers y tooltips | https://floating-ui.com | Floating UI contributors | transitivo |
| TanStack Virtual | Listas virtualizadas | https://tanstack.com/virtual | Tanner Linsley | `src/components/library/views/*View.tsx:1`, `src/hooks/useActiveRange.ts:1` |
| zustand | Estado global | https://github.com/pmndrs/zustand | Paul Henschel | `src/stores/*` (12 stores) |
| sonner | Notificaciones | https://sonner.emilkowal.ski | Emil Kowalski | `src/components/ui/toaster.tsx:2` |
| clsx | Combina clases CSS | https://github.com/lukeed/clsx | Luke Edwards | `src/lib/utils.ts:1` |
| tailwind-merge | Resuelve conflictos entre clases | https://github.com/dcastil/tailwind-merge | Dany Castillo | `src/lib/utils.ts:2` |
| class-variance-authority | Variantes de estilo | https://github.com/joe-bell/cva | **Apache-2.0**, «Copyright 2022 Joe Bell» | `src/components/ui/button.tsx:2` y otros 7 |
| @tauri-apps/api | Puente entre webview y Rust | https://tauri.app | Apache-2.0 OR MIT, «Tauri Apps Contributors» | `src/ipc/tauriBackend.ts:1-3` |
| @tauri-apps/plugin-dialog | Diálogo nativo | https://github.com/tauri-apps/plugins-workspace | MIT OR Apache-2.0. No trae los textos de licencia | `src/ipc/tauriBackend.ts:4` |
| Tailwind CSS (+ tailwindcss-animate) | Genera el CSS; su reset base (preflight) acaba en el bundle | https://tailwindcss.com | Tailwind Labs, Inc. / Jamie Kyle | `tailwind.config.ts:1-2` |
| Vite (polyfill de `modulepreload`) | Una parte mínima de su código acaba en el bundle | https://vite.dev | MIT. **[SIN VERIFICAR]** el año del copyright | `dist/assets/index-*.js` |
| Transitivas: react-remove-scroll y su familia, aria-hidden, get-nonce, detect-node-es, tslib (0BSD) | Bloquean el scroll en los modales; utilidades | https://github.com/theKashey/react-remove-scroll | MIT. **[SIN VERIFICAR]** el LICENSE de react-remove-scroll-bar, que no lo trae | transitivo |

Solo de compilación (crédito opcional): vite, rolldown, @vitejs/plugin-react, postcss, autoprefixer, svgo, svg-path-bbox (BSD-3), @resvg/resvg-js (MPL-2.0), lightningcss (MPL-2.0), TypeScript (Apache-2.0), vitest, eslint, prettier y @tauri-apps/cli.

### 3.3 Rust (crates destacados que van en el binario)

| Crate | Para qué | Enlace | Licencia |
|---|---|---|---|
| Tauri, wry, tauri-plugin-dialog, -fs, -opener, -updater | Base de la app de escritorio, webview, diálogos, abrir enlaces y actualizaciones | https://tauri.app · https://github.com/tauri-apps/wry · https://github.com/tauri-apps/plugins-workspace | Apache-2.0 OR MIT |
| SQLite (vía rusqlite / libsqlite3-sys 0.38.2) | Caché y base de datos local | https://sqlite.org | Dominio público; crate MIT. Cortesía |
| ureq, reqwest, hyper-rustls | Peticiones HTTP | https://github.com/algesten/ureq · https://github.com/seanmonstar/reqwest | MIT OR Apache-2.0 |
| rustls, ring, webpki-roots | TLS | https://github.com/rustls/rustls · https://github.com/briansmith/ring | ring: **Apache-2.0 AND ISC**, con textos de BoringSSL. webpki-roots: **CDLA-Permissive-2.0**, cuyo texto hay que incluir |
| image (png, jpeg, webp, gif, bmp, tga) | Decodificar y redimensionar imágenes | https://github.com/image-rs/image | MIT OR Apache-2.0 |
| zip + deflate64, sevenz-rust2, tar, ruzstd, lz4_flex, twox-hash, bzip2 / libbz2-rs-sys | Descomprimir archivos de ROM | https://github.com/hasenbanck/sevenz-rust2 · https://github.com/anatawa12/deflate64-rs · https://github.com/trifectatechfoundation/libbzip2-rs | Mayoritariamente MIT/Apache; bzip2-1.0.6 |
| brotli | Recursos web empaquetados (vía tauri-utils) | https://github.com/dropbox/rust-brotli | BSD-3-Clause AND MIT |
| directories → option-ext | Rutas de usuario | https://github.com/soc/option-ext | **MPL-2.0**: hay que indicar dónde se obtiene el código fuente |
| ICU4X (zerovec, icu_*) | Unicode y normalización | https://github.com/unicode-org/icu4x | **Unicode-3.0**: el aviso debe ir «with all copies» |
| tempfile, filetime | Archivos temporales del actualizador; fechas en tar | https://github.com/Stebalien/tempfile | MIT OR Apache-2.0 |

En Linux, GTK3 y WebKitGTK son LGPL-2.1+. Si la AppImage los empaqueta, hace falta un aviso LGPL. **[SIN VERIFICAR]** en un bundle real. En Windows no se ha comprobado si se distribuye WebView2Loader. **[SIN VERIFICAR]**.

---

## 4. Recuento de dependencias transitivas por licencia y avisos obligatorios

### 4.1 Rust (en runtime; `cargo tree -e normal,no-proc-macro`, sin crates propios)

Hay unos 262 crates en macOS, unos 262 en Windows, unos 329 en Linux y **381 en la unión de los tres**. El inventario anterior daba unos 252 para macOS; la diferencia probablemente viene del deduplicado. Recuento de la unión:

| Licencia (SPDX, agrupadas las variantes) | Crates |
|---|---|
| MIT OR Apache-2.0 (todas las grafías) | 229 |
| MIT | 87 |
| Unicode-3.0 | 15 (+1 combinada) |
| Zlib OR Apache-2.0 OR MIT (variantes) | 14 |
| Unlicense OR MIT | 7 |
| Apache-2.0 | 4 |
| BSD-3-Clause | 3 (+2 OR Apache, +1 /MIT, +1 AND MIT) |
| Apache-2.0 OR ISC OR MIT | 3 |
| Apache-2.0 WITH LLVM-exception OR … | 2 |
| Zlib | 2 |
| ISC | 2 |
| Una sola: bzip2-1.0.6, **MPL-2.0** (option-ext), **CDLA-Permissive-2.0** (webpki-roots), **Apache-2.0 AND ISC** (ring), Apache-2.0 AND MIT, (MIT OR Apache-2.0) AND Unicode-3.0, CC0-1.0 OR MIT-0 (OR Apache-2.0), 0BSD OR … | 1 cada una |


### 4.2 JavaScript

- **Producción**, según `pnpm licenses list --prod`: **74 paquetes**.

  | Licencia | Paquetes | Cuáles |
  |---|---|---|
  | MIT | 69 | — |
  | Apache-2.0 OR MIT | 1 | @tauri-apps/api |
  | MIT OR Apache-2.0 | 1 | plugin-dialog |
  | Apache-2.0 | 1 | cva |
  | ISC | 1 | lucide-react |
  | 0BSD | 1 | tslib |

- **Desarrollo y producción juntos**: unos 554 paquetes. Los de desarrollo no se distribuyen y no obligan a nada.

### 4.3 Avisos que legalmente deben viajar con la app

- **Textos completos de licencia:** MIT, Apache-2.0, ISC, BSD-3-Clause, MPL-2.0 (o la indicación de dónde está el código fuente de option-ext), CDLA-Permissive-2.0, Unicode-3.0 y OFL-1.1 (las tres fuentes). Zlib y bzip2 solo piden conservar el aviso en el código fuente; en el binario es de cortesía.
- **Copyrights concretos:** Joe Bell (cva), Tauri, Lucide y Cole Bemis, BoringSSL (ring), y los de cada crate y paquete MIT.
- **CC BY 4.0 de RetroArch Assets:** autor, enlace a la licencia, aviso de cambios y exención de garantías. Va en la propia pestaña.
- **ScreenScraper (BY-NC-SA):** crédito visible en la pestaña.

### 4.4 Cómo generarlo al compilar

- **Rust:** `cargo about generate` con `about.toml` y una plantilla. Hay que excluir proc-macros, build-deps y dev-deps, y limitarlo a los targets macOS, Windows y Linux. Alternativa: `cargo-deny` para el control y `cargo-about` para generar el texto.
- **JS:** `pnpm licenses list --prod --json` o `license-checker-rseidelsohn --production`, también con plantilla. Otra opción es `rollup-plugin-license` / `vite-plugin-license`, que solo recogen lo que de verdad entra en el bundle.
- **Recursos manuales:** las OFL, CC-BY y GameTDB se mantienen en un `credits.toml` o JSON propio. Así la pestaña y el archivo salen de la misma fuente.
- **Salida:** un único `THIRD-PARTY-NOTICES.txt` (o HTML) en `bundle.resources`, que la pestaña abra en local, más un check en la comprobación local de licencias (`check-licenses`) que falle si está desactualizado.

---

## 5. Previstas: añadir al integrarlas

| Fuente | Uso previsto | Enlace | Licencia | Crédito |
|---|---|---|---|---|
| **blawar/titledb** | Metadatos por title ID de Switch | https://github.com/blawar/titledb | MIT (comprobado con `gh api`) | Aviso MIT con la app |
| **GameTDB** (directo, ítem 7) | Portadas y metadatos de Wii, GameCube, Wii U y PS3 | https://www.gametdb.com | Permiso de su FAQ | **Enlace obligatorio** según el FAQ, y avisarles del proyecto |
| **Wikidata** | Metadatos de juegos | https://www.wikidata.org | CC0 **[SIN VERIFICAR en esta pasada]** | Cortesía |
| **xemu xdb / x360db** | Title IDs de Xbox y Xbox 360 | (hay que confirmar los repos oficiales) | **[SIN VERIFICAR]**, al parecer sin licencia | Pendiente |
| **IGDB** | Metadatos | https://www.igdb.com | **[SIN VERIFICAR]**: términos de Twitch | Pendiente; probablemente exija atribución |
| **TheGamesDB** | — (no hay implementación) | https://thegamesdb.net | Código GPL-3.0; términos de la API **[SIN VERIFICAR]** | Solo si se implementa |

---

## 6. Problemas a corregir junto a la pestaña

1. **Los textos de licencia no viajan con la app.** `src-tauri/tauri.conf.json:46-49` solo empaqueta `LICENSE` y `THIRD-PARTY-NOTICES.md`. Faltan `LICENSES/*.txt` y `src/assets/fonts/OFL-*.txt`, aunque `THIRD-PARTY-NOTICES.md:5-7` diga lo contrario.
   - En el bundle JS hay 0 comentarios `@license`.
   - **[SIN VERIFICAR]** si los `.woff2` conservan la licencia en la tabla `name`.
2. **TheGamesDB** aparece como fuente en `THIRD-PARTY-NOTICES.md:128-130` y en `docs/09-legal/license.md:43`, pero no hay ningún proveedor en el código. Hay que quitarlo o marcarlo como previsto.
3. **`THIRD-PARTY-NOTICES.md:133-134`** remite a unos términos por fuente en `docs/09-legal/` que no existen (allí solo están `license.md` y `trademark.md`).
4. **Faltan en los avisos:**
   - las dependencias JS MIT y Apache;
   - wry, reqwest, brotli, option-ext (MPL), webpki-roots (CDLA), ring (ISC + BoringSSL) y Unicode-3.0;
   - shadcn/ui.
5. **GameTDB:** añadir el enlace a gametdb.com. Afecta a GameCube, Wii, Wii U y PS3.
6. **Texto de interfaz «ScreenScraper lo conoce como»:** no aparece en `src/`. Hay que localizar la clave i18n real antes de citarla.
7. **Landing** (`coverly-roms-landing`): las capturas muestran iconos de Lucide, pero sus avisos dicen «uses no third-party code». Conviene añadir una línea de crédito a Lucide. El resto de capturas **[SIN VERIFICAR]**.

## 7. Pendiente de verificar

- Términos de SteamGridDB (hay que leerlos en un navegador real).
- Condiciones de DAT-o-MATIC (No-Intro).
- La frase original de Redump.
- TOSEC y el DAT de MAME.
- World of Spectrum, Clean CPC DB, tic80.com, WASM-4, CaveStory.org y robloach/libretro-dats.
- Wikidata, xemu xdb y x360db, e IGDB.
- La tabla `name` de los `.woff2`.
- Cuánto código de shadcn/ui queda.
- LGPL de GTK y WebKitGTK en la AppImage, y WebView2Loader en Windows.
- El copyright de Vite.


