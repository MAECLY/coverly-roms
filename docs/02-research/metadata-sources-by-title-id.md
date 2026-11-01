# Fuentes de metadatos por title ID / serial para Coverly ROMs

Sí, ya está investigado. Hay fuentes reales en internet, pero **ninguna lo da todo con una licencia limpia**. Los datos de texto (nombre, editora, fecha) se pueden cubrir bastante bien. Las imágenes oficiales (icono, banner, capturas) no las licencia nadie: son siempre de Nintendo, Sony, Microsoft o la editora.

Comprobado el 01-11-2026 con las fuentes originales. No he escrito en el repo ni he tocado /Volumes ni ~/Documents/ROMS.

## Reglas para todas las fuentes

- **Bajo demanda y en el equipo del usuario:** cada usuario descarga y guarda en caché en su máquina. No se meten datos, derivados ni imágenes en el repo ni en el instalador. GameTDB, ScreenScraper (NC-SA) y RAWG ("No data redistribution") lo exigen así.
- **Crédito visible** con enlace a cada fuente.
- **User-agent propio** y caché larga, de 30 días para los volcados.
- **Licencias:** ninguna fuente de terceros se puede relicenciar como GPL/AGPL. Solo Wikidata (CC0) y Redump dan permiso explícito de reutilizar los datos. Vita3K es MIT, pero su XML solo trae IDs y estado de compatibilidad.

## Switch

| # | Fuente | Licencia / términos | Qué da | Nota |
|---|---|---|---|---|
| 1 | blawar/titledb (y nlib-api, que es una copia) | El código es MIT. Los datos vienen de la eShop | Nombre, editora, fecha, versión, región, y URLs de icono, banner y capturas | Ya estaba estudiado. Las imágenes están en el CDN de Nintendo: solo se muestran en local y nunca se empaquetan |
| 2 | Wikidata P11072 (title ID) | **CC0** | Nombre, editora, fecha, y enlaces a IGDB, MobyGames y SteamGridDB | 2.608 juegos. Sirve de puente hacia SteamGridDB por ID. No usar la URL de formato de tinfoil.io |
| 3 | GameTDB SwitchTDB | Sin licencia formal. El FAQ da permiso informal | Ficha y carátulas | **No tiene title ID** (0 entradas). Solo encuentra por código de producto de 5 caracteres, que no va en el nombre del NSP. Hoy no sirve |
| — | Descartadas | | | nswdb (scene), tinfoil (propietaria), NX-DB (sin licencia), buscador e imágenes de Nintendo Europa (su página de Copyright dice *"Nintendo does not grant permission…"*) |

## 3DS

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | GameTDB 3DSTDB | Sin licencia formal. El FAQ dice *"anyone to use in any game-related project"* | Título por idioma, editora, fecha, región, hashes y carátulas (6.959) | 4.785 juegos, por ID de producto de 4 caracteres |
| 2 | No-Intro (DAT-o-MATIC) | No verifiqué sus términos para Nintendo. El informe de Sony los da por permisivos | Serial y hashes | Solo para identificar |
| — | Descartadas | | | 3dsdb.com (rastreador de releases de la scene), hax0kartik/3dsdb (sin licencia y con carpeta `keys/`), samurai/ninja (necesitan certificado de consola). 3DBrew solo como referencia |

## Wii U

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | GameTDB WiiUTDB | Igual que GameTDB 3DS | Ficha y carátulas (4.030) | 2.860 juegos: 2.403 IDs de 4 caracteres (eShop y VC) y 457 de 6 (discos). La cabecera WUD da `WUP-P-XXXX`, así que los discos se buscan por prefijo |
| 2 | No-Intro | Igual que en 3DS | Hashes de lo digital | Solo para identificar |
| — | WiiUBrew | Sin licencia | | Solo como referencia |

## Wii / GameCube

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | GameTDB WiiTDB | Sin licencia formal. Dolphin la usa | Ficha y carátulas: 18.224 covers, 13.148 full y 5.267 de disco | Wii 3.485, GC 1.708, WiiWare 998 |
| 2 | Redump | *"publicly available information to be used however people see fit"* | Hash de disco y serial | **La única con permiso explícito.** Identifica, pero no trae arte |
| 3 | libretro-thumbnails / ScreenScraper | Ya integradas | Carátulas por nombre | Respaldo |

(DS también la cubre GameTDB: 6.735 juegos de DS, 1.599 de DSiWare y 10.832 covers.)

## PS3

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | GameTDB PS3TDB | Sin licencia formal, igual que el resto de GameTDB | Título por idioma, editora, desarrolladora, fecha, región, género y carátulas | 10.766 juegos. El serial coincide con el TITLE_ID de PARAM.SFO |
| 2 | Wikidata P5944/P5971/P5999 | CC0 | Nombre, editora, fecha y enlaces | Unos 730 de PS3. Hay que filtrar DLC por el sufijo del Content ID |
| 3 | Compatibilidad de RPCS3 | Sin licencia | ID y estado | 6.798 IDs, pero el 61 % sin nombre |
| 4 | No-Intro / Redump DAT | No-Intro permisivo; Redump con permiso explícito | Serial y hashes | No tocar la sección "Disc Keys" de Redump |
| — | `-ver.xml` de Sony | Términos de Sony: solo *"personal, non-commercial"* y "NO DATA MINING" | Versión del parche | Solo como consulta puntual, no para montar una base |

## Vita

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Wikidata (Store IDs) | CC0 | Nombre, editora y fecha | Cubre poco |
| 2 | No-Intro | Permisiva | Serial y nombre | Para identificar |
| 3 | Compatibilidad de Vita3K | MIT | Title ID y estado (3.359) | El nombre sale de la API de issues de GitHub, que tiene sus propios límites |
| — | tmdb2 de Sony | Necesita un HMAC del firmware | Icono y fondo | **Descartado** |

## PSP

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | No-Intro | Permisiva | Serial y nombre | |
| 2 | Redump | Permiso explícito | Hashes de 3.500 discos | 0 seriales en su DAT |
| 3 | libretro-thumbnails / ScreenScraper | Ya integradas | Carátulas | |

Ninguna fuente abierta da ficha completa por serial para PSP.

## PS4

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Wikidata P5944/P5971 y P12332 | CC0 | Nombre, editora, fecha, y enlaces a IGDB y SteamGridDB | CUSA 3.703. **La única vía libre** |
| 2 | SerialStation API | **Sin términos visibles** (403). El API dice "NOT FINAL" | Nombre, content ID y tmdb (icono) | 140.791 title IDs. Fuente gris: sus imágenes vienen del tmdb2 firmado de Sony y tiene errores de datos. Solo después de escribir al operador |
| 3 | TheGamesDB | Solo el código es GPL; los datos no tienen términos | Ficha y arte | Solo 492 con uid. Necesita clave |
| 4 | Compatibilidad de shadPS4 | Sin licencia | ID y estado | El campo `version` es la versión del emulador, no la del juego |

## PS5

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Wikidata | CC0 | Nombre, editora y fecha | Solo 388 PPSA |
| 2 | SerialStation | Igual que en PS4 | | Pendiente de revisar a mano |

Para PS5 prácticamente no hay nada usable. Se puede caer a SteamGridDB por nombre.

## Xbox (original)

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Cabecera XBE (local) | No consulta nada | Title ID, nombre e icono | Ya en claro |
| 2 | xemu-project/xdb | **Sin licencia.** Solo *"for identification purposes"* | Nombre, regiones y ediciones; portada delantera y trasera, disco y `xtimage` | 1.025 títulos. Las carátulas vienen de MobyGames: no empaquetar. **La mejor por title ID** |
| 3 | libretro-thumbnails / ScreenScraper | Ya integradas | 1.326 carátulas | Respaldo |
| 4 | Redump | Permiso explícito | Hashes de 2.683 discos | Para identificar |

## Xbox 360

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Cabecera STFS/LIVE/CON/GoD (local) | No consulta nada | Title ID, nombre y miniatura PNG | No lo he probado con ficheros reales |
| 2 | xenia-manager/x360db | Sin licencia | Título, carátula, género, desarrolladora, editora y fecha | 5.886 juegos. Las carátulas están en `download.xbox.com`, que **solo responde por HTTP** y puede desaparecer |
| 3 | Compatibilidad de Xenia | El repo es BSD-3, el texto de las issues no | Nombre | Respaldo |
| — | Descartadas | | | XboxUnity (keywords "jtag" y "download"), DBox (403, sin verificar). La Microsoft Store no conoce los IDs de 360 |

## Xbox One / Series

| # | Fuente | Licencia | Qué da | Nota |
|---|---|---|---|---|
| 1 | Catálogo de la Microsoft Store (`displaycatalog.mp.microsoft.com`) | Microsoft ToU: nada se copia *"unless expressly permitted"*. Las Game Content Usage Rules **solo cubren juegos de Microsoft Studios**, son revocables y exigen aviso | Nombre, editora, fecha y 8 tipos de imagen | Busca por XboxTitleId en decimal. Sin clave. Consultar en vivo y cachear solo en local |
| 2 | Game Pass (`catalog.gamepass.com`) | Igual que la Store | Lista del catálogo | Respaldo |

Para juegos de otros editores no hay ningún permiso: las imágenes se usan solo para identificar el juego.

## Fuentes generales (por nombre o con clave)

- **IGDB:** gratis para uso no comercial, pero necesita cuenta de Twitch con 2FA, cada usuario pone su clave y el límite es 4 peticiones por segundo. Sirve como opcional, igual que SteamGridDB.
- **RAWG:** solo no comercial, 20.000 peticiones al mes y sin redistribución. Busca por nombre.
- **MobyGames:** la API es de pago. Descartada.
- **LaunchBox:** 190.000 juegos, pero sin términos y sin title IDs de consola moderna. Descartada.
- **TheGamesDB:** casi no tiene uids de consolas modernas y el campo de hashes está vacío. Secundaria.

## Recomendación: qué integrar primero

1. **GameTDB (un solo proveedor para 3DS, Wii U, Wii, GC, DS y PS3).** Es lo que más cobertura da con menos trabajo. Antes de integrarlo:
   - Escribirles para pedir permiso por escrito y preguntar si añadirían title IDs de Switch.
   - Descargar el XML bajo demanda con caché de 30 días y user-agent propio.
   - Respetar el `Crawl-Delay` con las imágenes de `art.gametdb.com`.
   - Poner un enlace visible.
   - **No** copiar el modelo de Mimir de llevar los datos dentro de la app.
2. **Wikidata SPARQL (CC0) como puente entre sistemas.** Lleva del title ID de Switch y del content ID de PS3, PS4, PS5 y Vita al nombre, la editora y la fecha, y además da IDs de SteamGridDB e IGDB. Así se reutiliza el proveedor de SteamGridDB que ya existe para las carátulas. Es la única fuente libre para PS4 y PS5.
3. **Xbox:**
   - Leer en local las cabeceras XBE y STFS/GoD.
   - **xemu xdb** para la Xbox original.
   - **Catálogo de la Microsoft Store** para One/Series.
   - **x360db** para 360, aceptando el HTTP del CDN y el riesgo de que desaparezca.
4. **Redump y No-Intro** para identificar por hash o serial. Redump es la única con permiso explícito.
5. **Switch:** seguir con titledb (ya decidido) más Wikidata.

**Pendiente, sin integrar:** SerialStation (revisar a mano quién la lleva y sus términos), DBox, y los términos de No-Intro para los sistemas de Nintendo.

**Descartadas definitivamente:**
- 3dsdb.com, hax0kartik/3dsdb, nswdb y XboxUnity, por su relación con la scene o con claves.
- Tinfoil, por ser propietaria.
- El buscador y las imágenes de Nintendo Europa, porque su página de Copyright lo prohíbe.
- El tmdb2 de Sony (necesita un HMAC del firmware) y el uso masivo de la web de Sony (sus términos lo prohíben).
- MobyGames (de pago) y LaunchBox (sin términos).

**Dónde no hay nada usable:**
- Imágenes oficiales con licencia: no existen para ninguna consola moderna.
- PS5: casi nada (388 IDs en Wikidata).
- PSP y Vita: solo identificación, sin ficha completa.
- Switch por title ID: solo titledb y Wikidata. GameTDB no sirve mientras no añada title IDs.

**Archivos de la verificación:**
