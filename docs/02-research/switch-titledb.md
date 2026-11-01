# Catálogo de títulos de Switch: blawar/titledb y alternativas (versión revisada)

**Respuesta corta:** sí, se puede usar blawar/titledb. Su archivo `LICENSE` es MIT y la licencia MIT es compatible con GPL/AGPL. Recomiendo no empaquetarlo: la app lo descargaría bajo demanda, como opción, y guardaría solo id, nombre, editor, fecha y región. Esto es lo que hacen RomM (AGPL-3.0) y NSC_BUILDER.

**Corrección importante sobre la premisa de Azahar.** La versión actual de Azahar sí trae las claves dentro. El `keys.txt` del usuario solo las sustituye. El ejemplo correcto de «el emulador sabe usar el dato pero no lo trae» son los emuladores de Switch, que exigen el `prod.keys` del usuario (ver la sección 0).

## Cambios respecto al informe anterior
| # | Afirmación anterior | Qué es correcto | Fuente |
|---|---|---|---|
| 1 | Azahar no incluye las claves; usa `aes_keys.txt` | Azahar compila por defecto un bloque de claves cifrado dentro del propio programa (opción `ENABLE_BUILTIN_KEYBLOB`, activada por defecto). El archivo que lee se llama **`keys.txt`** y está en la carpeta `sysdata`. Solo si ese archivo no existe usa las claves integradas | `CMakeLists.txt` L162 `option(ENABLE_BUILTIN_KEYBLOB ... ON)`; `src/common/common_paths.h` `#define KEYS_FILE "keys.txt"`; `src/core/hw/aes/key.cpp` L292-313; `src/core/hw/default_keys.h` (https://github.com/azahar-emu/azahar). La PR #2024 (2026-04-12) añadió la opción para *desactivarlo* |
| 2 | switch-library-manager descarga titledb | **Falso.** Usa `https://tinfoil.media/repo/db/titles.json`. Las URL de titledb están comentadas | `settings/settings.go` L23-26 (https://github.com/giwty/switch-library-manager) |
| 3 | DBI descarga titledb | **No verificable.** Es de código cerrado. Lo quito | — |
| 4 | «Ninguno lo empaqueta» | Es **matizable**. Ninguno lo mete en su programa, pero **ownfoil vuelve a alojar una copia completa**: el release `titledb` de su GitHub contiene `titles.{REGION}.{lang}.json.zst` (unos 26 MB cada uno), `cnmts.json.zst`, etc. Se reconstruye con cada commit de blawar | `app/constants.py` L19 `TITLEDB_RELEASE_URL`; `gh api repos/a1ex4/ownfoil/releases/tags/titledb` |
| 5 | nlib-api se sincroniza a diario | **Dos veces al día** (3:00 y 15:00). Además usa `nx-missing.ghostland.at` | README de ghost-land/nlib-api, sección «Switch — TitleDB» |
| 6 | switchbrew tiene «unas 1.000+ filas» | Unas **2.381 filas** con ProgramId | `action=raw` de Title_list/Games, contando las filas que empiezan por `0100…` |
| 7 | Índice local reducido < 2 MB | US.en.json reducido a `id → {name, publisher, releaseDate, region}` da 29.572 ids: **3,6 MB en JSON y 0,76 MB en gzip** | Medido en local |
| 8 | Cifras de campos | `ids` aparece en 10.565 entradas, pero solo 10.051 tienen valor. `parentId` aparece en 1.262 y tiene valor en 1.261. `iconUrl` tiene valor en 19.601. Faltaba el campo `nsuId`. Hay 61 archivos de región. 29.774 entradas tienen titleId, que corresponden a 29.572 titleIds distintos | Análisis local de US.en.json |

## 0. Analogía con las claves: corregida
- **Azahar (3DS), GPL-3.0 según GitHub.** Si existe `sysdata/keys.txt`, lo lee. Si no, descifra las claves que lleva integradas, mientras `ENABLE_BUILTIN_KEYBLOB` siga activado, que es lo normal. El comentario del código dice: *"The key data is encrypted in the source to prevent easy access to it for unintended purposes"* (key.cpp L303-304). Por tanto, **Azahar no es el modelo de «no trae las claves»**: ofusca las claves, pero las distribuye.
- Para el modelo que busca Miguel, el ejemplo válido es la extracción de `control.nacp` en Switch. Para leerlo hacen falta el `prod.keys` y los titlekeys del usuario, y los emuladores de Switch no los incluyen. Esto no lo he vuelto a comprobar en el código de cada emulador. El repositorio original de Ryujinx ya no está en GitHub.
- Conclusión: **la ruta con claves debe pedir siempre el archivo al usuario, y coverly-roms no debe integrar ninguna clave, ni ofuscada.** Que Azahar lo haga no es un precedente legal seguro.

## 1. blawar/titledb (verificado)
### Licencia
- `LICENSE`: MIT, "Copyright (c) 2019 Blake Warner". `gh api repos/blawar/titledb` devuelve `spdx_id: MIT`. **Confirmado.**
- README: *"The repository provides metadata only"*. No hay una licencia aparte para los datos. **Confirmado.**
- Issue #22 (2026-07-22, "No updates for TitleDB, NUT, and tinfoil.media"): la cita *"nobody knows how you generate it"* está en el cuerpo de la issue. **Confirmado.**
- Issue #21 (2026-06-26, "No titledb update since middle of May"): la cita *"CDN deprecated access by old FWs"* está en un comentario. **Confirmado.**
- Marco legal:
  - **Feist v. Rural**, 499 U.S. 340 (1991): los hechos no tienen copyright (https://supreme.justia.com/cases/federal/us/499/340/). Es jurisprudencia muy conocida, pero no he podido volver a descargar la página en esta revisión.
  - **Directiva 96/9/CE, art. 7**: derecho *sui generis* sobre la «parte sustancial» de una base de datos (https://eur-lex.europa.eu/eli/dir/1996/9/oj). No he podido descargar EUR-Lex de forma automática; la referencia es correcta.
  - **LPI española, art. 133** «Objeto de protección»: exige una «inversión sustancial, evaluada cualitativa o cuantitativamente». **Confirmado en el BOE** (https://www.boe.es/buscar/act.php?id=BOE-A-1996-8930).
  - Nada de esto es un dictamen jurídico.

### Contenido (verificado con `gh api repos/blawar/titledb/contents`)
| Archivo | Tamaño | Clave |
|---|---|---|
| 61 archivos `{REGION}.{lang}.json` (US.en 90.699.593 B; ES.es 90.451.541 B) | unos 90 MB | nsuId |
| `cnmts.json` | 50,5 MB | titleId |
| `ncas.json` | 87,6 MB | ncaId |
| `cheats.json` | 12,0 MB | titleId → buildId |
| `versions.json` | 1,77 MB | titleId |
| `versions.txt` | 3,37 MB | titleId |
| `languages.json` | 1.674 B | región |

- **Campos de una ficha.** `id`, `nsuId`, `name`, `publisher`, `developer`, `releaseDate`, `region`, `languages`, `iconUrl`, `bannerUrl`, `frontBoxArt`, `screenshots`, `description`, `intro`, `category`, `rating`, `ratingContent`, `numberOfPlayers`, `size`, `version`, `rightsId`, `isDemo` y `key`. Algunas fichas tienen además `ids` y `parentId`.
- **El campo `key` está a `null` en las 37.473 entradas.** **Confirmado.**
- **Id de una actualización.** Switchbrew dice: *"Game updates have the same ProgramId as the main application, except with bitmask 0x800 set."* **Confirmado** (https://switchbrew.org/wiki/Title_list/Games).
- **Id base.** La función `getBaseId` de nut devuelve `titleIdNum & 0xFFFFFFFFFFFFE000`. **Confirmado** (https://github.com/blawar/nut/blob/master/nut/Title.py, función `getBaseId`, hacia la línea 73).

### Frecuencia de actualización
- Último commit de US.en.json y ES.es.json: **2026-09-14T05:33Z**. Último de versions.txt: **2026-10-16T05:24Z**. Los commits recientes solo tocan «versions» y «nca». **Confirmado.**
- Instantáneas históricas: titledb_02112024, titledb_12032024 y titledb_04082025. **Confirmado** en el README.

### Uso en otros proyectos (corregido)
- **RomM (AGPL-3.0).** `backend/tasks/scheduled/update_switch_titledb.py` L33 usa `url="https://raw.githubusercontent.com/blawar/titledb/master/US.en.json"`. **Confirmado.**
- **NSC_BUILDER (MIT).** `py/zconfig/NUT_DB_URL.txt` apunta a `US.en.json` y `versions.json` de blawar, con refresco cada 24 h. **Confirmado.**
- **ownfoil (AGPL-3.0).** Consume una **copia propia** desde su release de GitHub, con refresco por defecto cada 12 h (`titledb_update_interval: "12h"`).
- **nut (GPL-3.0).** Es del mismo autor y genera el catálogo. **Confirmado.**
- **switch-library-manager** usa tinfoil.media, no titledb. **DBI** no es verificable. Los dos salen de esta lista.

## 2. Alternativas (verificado)
| Fuente | Licencia | Datos comprobados | Valoración |
|---|---|---|---|
| ghost-land/nlib-api | GPL-3.0 | `GET /nx/:tid?lang=`, 11 idiomas. Se sincroniza con blawar/titledb a las 3:00 y a las 15:00 | Respaldo para consultas puntuales. Por detrás son los mismos datos |
| ghost-land/NX-DB | **sin licencia** (`license: null`) | `fulldb.json` de 70.064.267 B | Descartado |
| switchbrew Title list | El wiki no declara licencia (`rightsinfo` vacío) | Unas 2.381 filas | Solo como referencia técnica |
| nswdb.com/xml.php | Sin términos publicados | XML con `<group>`, `<releasename>` (por ejemplo "…NSW-BigBlueBox"), `<titleid>` y `<imgcrc>` | Descartado por su vínculo con la scene |
| ninstar/rich-presence-u-db | ODbL-1.0 | `titles/hac.csv` con 899 líneas y cabecera `ID,US,EU,JP,US TITLE,…`. La clave es un slug y no aparece **ningún** titleId `0100…` | No sirve para resolver id → nombre |
| 16BitWonder/nx-versions | sin licencia | — | Descartado |
| tinfoil.media | sin licencia publicada | No lo he comprobado más | Descartado |
| NACP local | — | Requiere las claves del usuario | Lo más exacto (ver la sección 0) |

## 3. Recomendación para coverly-roms (ajustada)
1. **No meter titledb en el repositorio ni en el programa.** Ofrecer una opción «Catálogo de nombres de Switch (descarga opcional)».
   - Descargaría un archivo de región, `ES.es.json` o `US.en.json`, de unos 90 MB.
   - Lo actualizaría como mucho una vez por semana usando ETag. Actualizarlo más a menudo no aporta nada, porque los archivos de región llevan detenidos desde el 2026-09-14.
2. **Reducirlo al importar** a `id → {name, publisher, releaseDate, region}`. Ocupa 3,6 MB, o 0,76 MB en gzip. Las descripciones y las capturas se descartan.
3. **Calcular las relaciones base/update/DLC a partir del propio id**, con `& 0xFFFFFFFFFFFFE000` y el bit `0x800`. Así se pueden agrupar juegos y detectar duplicados por id sin catálogo, acoplado al sistema de duplicados que ya existe.
4. **Ruta opcional con claves.** El usuario indica su `prod.keys` y la app lee `control.nacp`.
   - **No integrar claves, ni siquiera ofuscadas** (a diferencia de Azahar). No enlazar a dónde conseguirlas.
   - Orden de prioridad: NACP → catálogo → nombre del archivo.
5. **Iconos:** se descargan bajo demanda desde `iconUrl` y se guardan solo en caché local. No se redistribuyen.
6. **Atribución:** "Switch title names: blawar/titledb (MIT, © 2019 Blake Warner); game names and artwork © their respective owners", más el texto MIT si se conserva alguna copia.
7. **Si titledb falla**, la app usa el nombre del archivo. Como fuente alternativa configurable: `api.nlib.cc/nx/:tid`. Se puede añadir como tercera opción la copia de ownfoil en GitHub Releases, que está comprimida en zstd y alojada en GitHub.

Archivos de trabajo:
