# Capacidades de cada fuente frente a lo que usa Coverly

Este documento junta cinco auditorías verificadas el 2026-11-01:
- libretro;
- ScreenScraper;
- SteamGridDB, titledb y GameTDB;
- IGDB, TheGamesDB, RAWG, Wikidata y Steam;
- Xbox, Redump/No-Intro y RetroAchievements.

No se ha escrito nada en el repo.

**Marcas:**
- **U**: Coverly ya lo usa.
- **D**: la fuente lo ofrece y Coverly no lo usa.
- **D?**: la fuente lo ofrecería, pero está sin verificar.
- **·**: la fuente no lo ofrece o no aplica.
- **[NV]**: dato sin verificar.

**Columnas:**

| Sigla | Fuente |
|---|---|
| LT | libretro-thumbnails |
| LDB | libretro-database |
| SS | ScreenScraper |
| SGDB | SteamGridDB |
| GTDB | GameTDB |
| TDB | blawar/titledb |
| IGDB | IGDB |
| TGDB | TheGamesDB |
| RAWG | RAWG |
| WD | Wikidata |
| Steam | Steam |
| xdb | xemu-project/xdb |
| x360 | xenia-manager/x360db |
| MSS | Microsoft Store (displaycatalog) |
| RA | RetroAchievements |
| Loc | cabeceras locales: XBE, XEX, STFS, GDK, ERA, sniff y la fuente embebida |

---

## 1. Matriz resumen

### 1.1 Arte

| Capacidad | LT | LDB | SS | SGDB | GTDB | TDB | IGDB | TGDB | RAWG | WD | Steam | xdb | x360 | MSS | RA | Loc |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Portada | **U** | · | **U** box-2D | **U** solo vertical estática | D cover/coverHQ | · (`frontBoxArt` es null) | D | D | · | D? | D capsule | D | D | D BoxArt/Poster | D? | **U** embebida |
| Contraportada | · | · | D box-2D-back | · | D coverfull | · | · | D | · | · | · | D | · | · | · | · |
| Caja 3D | · | · | D? box-3D | · | D cover3D | · | · | · | · | · | · | · | · | · | · | · |
| Cartucho o disco | · | · | D support-* | · | D disc | · | · | · | · | · | · | D media.jpg | · | · | · | · |
| Ancha | · | · | D steamgrid | D | · | D bannerUrl | · | D banner | · | · | D header | · | D? banner | D? | · | · |
| Cuadrada | · | · | · | D | · | D iconUrl | · | · | · | · | · | · | · | D | · | · |
| Fondo (hero) | · | · | D fanart | D heroes | · | · | D artworks | D fanart | D | · | D hero | · | D | D SuperHero | · | · |
| Logo, wheel o marquesina | D Named_Logos | · | D wheel/marquee | D logos | · | · | D | D? clearlogo | · | D? | D | · | · | · (solo apps) | · | D GDK/ERA |
| Icono | · | · | · | D | · | D | · | · | · | D? P8972 | · | · | D | · | D | D STFS/GDK/ERA |
| Captura | **U** | · | **U** ss | · | · | D | D | D | D | · | D | · | D | D | D? | · |
| Pantalla de título | **U** | · | **U** sstitle | · | · | · | · | D? | · | · | · | D? xtimage | · | · | D? | D? $$XTIMAGE |
| Vídeo | · | · | D MP4 | D animadas | · | · | D (enlace) | D MP4 o YouTube | D? (plan Business) | · | D | · | · | D (solo enlace) | · | · |
| Manual | · | · | D | · | · | · | · | · | · | · | · | · | · | · | D (guía) | · |
| Bezel | · | · | D (6 variantes) | · | · | · | · | · | · | · | · | · | · | · | · | · |
| Mapa | · | · | D maps | · | · | · | · | · | · | · | · | · | · | · | · | · |

### 1.2 Metadatos, identificación y logros

| Capacidad | LT | LDB | SS | SGDB | GTDB | TDB | IGDB | TGDB | RAWG | WD | Steam | xdb | x360 | MSS | RA | Loc |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Descripción | · | · | D por idioma | · | D | D | D | D | D | · | D | · | D? | D | · | D STFS |
| Género | · | D metadat | D | · | D | D | D | D | D | D | D | · | D? | D | D? | · |
| Desarrolladora | · | D (metadat y DAT de GameTDB) | D | · | D | D | D | D | D | D | D | · | D? | D? | D? | · |
| Editora | · | D (metadat y DAT de GameTDB) | D | · | D | D | D | D | D | D | D | · | D? | D | D? | D STFS |
| Fecha por región | · | D (año y mes; día en GameTDB, poco fiable) | D | D? (`release_date`) | D | D (archivo por región) | D | D (una sola) | D (una sola) | D | D | D? | · | D | D | · |
| Jugadores | · | D users/maxusers | D | · | D | D | D | D | · | · | · | · | · | D (coop) | · | · |
| Nota | · | D edge/famitsu | D (/20) | · | · | · | D | · | D | · | D? | · | · | D? | · | · |
| Clasificación por edad | · | D esrb/bbfc/elspa | D | · | D | D (ESRB) | D | D | D | D | · | · | · | D? | · | D XBE +0xA4 |
| Saga o serie | · | D franchise | D familles/numeros | · | · | · | D | · | D | D | · | · | · | · | · | · |
| Nombres alternativos | · | · | D noms | · | D? | D | D | D | · | D | · | · | · | · | · | D XBE +0x5C |
| Idiomas | · | · | D romlangues | D (en logos) | D | D | D | · | · | D | D | · | · | D? | · | · |
| Tiempo hasta terminar | · | · | · | · | · | · | D? | · | D playtime | · | · | · | · | · | · | · |
| Hash | · | **U** CRC+tamaño o SHA1 | **U** crc/md5/sha1 | · | D (solo en el XML) | · | · | D md5/crc | · | · | · | · | · | · | D (MD5 por sistema) | · |
| Serial | · | **U** (solo en Duplicados) y D metadat/serial | D serialnum | · | D ID6 | · | · | D uid | · | · | · | D | · | · | · | **U** sniff |
| Title ID | · | · | · | · | D? | D | · | · | · | D P11072 | · | D | D | D XboxTitleId | · | **U** XBE/XEX/STFS |
| Ids de plataforma | · | · | D gameid | D (steam, eshop…) | · | D nsuId | D external_games | · | D? | D P-ids | D appid | · | D? | **U** StoreId guardado | D GameID | · |
| Logros | · | · | D (marca) | · | · | · | · | · | · | · | · | · | · | · | D | · |

**Lo que hay hoy:**
- `Media` (`crates/coverly-core/src/profiles/mod.rs:126-137`) solo tiene `Cover`, `Screenshot` y `Title`.
- No hay modelo de metadatos por juego. `Candidate` (`sources/mod.rs:51-71`) solo lleva arte.
- `ArtQuery` (`sources/mod.rs:76-102`) no lleva `serial`, `title_id` ni ids externos.

---

## 2. Cada fuente: qué usamos, qué no y límites de licencia

**libretro-thumbnails**
- **Usamos:**
  - Boxarts, Snaps y Titles: `sources/libretro.rs:41-47`, con caché de 2 días (`:23-26`).
  - El emparejado por nombre: `:349-412`.
- **No usamos:**
  - `Named_Logos`, que es la fuente gratis y sin clave del hueco Logo.
  - El ETag de cada imagen, para avisar de «arte actualizado» [NV].
- **Licencia:** el repo no declara licencia (`license: null`) y el arte es de las editoras. Por eso, solo descarga bajo demanda y caché local.

**libretro-database**
- **Usamos:**
  - DAT de `dat/`, no-intro, redump, tosec, mame, fbneo y homebrew.
  - Búsqueda por CRC + tamaño (o SHA1): `identify/dat.rs:415-450`.
  - Búsqueda en todos los DAT para adivinar el sistema: `organize/evidence.rs:449`.
  - Serial y prefijo STFS, pero solo en Duplicados: `dedupe/source.rs:206-224`.
- **No usamos:**
  - `developer`, `publisher`, `releaseyear/month/day`, `users` y `esrb_rating` de los DAT de GameTDB que **ya se descargan**. Se ignoran en la rama `_ => {}` de `dat.rs:239`.
  - Las 30 carpetas de `metadat/`: genre, developer, franchise, esrb, analog, rumble, serial, hacks, mame-split, mame-nonmerged, mame-member, fbneo-member, tosec, etc.
  - `cloneof`: se escribe en `cache/mod.rs:405`, pero ningún `SELECT` lo lee.
  - El MD5 del DAT, para buscar.
  - `DatStore::by_name_prefix` (`dat.rs:459`), que no se llama fuera de los tests.
- **Ojo:** los DAT de GameTDB no traen CRC. En GameCube, Wii, Wii U y PS3 solo sirven por serial.
- **Licencia:**
  - CC-BY-SA-4.0. Si se empaquetan DAT, pasan a ser obligatorios BY y SA.
  - Lo que viene de GameTDB pide un enlace a gametdb.com.
  - El origen de las carpetas de `metadat/` no está documentado (`docs/adr/0004-license-agpl.md:39`).

**Playlists de RetroArch**
- **Usamos:** solo se escriben miniaturas, en `profiles/builtin.rs:659-682`.
- **No usamos:**
  - Los `.lpl`: favoritos e historial.
  - Los `.lrtl`: tiempo jugado y última partida.
- **Fallo confirmado en el código:** `profiles/template.rs:75` no cambia los caracteres ``&*/:`<>?\|"`` por `_`, y RetroArch los cambia al buscar (`gfx_thumbnail.c:2728`). Por eso no encuentra esas miniaturas.
- **Licencia:** es solo un formato de datos y no se copia código GPLv3.

**ScreenScraper**
- **Usamos:**
  - 4 de los 27 endpoints: `jeuRecherche` (`screenscraper.rs:467`), `jeuInfos` (`:496`), `mediaJeu` (`:556`/`:616`) y `ssuserInfos` (`:641`).
  - 4 tipos de medio: `box-2D`, `flyer`, `ss` y `sstitle` (`:40`).
  - Hash: con el contenido completo manda crc/md5/sha1; dentro de un zip, solo CRC (`:235-257`).
  - Cuota: lee `maxthreads` y la cuota diaria (`:422-437`).
- **No usamos:**
  - Ningún texto: synopsis, editeur, developpeur, dates, joueurs, note, genres, modes, familles/numeros, classifications, noms ni flags de ROM.
  - Unos 42 tipos de medio: fanart, wheel(-hd), marquee, box-2D-back/side, support-*, bezel-*, video, manuel, maps, steamgrid, figurine…
  - `serialnum`, `gameid`, `romtype` iso o dossier.
  - `maxwidth`, `maxheight` y `outputformat`.
  - El hash de la imagen local para recibir `CRCOK`.
  - La cuota ko (`requestskotoday`, `maxrequestskoperday`) y el reintento ante un 429.
- **Fallos:**
  - `parse_game` descarta los medios sin región (`:187-188`). Así se perderían fanart, steamgrid, video y marquee aunque se añadieran a la lista.
  - Organizar solo manda CRC (`organize/online.rs:39-50`).
- **Licencia:**
  - La API solo vale para aplicaciones gratuitas y distribuidas.
  - Los datos son CC BY-NC-SA 4.0: exportar textos a `gamelist.xml` hereda SA y obliga a poner la atribución.
  - Los medios, siempre bajo demanda y nunca en lote.
  - La v2 está en betatest: hay que pedir `mediasJeuListe` antes de usar tipos como `box-3D` o `mixrbv*` [NV].

**SteamGridDB**
- **Usamos:**
  - `/search/autocomplete` y `/grids/game/{id}`, con `600x900,342x482`, `types=static` y solo `Media::Cover` (`steamgriddb.rs:70-82`, `:363`, `:426`).
  - Los filtros nsfw, humor y epilepsia (`:58-67`), que la interfaz no expone.
- **No usamos:**
  - Portadas ancha y cuadrada, heroes, logos, icons, animadas, `styles`/`oneoftag`, `page`.
  - La búsqueda por id de plataforma (`/games/steam/{appid}`; eshop [NV]).
  - Los campos `thumb`, `author`, `language`, `notes`, `release_date` y `verified`.
- **Licencia:**
  - Los términos están [NV]: la página solo se ve con JavaScript.
  - El arte es de cada autor: crédito por autor y solo caché local.
  - La clave la pone el usuario.

**blawar/titledb**
- **Usamos:** no está integrado. TODO.md:218 y :442 lo dejan para el nombre por title ID.
- **Qué ofrece:**
  - 61 archivos `REGION.lang.json` con `description`, `developer`, `publisher`, `releaseDate`, `numberOfPlayers`, `rating` (edad), `languages`, `category`, `iconUrl`, `bannerUrl`, `screenshots`, `isDemo` y `size`.
  - `versions.json`, `cnmts.json` y `cheats.json`.
  - Los archivos van por `nsuId`, así que hay que montar un índice inverso `id` → entrada. Hay title IDs duplicados.
- **Licencia:**
  - MIT en el repo; el arte y los textos son de Nintendo o de cada editora.
  - La lista negra de títulos solo online vive en `blawar/nut`, que necesita su propio crédito.

**GameTDB**
- **Usamos:** solo el DAT, a través de libretro, y solo nombre y serial (`systems.toml:1246/1259/1290`).
- **No usamos:**
  - Los XML: ESRB, PEGI, CERO, GRB, géneros con nombre traducido, accesorios, idiomas, y crc/md5/sha1 en Wii y 3DS.
  - El arte: cover, cover3D, disc, coverfull(HQ) y coverHQ.
  - Las bases de DS, 3DS y Switch.
  - Un HEAD que da 404 detecta que no hay arte sin descargar nada.
- **Licencia:**
  - Exige enlace a GameTDB y pide avisarles del proyecto. No se puede usar en un sitio web sin su permiso.
  - `robots.txt` prohíbe a los rastreadores `*tdb.zip` y `art.gametdb.com` (`Disallow: *.*`). Así que solo bajo demanda y uno a uno.

**IGDB**
- **Usamos:** no está integrado.
- **Qué ofrece:**
  - Juegos con géneros, temas, franquicias y colecciones, `age_ratings`, `release_dates` por región, `alternative_names`, `language_supports` y `multiplayer_modes`.
  - Imágenes: covers, artworks, logos y screenshots (con `alpha_channel` y `animated`), y `game_videos` como enlace.
  - `external_games`: Steam, GOG, Xbox, PS Store de EE. UU.; **no** la eShop.
  - `platform_logos`.
- **Requisitos:** cuenta de Twitch con `client_credentials`; 4 peticiones por segundo y 8 abiertas a la vez.
- **Licencia:**
  - La introducción dice «non-commercial» y la FAQ dice «commercial too»: se contradicen.
  - Atribución visible en un sitio fijo. Cachear está permitido.

**TheGamesDB**
- **Usamos:** no está integrado, aunque `THIRD-PARTY-NOTICES.md:128-130` ya la lista como fuente.
- **Qué ofrece:**
  - Búsqueda `ByGameHash` (md5 o crc) y `ByGameUniqueID` (serial), con `filter[platform]`, y `filter[region_id]` en la búsqueda por nombre.
  - Datos: overview, players, genres, publishers, developers, rating, alternates, youtube.
  - Imágenes: boxart frontal y trasera, fanart, banner, screenshot, clearlogo [NV].
  - `Games/Videos` devuelve MP4 pesados.
  - `Platforms?fields=icon`.
- **Cupo:** mensual [NV].
- **Licencia:** no hay términos publicados para los datos.

**RAWG**
- **Usamos:** no está integrado.
- **Qué ofrece:** descripción, géneros, `released`, `rating` y Metacritic, `esrb_rating`, `playtime`, `game-series`, `short_screenshots` y `background_image`. `movies` puede exigir el plan Business [NV].
- **Licencia:**
  - El plan Free solo admite proyectos no comerciales, con 20.000 peticiones al mes.
  - Exige un enlace activo en cada vista con sus datos, sin clonar el servicio ni redistribuir los datos.
  - **Conclusión: no exportar datos de RAWG a los frontends.**

**Wikidata**
- **Usamos:** no está integrado.
- **Qué ofrece:**
  - Puente de ids: P11072 (title ID de Switch, 2.611 ítems), P5794 (IGDB), P12561 (SteamGridDB), P1733 (Steam), P5290 (Redump), P11789 (Xbox 360).
  - Etiquetas por idioma, género, saga, fechas por país y clasificaciones.
  - Camino de búsqueda: `haswbstatement` y después `wbgetentities`.
- **Límites del servicio de consultas SPARQL (WDQS):** 5 consultas en paralelo por IP, corte a los 60 s y User-Agent con contacto obligatorio.
- **Licencia:** los datos son CC0 [NV en credits-inventory.md:194]. Los archivos de Commons llevan cada uno su licencia.

**Steam**
- **Usamos:** no está integrado. TODO.md:481 lo pone en el puesto 5 del orden de fondos.
- **Qué ofrece:** capsule 600×900, header, hero, logo y fondo de página; `appdetails` (no oficial, [NV]); `IStoreBrowseService/GetItems` [NV].
- **Términos (Steam Web API Terms):**
  - 100.000 llamadas al día.
  - No sugerir afiliación con Valve.
  - Avisar al usuario de qué datos de Steam se guardan y publicar una política de privacidad.
  - Al terminar el acuerdo, borrar los datos.
  - No está claro si cubren `appdetails` o el CDN, que no usan clave [NV].

**xdb (Xbox)**
- **Usamos:** no está integrado.
- **Qué ofrece:**
  - `info.json` con releases por región y edición.
  - `cover_front.jpg`, `cover_back.jpg`, `media.jpg` y `xtimage.png`.
  - Ojo: `disc_id` es un índice por versión, no el número de disco.
- **Licencia:** `license: null` y el README dice «for identification purposes only». El arte viene de MobyGames. Solo en local y con crédito.

**x360db (Xbox 360)**
- **Usamos:** no está integrado.
- **Qué ofrece:** `boxart.jpg`, `background.jpg`, `banner.png`, `icon.png`, `screenlg*.jpg` e `info.json`.
- **Licencia:** `license: null`. Solo en local.

**Microsoft Store (displaycatalog)**
- **Usamos:** el StoreId se guarda en `consoles/mod.rs:162-165`, pero no hay proveedor.
- **Qué ofrece:**
  - Imágenes: BoxArt, Poster, SuperHeroArt, TitledHeroArt, FeaturePromotionalSquareArt y Screenshot. En juegos no hay `Logo`.
  - Campos: `Attributes`, `XboxCrossGenSetId`, `IsDemo`, `PackageIdentityName` y Videos (solo enlace).
- **Licencia:** no permite copiar ni retransmitir. Solo en vivo, con caché local, aviso de copyright y apagado por defecto.

**RetroAchievements**
- **Usamos:** nada. Solo hay citas en `disc/signatures.rs:23` y `:43`.
- **Qué ofrece:** `GetGameExtended` (logros, `GuideURL`, `ParentGameID`, número de jugadores), `GetGameHashes` (MD5) y `GetGameList` (paginada, con `DateModified`).
- **Ojo:** el MD5 de RA solo coincide con el de Coverly en algunos sistemas. Hace falta una tabla por sistema o rcheevos.
- **Licencia:** rcheevos es MIT; los términos de la web siguen [NV] (devuelven 403).

**Cabeceras locales de Xbox**
- **Usamos:**
  - XBE: title ID, nombre y versión (`xbox.rs:125-170`).
  - XEX: title ID, versión y disco.
  - STFS: tipo de contenido, title ID, nombre y la miniatura del contenido.
  - GDK y ERA.
- **No usamos:**
  - XBE: región (+0xA0), clasificación (+0xA4), ids alternativos (+0x5C) y `$$XTIMAGE`/`$$XSIMAGE` [NV].
  - XEX: media ID.
  - STFS: descripción, editora y la miniatura del título.
- **Fuente embebida:** no incluye `xbox` (`embedded.rs:42-52`) y solo da `Cover` (`:85`, `:116`).
- **Licencia:** son archivos del usuario.

---

## 3. Tareas nuevas propuestas

Están deduplicadas entre fuentes, cada una en una línea y ordenadas por valor.

### 3.1 Fuentes y huecos de arte

1. Ampliar `Media` y `Media::ALL` (`profiles/mod.rs:126-137`) con los huecos del §4, cada uno con su límite de tamaño. `Video` no entra como hueco.
2. Hueco Logo:
   - fuentes, en este orden: `Named_Logos` de libretro, `wheel-hd` > `wheel` de ScreenScraper, `/logos` de SteamGridDB (en el idioma del usuario), IGDB `logos` con `alpha_channel`, y GDK/ERA locales;
   - quitar `Logo` del mapeo de la Microsoft Store.
3. ScreenScraper: que `parse_game` acepte medios sin región (`screenscraper.rs:187-188`) y no descarte región desconocida (`:587`). Sin esto no llegan fanart, steamgrid, video ni marquee.
4. Hueco Fondo, en este orden: heroes de SteamGridDB, fanart de ScreenScraper, artworks de IGDB, fanart de TheGamesDB, hero de Steam, background de x360db, SuperHero de la Store y `background_image` de RAWG.
5. SteamGridDB:
   - portadas ancha y cuadrada, icons, `types=animated` (con límite propio frente a los 16 MB de hoy), `styles`/`oneoftag`, `page`/`thumb`;
   - desempatar por `release_date` y `verified`.
6. Exponer en Ajustes los filtros nsfw, humor y epilepsia de SteamGridDB (`steamgriddb.rs:52-54`).
7. Añadir `serial`, `title_id` y `ext_ids` a `ArtQuery` (`sources/mod.rs:76-102`), con el serial que ya lee `sniff.rs:210`.
8. Identificar por serial y por prefijo STFS al escanear:
   - llevar `dat_by_serial`, `serial_keys` y `by_name_prefix` a Organizar y al relleno;
   - va dentro de la tarea de TODO.md:510 y es lo que hace útiles los DAT de GameTDB.
9. ScreenScraper, búsqueda:
   - `serialnum` cuando no hay hash, `romtype` según el contenedor (iso o dossier) y `gameid` en las repeticiones;
   - mandar md5 y sha1 desde Organizar (`online.rs:39`).
10. ScreenScraper, cuotas:
    - leer `requestskotoday` y `maxrequestskoperday`, esperar unos 5 s y reintentar ante un 429;
    - leer el bloque `serveurs` y `ssinfraInfos`;
    - mostrar las cuotas en Ajustes.
11. ScreenScraper, imágenes: pedirlas con `maxwidth`, `maxheight` y `outputformat` según el perfil, y mandar el CRC de la imagen local para recibir `CRCOK`.
12. Huecos de caja y soporte:
    - ScreenScraper: box-2D-back, box-2D-side, support-texture y support-scan (con número de soporte);
    - GameTDB: cover3D, disc y coverfull;
    - xdb: `cover_back` y `media`;
    - box-3D de ScreenScraper solo si sale en `mediasJeuListe` [NV].
13. Puente de ids con Wikidata: `haswbstatement` (P11072, P5290, P5794, P12561, P1733) y después `wbgetentities`. SPARQL solo en lotes pequeños.
14. Proveedor titledb por title ID, con un índice inverso `id` → entrada: icono, banner, capturas y nombre.
15. Proveedor GameTDB de arte por ID6 o ID4: cover, coverHQ, cover3D y disc. HEAD → 404 para saber si hay arte. Uno a uno, respetando robots.
16. Proveedores de Xbox:
    - xdb por serial o title ID;
    - x360db (banner e icono en `.png`);
    - Microsoft Store por StoreId o `PackageIdentityName`, con `XboxTitleId` solo como alternativa;
    - añadir `xbox` a la fuente embebida, tras confirmar si la imagen es `$$XTIMAGE` o `$$XSIMAGE`.
17. Proveedor IGDB (cuenta de Twitch del usuario): `external_games`, covers, artworks y logos.
18. Proveedor TheGamesDB por hash o uid con `filter[platform]` y `filter[region_id]`.
19. Hueco Marquesina para arcade: `marquee` y `screenmarquee` de ScreenScraper.
20. Huecos Bezel y Overlay de ScreenScraper, eligiendo 4:3 o 16:9 (horizontal, vertical o cóctel) según el perfil.
21. Guardar en el candidato la procedencia (`author`, `language`, `notes`) y los datos del medio (`url`, formato, tamaño y hash).
22. Orden de regiones configurable: la del usuario > `wor` > `us` > `eu` > `jp` > `ss` > `cus` > la primera. `favregion` se traduce aparte.
23. RetroAchievements:
    - proveedor con hash por sistema (tabla o rcheevos) y caché que se invalida por `DateModified`;
    - [NV] sus imágenes `ImageBoxArt`, `ImageTitle` e `ImageIngame` como fuentes de último recurso.
24. Pedir y cachear `mediasJeuListe` e `infosJeuListe` de ScreenScraper e ignorar los tipos desconocidos sin fallar.
25. Puente CRC → serial con `metadat/serial` para cartuchos.
26. Valorar los DAT de TOSEC para los sistemas sin No-Intro (opcional).

### 3 Biblioteca (ficha, colecciones, estado del jugador, integridad)

1. **Modelo de metadatos por juego,** con origen por campo y la edición manual por encima de todo (TODO.md:402 y :416). Ver el §4.2.
2. Leer gratis los metadatos de los DAT de GameTDB que ya se descargan: `developer`, `publisher`, `releaseyear/month/day`, `users` y `esrb_rating`. Va en `parse_game` (`dat.rs:233-239`) y la fecha se marca como poco fiable.
3. Ficha desde ScreenScraper: synopsis por idioma, editeur, developpeur, dates por región, joueurs, note y classifications.
4. Ficha desde `metadat/` de libretro, bajo demanda y uniendo por CRC o serial: genre, developer, publisher, franchise, releaseyear/month, maxusers y esrb/bbfc/elspa/edge/famitsu.
5. Ficha de Switch desde titledb y de Wii/Wii U/3DS/DS desde el XML de GameTDB: descripción, idiomas, jugadores y clasificación.
6. Guardar `ext_ids` por juego (gameid de ScreenScraper, id de SteamGridDB, slug de IGDB, QID de Wikidata, appid de Steam, StoreId, id de RA), con corrección manual.
7. Agrupar versiones y regiones con `cloneof` (de libretro y de ScreenScraper), `XboxCrossGenSetId` y las releases de xdb.
8. Etiquetar variantes con las marcas de ROM de ScreenScraper (`beta`, `demo`, `trad`, `hack`, `unl`, `alt`, `best`), `notgame`, `IsDemo` e `isDemo`.
9. Colecciones inteligentes:
   - por género, modo, tema o estilo (nombres traducidos);
   - saga ordenada por número (`familles`/`numeros`, franchise, collections de IGDB);
   - hardware (analog, rumble, enhancement_hw);
   - «con logros».
10. Estado del jugador: importar tiempo jugado y última partida de los `.lrtl` y `content_history.lpl`, y favoritos de `content_favorites.lpl` (TODO.md:405 y :419).
11. Integridad:
    - estado del volcado frente al DAT;
    - sets de arcade (mame-split, mame-nonmerged, mame-member, fbneo-member, headered);
    - hashes de Wii, Wii U y 3DS desde el XML de GameTDB;
    - `romregions`, `romlangues` y `romserial` de ScreenScraper para comparar con lo leído.
12. Cabeceras de Xbox en la ficha: región, clasificación e ids alternativos del XBE; media ID del XEX (necesario para elegir edición en x360db); descripción y editora del STFS.
13. Galería en la ficha con miniaturas, «ver más» y autor. El logo va sobre el fondo en Detalles (TODO.md:417).
14. Logros de RA: insignia provisional con la marca `retroachievement` de ScreenScraper y después los datos completos de `GetGameExtended`. Va en la subsección de TODO.md:423 y :437.
15. Logo de editora y desarrolladora (`mediaCompagnie`) y icono de cada colección (`mediaGroup`).
16. Manual, flyer de varias páginas y mapas de ScreenScraper bajo demanda desde la ficha. Vídeo como enlace (YouTube de TheGamesDB, IGDB, Store) y descarga de MP4 opcional, apagada y con límite.
17. Edición física o digital: `game_release_formats` y `external_games.media` de IGDB.
18. Pestaña de controles con `tips`, `actions` y `couleurs` de ScreenScraper (prioridad baja).
19. Aparcado: enviar datos con `botNote` y `botProposition`, solo con confirmación expresa del usuario cada vez.

### 6 Exportar a frontends

1. **Fallo:** aplicar `sanitise_libretro` (`identify/names.rs:398`) al `{stem}` del perfil RetroArch (`profiles/template.rs:75`).
2. Escribir los huecos nuevos en cada frontend: el logo en `marquees/` de ES-DE, `marquee` de Batocera y `Named_Logos/`; el fondo, la caja 3D y el disco donde cada frontend los admita.
3. Exportar metadatos (`genre`, `players`, `releasedate`, `developer`, `publisher`, `desc`) a `gamelist.xml`, Pegasus y LaunchBox. Solo campos con licencia que lo permita:
   - **nunca** datos de RAWG;
   - lo de ScreenScraper con la atribución CC BY-NC-SA junto al archivo exportado.
4. Exportar `.lpl`: `path` (con `#` en zips), `label`, `crc32` como `CRC|crc` o `serial|serial`, `db_name` y `base_content_directory`.
5. Pasar `rotation` y `resolution` al frontend para arcade vertical.
6. Arte de sistema: medios de sistema de ScreenScraper, `Platforms?fields=icon` y `Platforms/Images` de TheGamesDB, y `platform_logos` de IGDB.
7. Avisar de que el actualizador de RetroArch puede pisar el arte [NV].

### 7 Créditos

1. ScreenScraper: crédito «ScreenScraper y sus fuentes» que cubra textos, vídeos y manuales (`credits-inventory.md:36`).
2. libretro-database: añadir «y metadatos», con CC BY-SA (`:34`), y el enlace a GameTDB (`:55`), obligatorio si sus datos salen en la ficha.
3. SteamGridDB: crédito por autor de cada imagen.
4. Filas nuevas: RAWG (enlace activo, «No data redistribution», «No cloning»), Steam y Valve (sin sugerir afiliación, aviso de datos guardados y política de privacidad) y Commons.
5. Fila de IGDB: reflejar la contradicción entre la introducción y la FAQ.
6. Filas nuevas: xdb (con MobyGames como origen), x360db, Microsoft Store, RA, rcheevos y `blawar/nut`.
7. Verificar los términos de Redump y No-Intro, hoy [NV] en `:50-51`.

### 2.1 Variantes, DLC y trucos

- Hacks y traducciones (`metadat/hacks`, `libretro-dats`) como variantes del juego base.
- Trucos `.cht` de libretro.
- Comprobar sets multidisco con `romnumsupport` y `romtotalsupport` de ScreenScraper.
- Versiones y DLC con `versions` y `cnmts` de titledb, `RelatedProducts` de la Store y `products` de x360db.
- Demos con `IsDemo`.

---

## 4. Modelo de datos unificado

### 4.1 Huecos de arte (ampliación del enum `Media`)

| Hueco | Fuentes por orden de prioridad | Notas |
|---|---|---|
| `Cover` (ya existe) | Embebida o local > libretro > ScreenScraper box-2D > GameTDB coverHQ > SteamGridDB vertical > TheGamesDB > IGDB > xdb, x360db y Store > Steam capsule | |
| `CoverWide` | SteamGridDB 460×215 y 920×430 > Steam header > ScreenScraper steamgrid > TheGamesDB banner > titledb bannerUrl | |
| `CoverSquare` | SteamGridDB 512 y 1024 > Store FeaturePromotionalSquare > titledb iconUrl | |
| `Background` | SteamGridDB heroes > ScreenScraper fanart > IGDB artworks > TheGamesDB fanart > Steam hero > x360db > Store SuperHero > RAWG | Datos de RAWG solo para ver, nunca para exportar |
| `Logo` | libretro Named_Logos > ScreenScraper wheel-hd o wheel > SteamGridDB logos (por idioma) > IGDB logos con alpha > TheGamesDB clearlogo [NV] > GDK/ERA | |
| `Marquee` | ScreenScraper marquee o screenmarquee | Arcade |
| `Icon` | Local STFS/GDK/ERA > x360db > titledb > SteamGridDB icons > RA ImageIcon > Commons P8972 [NV] | |
| `Screenshot` (ya existe) | libretro Snaps > ScreenScraper ss > TheGamesDB > IGDB > titledb > Steam, Store, x360db > RAWG | |
| `Title` (ya existe) | libretro Titles > ScreenScraper sstitle > TheGamesDB titlescreen [NV] > $$XTIMAGE y xtimage [NV] | |
| `BoxBack` | ScreenScraper box-2D-back > TheGamesDB boxart trasera > xdb cover_back > GameTDB coverfull | |
| `BoxSpine` | ScreenScraper box-2D-side | Opcional |
| `Box3D` | GameTDB cover3D > ScreenScraper box-3D [NV] | |
| `Physical` (cartucho o disco) | GameTDB disc > ScreenScraper support-texture o support-scan > xdb media | Con número de soporte |
| `Bezel` | ScreenScraper bezel-4-3 o bezel-16-9 (variante según el perfil) | |
| Adjuntos bajo demanda, fuera de `Media::ALL`: `Video`, `Manual`, `Map`, `Flyer` | Vídeo: enlaces (TheGamesDB youtube, IGDB, Steam, Store) > MP4 de ScreenScraper o TheGamesDB, opcional y con límite. Manual, mapas y flyer: ScreenScraper | La Store no permite descargar vídeo; las animadas de SteamGridDB tienen su propio límite |

### 4.2 Campos de metadatos

Cada campo guarda su valor, su fuente y la fecha. La edición manual tiene prioridad siempre.

| Campo | Fuentes por orden de prioridad |
|---|---|
| `name` y `alt_names[lang]` | DAT No-Intro o Redump > ScreenScraper noms (orden de regiones) > titledb (por idioma) > Wikidata (etiquetas) > IGDB alternative_names > TheGamesDB alternates > cabecera local |
| `description[lang]` | ScreenScraper synopsis (idioma de la interfaz > en) > titledb > XML de GameTDB > IGDB > TheGamesDB > Store > STFS > RAWG (solo para ver) |
| `genres[]` | ScreenScraper > libretro `metadat/genre` > GameTDB > IGDB > TheGamesDB > Wikidata > RAWG |
| `developers[]` y `publishers[]` | DAT de GameTDB (ya descargado) > libretro `metadat/developer` y `publisher` > ScreenScraper > titledb > IGDB > TheGamesDB > Wikidata > STFS (editora) |
| `release_dates[region]` | ScreenScraper dates > titledb (por región) > GameTDB > IGDB release_dates > Wikidata P577+P291 > libretro releaseyear/month (poco fiable) > TheGamesDB y RAWG (una sola fecha) |
| `players` y `coop` | ScreenScraper joueurs > libretro users o maxusers > titledb numberOfPlayers > GameTDB > TheGamesDB > IGDB multiplayer_modes > Store Attributes |
| `score` | ScreenScraper note > IGDB rating > RAWG o Metacritic (solo para ver) > edge/famitsu |
| `age_ratings[org]` | GameTDB (ESRB, PEGI, CERO, GRB) > ScreenScraper classifications > IGDB > titledb > libretro esrb/bbfc/elspa > XBE +0xA4 > TheGamesDB |
| `series` y `series_no` | ScreenScraper familles y numeros > libretro franchise > IGDB franchises o collections > Wikidata P179 > RAWG |
| `languages[]` y `regions[]` | ScreenScraper romlangues y romregions > titledb > GameTDB > IGDB language_supports > XBE +0xA0 |
| `tags[]` (hardware, modo, tema) | libretro analog, rumble, enhancement_hw > ScreenScraper modes, themes, styles > IGDB themes |
| `variant_flags` | ScreenScraper marcas de ROM > DAT (hacks, homebrew) > IsDemo de la Store o titledb > ScreenScraper notgame |
| `parent` y `clones` | DAT `cloneof` > ScreenScraper cloneof y romcloneof > XboxCrossGenSetId > xdb releases |
| `ids` (serial, title_id, media_id, ext_ids) | Cabecera local o sniff > DAT o `metadat/serial` > Wikidata (puente) > IGDB external_games > ScreenScraper gameid > SteamGridDB id > Steam appid > Store StoreId > RA GameID |
| `time_to_beat` | IGDB game_time_to_beats [NV] > RAWG playtime (solo para ver) |
| `achievements` | RA (número de logros, puntos, `GuideURL`) > marca `retroachievement` de ScreenScraper (provisional) |
| `play_state` (tiempo, última partida, favorito) | `.lrtl` y `.lpl` de RetroArch > el propio Coverly |
| `verification` | DAT (CRC y SHA1) > XML de GameTDB > hashes de RA > `status` del DAT [NV] |

**Reglas para todos los campos:**
- Primero las fuentes locales, gratis y sin clave. Después las que buscan por id exacto (hash, serial, title ID). Al final, la búsqueda por nombre.
- Un campo de RAWG nunca se exporta.
- Un campo de ScreenScraper que se exporte lleva la atribución CC BY-NC-SA.
- Nada de datos ni imágenes de terceros dentro del repo ni del instalador.

**Pendientes de verificar:**
- Los tipos de ScreenScraper que no están en la tabla oficial: box-3D, support-2D, mixrbv*, wheel-carbon y wheel-steel.
- `platformId` de eshop en SteamGridDB.
- El cupo mensual de TheGamesDB.
- `movies` de RAWG con la clave Free.
- Los términos de SteamGridDB, Redump, No-Intro y RA.
- `appdetails` de Steam y `IStoreBrowseService/GetItems`.
- Si las imágenes de libretro traen ETag.
- En qué menús de RetroArch se ven los `Named_Logos`.
- Si la imagen de xdb sale de `$$XTIMAGE` o de `$$XSIMAGE`.
- Que la licencia de Wikidata sea CC0.

**Archivos de referencia:**
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/TODO.md
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/profiles/mod.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/sources/screenscraper.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/sources/steamgriddb.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/sources/libretro.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/identify/dat.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/profiles/template.rs
- /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/docs/02-research/credits-inventory.md
