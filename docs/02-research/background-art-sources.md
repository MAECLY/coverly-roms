# Arte de fondo (background/hero) para PS4, PS5, Xbox One y Series

Etiquetas: **[OK]** comprobado en la fuente primaria · **[NV]** no verificado · **[NUEVO]** fuente que no estaba en el repo

## Lo esencial

- **No hay ninguna fuente con licencia abierta para estas imágenes.** El arte siempre tiene copyright de la editora. Lo que cambia de una fuente a otra son los términos de acceso. Por eso se mantiene el modelo actual: se descarga bajo demanda, con la clave del propio usuario, se guarda en su equipo y nunca se empaqueta.
- **El arte incrustado de los archivos del usuario es la fuente mejor y más legal.** Para Xbox, el hero oficial (SuperHeroArt/TitledHeroArt) no viene dentro del paquete. Solo está en el catálogo de la Store. **[OK]**
- **El puente para buscar en otras fuentes es Wikidata (CC0)**, a partir del title ID o del nombre:

  | Propiedad | Qué es | Destino |
  |---|---|---|
  | P12561 | ID de SteamGridDB | SteamGridDB |
  | P5885 | ID de producto de Microsoft Store | Microsoft Store |
  | P1733 | ID de aplicación de Steam | Steam |
  | P5794 | Slug de IGDB, no su ID numérico | IGDB |
  | P5944 / P5971 | PS Store NA / EU | PS Store |

  **[OK]**

  Ítems con P12561, contando por P400 (las cifras cambian unas unidades de un día a otro):

  | Plataforma | Con P12561 | Total |
  |---|---|---|
  | PS4 | 4.997 | 6.818 |
  | PS5 | 2.948 | 3.973 |
  | Xbox One | 3.376 | 4.719 |
  | Xbox Series | 3.259 | 4.361 |

  Ítems con P5885: 3.146 de Xbox One y 2.300 de Series.

  Ítems con P1733:

  | Plataforma | Con P1733 |
  |---|---|
  | PS4 | 5.055 |
  | PS5 | 3.098 |
  | Xbox One | 3.454 |
  | Xbox Series | 3.389 |

  Hay juegos muy conocidos sin P12561, por ejemplo Bloodborne, Halo 5 y Demon's Souls (2020). En esos casos hay que buscar por nombre como respaldo.

## PS4

| # | Fuente | Qué da | Cómo se consulta | Licencia / términos | Cobertura |
|---|---|---|---|---|---|
| 1 | `sce_sys/pic1.png` (carpeta del juego) | Fondo. El lanzador de shadPS4 lo usa como fondo (`game_list_frame.cpp:235`) **[OK]**. Tamaño 1920×1080 **[NV]** | Leer el archivo. Variantes `pic1_00…30.png` y `pic1.dds` **[OK]** | Archivo del usuario | Todos los juegos volcados |
| 2 | Carpeta de parche `CUSA…-patch/sce_sys/pic1.png` | Puede traer un `pic1` más reciente. shadPS4 la consulta (`game_info.h:85`) **[OK]** | Leer el archivo, con prioridad sobre el juego base | Archivo del usuario | Si existe la carpeta |
| 3 | PKG sin cifrar: entradas `0x1006` (PIC1), `0x1241…0x125F` (PIC1_NN), `0x1220` (PIC0), `0x12C0` (PIC1_DDS) | Lo mismo que el punto 1 | Solo si `flags1 & ENCRYPTED == 0`, comprobación que ya existe en `orbis.rs:123` y `orbis.rs:176` **[OK]** | Archivo del usuario | PKG sin cifrar |
| 4 | `appmeta` (`/user/appmeta/<TitleID>/pic1.png`, sacado por FTP) | Fondo | Solo como carpeta local de origen | Archivo del usuario | **[NV]**: psdevwiki está bloqueado |
| 5 | SteamGridDB, heroes | 1920×620, 3840×1240 o 1600×650, es decir 3:1. Hay que recortarlo a 16:9 **[OK]** | `/heroes/game/{id}` usando P12561 y, si no hay, búsqueda por nombre | Clave del usuario. Términos **[NV]**: la web es una SPA y no se pudo leer el texto | Unos 4.997 juegos con P12561. Heroes por juego **[NV]** |
| 6 | TheGamesDB, `fanart` | Fondo 16:9 | Plataforma 4919, búsqueda por nombre | GPL-3.0 solo para el código. No hay términos para las imágenes **[OK]**. ES-DE lo usa como fanart **[OK]** | **[NV]** |
| 7 | Steam CDN, `library_hero.jpg` | 3840×1240 (3:1) **[OK]** | `shared.akamai.steamstatic.com/store_item_assets/steam/apps/{P1733}/library_hero.jpg`, sin clave | Términos de Steam **[NV]**. RomM usa Steam como proveedor | Solo juegos que también están en PC (5.055 con P1733) |
| 8 | IGDB, `artworks` | `t_1080p`: tamaño máximo 1920×1080, no exacto **[OK]** | `where slug = "<P5794>"` | *"free for non-commercial usage"* con el Twitch Developer Service Agreement. Credenciales Twitch del usuario. Las imágenes borradas en IGDB duran 30 días, lo que afecta al diseño de la caché **[OK]** | **[NV]** |
| 9 | ScreenScraper, `fanart` | Fondo | Sistema 60 (ya integrado) | La web dice *"Creative Commons BY-NC-SA 4.0"*, pero el arte original sigue siendo de la editora **[OK]** | 7.354 juegos de PS4 en el sistema. Fanart **[NV]** |
| 10 | RAWG, `background_image` | Fondo **[NV]**: el nombre del campo no se ha comprobado | API con clave del usuario | Obliga a citar a RAWG con un *"active hyperlink from every page"*. *"We do not claim ownership of any of the images"* **[OK]** | **[NV]** |

## PS5

| # | Fuente | Qué da | Cómo se consulta | Licencia / términos | Cobertura |
|---|---|---|---|---|---|
| 1 | `sce_sys/pic0.png` | Fondo. La guía de PS-Mac-Tools dice: *"Change game/app background -> Replace … pic0.png"* **[OK]**. Tamaño 3840×2160 **[NV]**: medido en una sola muestra | Leer el archivo | Archivo del usuario | Juegos volcados en carpeta. PKG de tienda, `.ffpkg` y exFAT quedan fuera |
| 2 | `sce_sys/pic1.png` | Respaldo de 1920×1080 **[NV]**: su papel exacto no está claro | Leer el archivo | Archivo del usuario | Igual que el punto 1 |
| 3 | `appmeta` (FTP) | Igual que en PS4 | Carpeta local | Archivo del usuario | **[NV]** |
| 4 | SteamGridDB, heroes | 3:1 | P12561 y, si no hay, nombre | Igual que en PS4 | Unos 2.948 juegos con P12561 |
| 5 | TheGamesDB, `fanart` | 16:9 | Plataforma **4980** **[OK]** | Igual que en PS4. ES-DE solo mapea PS4 | **[NV]** |
| 6 | Steam, `library_hero` | 3:1 | P1733 | **[NV]** | 3.098 con P1733 |
| 7 | IGDB, artworks | Hasta 1080p | Slug | Igual que en PS4 | **[NV]** |
| 8 | ScreenScraper, `fanart` | Fondo | Sistema 284 **[OK]** | CC BY-NC-SA, según la propia web | **[NV]** |
| 9 | RAWG | Fondo | Nombre | Igual que en PS4 | **[NV]** |

## Xbox One y Xbox Series

| # | Fuente | Qué da | Cómo se consulta | Licencia / términos | Cobertura |
|---|---|---|---|---|---|
| 1 | GDK `MicrosoftGame.config` → `ShellVisuals/@SplashScreenImage`, o antes `Executable/@OverrideSplashScreenImage` | Splash de 1920×1080, obligatorio en consola **[OK]** | Leer el manifiesto y el PNG del paquete sin cifrar | Archivo del usuario | Los juegos de consola lo traen. En juegos solo de PC es opcional desde el GDK de febrero de 2021, y los DLC no lo traen **[OK]** |
| 2 | ERA `AppxManifest.xml` → `SplashScreen/@Image` | Splash de 1920×1080. Solo se ha visto en un ejemplo de Microsoft (ATG Samples), no en un juego comercial **[OK]** | Leer el manifiesto | Archivo del usuario | **[NV]** que sea obligatorio |
| 3 | SteamGridDB, heroes | 3:1 | P12561 y, si no hay, nombre | Igual que en PS4 | One unos 3.376, Series unos 3.259 |
| 4 | TheGamesDB, `fanart` | 16:9 | One **4920**, Series X **4981** **[OK]** | Igual que en PS4 | Forza Horizon 5 aparece **[OK]**. El resto **[NV]** |
| 5 | **[NUEVO]** Microsoft Store `displaycatalog` | `SuperHeroArt` (hasta 3840×2160) y `TitledHeroArt` (1920×1080) **[OK]**. Es el arte del dashboard | `GET displaycatalog.mp.microsoft.com/v7.0/products?bigIds={P5885}&market=US&languages=en-us`, o `/v7.0/products/lookup?alternateId=XboxTitleId&value=<decimal>`. Sin clave, `?w=` para el tamaño **[OK]** | **No es abierta.** Términos de uso de Microsoft: *"No logo, graphic … or image … may be copied or retransmitted unless expressly permitted"*. Las Game Content Usage Rules solo cubren los juegos de Microsoft Studios **[OK]**. Es una API sin documentación pública | One 3.146 y Series 2.300 con P5885. Algunos productos secundarios no tienen `XboxTitleId` |
| 6 | Steam, `library_hero` | 3:1 | P1733 | **[NV]** | One 3.454, Series 3.389 |
| 7 | IGDB, artworks | Hasta 1080p | Slug | Igual que en PS4 | **[NV]** |
| 8 | ScreenScraper, `fanart` | Fondo | One: sistema 34. Series: **[NV]** que exista sistema | CC BY-NC-SA, según la propia web | **[NV]** |
| 9 | RAWG | Fondo | Nombre | Igual que en PS4 | **[NV]** |

## Orden recomendado en Coverly

1. **Lo incrustado primero (`embedded_first`):**
   - PS4: el parche, después `pic1`, después `pic1_NN`, después `pic0`. En PKG sin cifrar: `0x1006`, después `0x1241…`, después `0x1220`. Después `appmeta`.
   - PS5: `pic0`, después `pic1`.
   - GDK: `OverrideSplashScreenImage`, después `SplashScreenImage`.
   - ERA: `SplashScreen`.
2. **TheGamesDB `fanart`:** ya es 16:9 y ES-DE lo usa.
3. **SteamGridDB heroes:** primero por P12561 y luego por nombre. Es el proveedor ya integrado, pero hay que recortarlo de 3:1 a 16:9.
4. **ScreenScraper `fanart`:** ya integrado. Requiere cuenta.
5. **Steam `library_hero`:** por P1733, solo para juegos multiplataforma.
6. **IGDB artworks:** opcional, con las credenciales Twitch del usuario.
7. **Microsoft Store `displaycatalog` (solo Xbox):** desactivado por defecto. El usuario lo activa, con un aviso de copyright y caché solo local. Preferir `SuperHeroArt` y, si no hay, `TitledHeroArt`.
8. **RAWG:** último recurso, solo si se puede poner la cita con enlace en la vista.

## Descartadas, y por qué

- **PS Store** (API GraphQL, roles `BACKGROUND_LAYER_ART`/`GAMEHUB_COVER_ART` de 3840×2160): sus términos prohíben *"construct a database with it"* (§3) y el uso de *"robot … scraper"* (§10) **[OK]**. Como mucho, una acción manual del usuario para un solo juego.
- **`m.np.playstation.com/api/catalog`** y **Xbox TitleHub**: piden el token de la cuenta del usuario y sus términos no son abiertos. No aportan nada frente a las fuentes anteriores.
- **TMDB2 de PS4** (`tmdb.np.dl.playstation.net`): necesita una clave HMAC sacada del firmware, lo que supone saltarse un control técnico.
- **Asociadas a piratería u homebrew de fPKG:** ORBISPatches y similares se excluyen. PS-Mac-Tools, PS5X360 y PS4PKGTool solo sirven como referencia del formato, nunca como fuente de datos.
- **Sin interés:**
  - fanart.tv: no tiene sección de juegos.
  - MobyGames: es de pago.
  - Wikimedia Commons: no aloja key art con copyright.
  - libretro-thumbnails: no tiene fondos.
- **LaunchBox Games DB** (`Metadata.zip`, unos 109 MB, tipo `Fanart - Background`): aplazada. No hay términos publicados (`/terms` da 404) y obliga a descargarlo todo de golpe.
- **XVC/XVD/MSIXVC:** el arte va cifrado y no se puede leer.

## Tareas para el punto 7.1

1. Añadir `Media::Background` (serde `background`) en `crates/coverly-core/src/profiles/mod.rs:126` y en `Media::ALL`. Revisar `MediaArg` en `coverly-cli/src/audit.rs:50`.
2. Darle a ese hueco su propio límite, por ejemplo 3840 px de lado y un tope de bytes, separado de `MAX_SIDE = 1024` (`sources/embedded.rs:37`).
3. Leer el fondo incrustado en el probe de `embedded.rs` para PS4, PS5, GDK y ERA, en el orden de arriba.
   - Sustituir el test `embedded_ps4_and_ps5_backgrounds_are_not_covers` (`tests/fill_sources.rs:665`) por tests que comprueben que el fondo va al hueco nuevo.
   - Comprobar que el crate `image` decodifica los `.dds` **[NV]**.
   - Detectar el formato por el contenido, no por la extensión.
4. Actualizar la D21 (`docs/superpowers/specs/2026-10-26-systems-batch-3.md:91`): el hueco ya existe, y `pic0` de PS5 mide 3840×2160 **[NV]**, no 1920×1080.
5. Pedir el fondo a los proveedores ya integrados: SteamGridDB heroes (con recorte a 16:9) y ScreenScraper fanart.
6. Proveedores nuevos:
   - TheGamesDB fanart, con los IDs 4919, 4980, 4920 y 4981.
   - Steam `library_hero` vía P1733.
   - Más adelante, IGDB.
7. Puente de Wikidata: añadir las consultas por P5885, P1733 y P5794 a las que ya hay por P12561.
8. Microsoft `displaycatalog`, opcional y apagado por defecto, con aviso de copyright y caché local.
9. Escribir el fondo en cada frontend: carpeta `fanart` en ES-DE, `background` en Pegasus, `Fanart - Background` en LaunchBox **[NV]**: los nombres de destino no se han comprobado.
10. Vistas:
    - Hero de fondo en el panel o inspector de Detalles y en Galería, con velado para que el texto se lea.
    - Columna «Fondo» en la vista Huecos de arte.
    - Filtro «sin fondo».
11. Verificar antes de dar algo por hecho:
    - Los términos de SteamGridDB, en un navegador.
    - La cobertura real de fanart en TheGamesDB y ScreenScraper con una muestra de unos 20 juegos.
    - El sistema de Xbox Series en ScreenScraper.
    - El tamaño de `pic1` en PS4.

