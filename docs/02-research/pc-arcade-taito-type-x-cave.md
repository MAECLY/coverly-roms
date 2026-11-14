# Arcade de PC: Taito Type X y Cave (CV1000 y versiones de PC)

Fecha: 2026-11-14. Investigación para el punto 2.2 de `TODO.md`. Revisé cada dato contra la fuente primaria cuando la hay: `es_systems.xml` y `USERGUIDE.md` de ES-DE (`master` en `ddc2653b`, 2026-11-07), `es_systems.yml` y los `*.emulator.yml` de Batocera (`a24e141c`, 2026-11-08), `es_systems.yml` de Knulli (`aef09a01`), `es_systems.cfg` de RetroBat (`0724b2b7`, 2026-11-13) y su `emulatorlauncher` (`9046216b`), el driver `cave/cv1k.cpp` y `cave/cavepc.cpp` de MAME (`35b719ed`), `d_cv1k.cpp` de FBNeo (libretro, `b92082ed`), los DAT de `libretro-database` (`5cfb6cd3`), el servidor en vivo `thumbnails.libretro.com` y el mapa de sistemas de ScreenScraper que publica Skyscraper (`screenscraper_platforms.json`, `de90920d`). Lo que solo viene de foros o wikis comunitarios va marcado **[comunidad]**; lo que deduzco yo, **[inferencia]**.

Regla de este documento: se nombran cargadores y emuladores como hechos, pero no se enlazan ni se recomiendan, y no hay ningún enlace a sitios de descarga. Coverly no abre, no hashea ni sube nada de un juego de Type X: solo lee nombres de carpetas y archivos.

## Resumen

- **Cave CV1000 (`cave3rd`) ya funciona hoy** en `mame`/`fbneo`. Los 32 sets del driver de FBNeo y los de MAME están en los dos DAT que usa Coverly, con portada, captura y pantalla de título en `MAME/` y en `FBNeo - Arcade Games/` de libretro-thumbnails (comprobado con tres juegos, 200 en los nueve casos). Lo único que falta es que la carpeta `cave3rd/` de Batocera y Knulli cuente como «ya ordenada»: hoy no resuelve a ningún sistema, así que Organizar propondría mover sus sets a `fbneo/` **[inferencia, por la regla del recorrido]**.
- **Las versiones de PC de Cave (Steam) no son arcade**: son juegos de Windows normales. No van ni en `mame`/`fbneo` ni en Type X. Coverly no tiene sistema `windows`; quedan fuera de 2.2.
- **Taito Type X / X⁺ / X² / X³ / X⁴ y NESiCAxLive** son PCs con Windows Embedded. Un juego volcado es **una carpeta** con su ejecutable (`game.exe` en la mayoría) y datos sueltos. No hay DAT, ni hashes, ni identificador interno legible sin abrir el ejecutable; **el título sale del nombre de la carpeta**. Solo ES-DE tiene una carpeta propia (`type-x`, y también `pcarcade`); RetroBat los mete en `teknoparrot` junto con otras placas de PC; Batocera y Knulli no tienen ninguna.
- **En una Anbernic no se juegan.** Type X necesita un PC x86 con Windows (o Wine/Proton en x86) y un cargador. CV1000 en ARM solo es posible con el núcleo FBNeo o MAME, y su rendimiento en chips tipo H700 no está comprobado.
- **Recomendación:** opción A (sistema `typex` como «juego = carpeta» reconocido solo por su carpeta, más un sistema `cave3rd` hecho como `model2`). Detalle en §6.

## 1. Taito Type X y familia

### 1.1 Qué es cada placa

| Placa | Año | SO | Notas |
|---|---|---|---|
| Type X / X⁺ | 2004 | Windows XP Embedded | X⁺ con mejor gráfica. Primer juego: Raiden III. |
| Type X² (y X² Satellite) | 2007 | Windows XP Embedded SP2 | LGA 775, audio Realtek. Sus juegos no corren en la Type X original ni al revés. |
| Type X⁷ | — | — | Sobre todo pachinko en Japón. |
| Type X Zero | 2010 | Windows Embedded Standard 7 | Intel Atom + ION. |
| Type X³ | 2012 | Windows Embedded Standard 7 (64 bits) | Core i3/i5/i7 de 2.ª generación. |
| Type X⁴ | 2016 | Windows Embedded 8 / Windows 10 IoT **[comunidad]** | Varias variantes; poca documentación pública. |
| NESiCAxLive / NESiCAxLive 2 | 2011 | — | No es una placa: es la distribución digital de Taito para recreativas. Empezó sobre Type X² y siguió en placas posteriores. |

Fuentes: [Wikipedia, Taito Type X](https://en.wikipedia.org/wiki/Taito_Type_X), Emulation General Wiki (sin enlace: enlaza cargadores), [System16](https://system16.com/hardware.php?id=677).

### 1.2 Cómo se distribuyen y cómo es un volcado

- **En la máquina original** el juego venía en un disco duro (IDE en Type X/X², SATA en X³) y, en Type X/X², con una **llave USB de seguridad**. Según varios usuarios, la llave guarda la clave con la que el lanzador de Taito descifra un contenedor cifrado del disco **[comunidad]** (solo fragmentos de búsqueda, sin abrir las páginas: [«Inside Taito Type X, part 1»](https://arcadefever.blogspot.com/2009/05/inside-taito-type-x-part-1.html) del blog Arcade Fever e hilos del foro arcade-projects; la wiki de Arcade Otaku no se pudo leer). Coverly no tiene que saber nada de esto.
- **Lo que circula** es la carpeta del juego ya extraída: un directorio por juego con el ejecutable, DLL y datos. Para jugarla en un PC hace falta un cargador que emule la E/S JVS y la protección: **TeknoParrot** y **JConfig** son los que cita la Emulation General Wiki. Son programas para Windows x86.
- **Nombre del ejecutable.** La tabla de ejecutables por juego del generador de TeknoParrot de RetroBat (`emulatorLauncher/Generators/TeknoParrot.generator.cs`, 137 entradas) lo muestra: **50 de 137 son `game.exe`/`Game.exe`**, y 18 son carpetas `«Título» for NesicaxLive\game.exe`. Ejemplos: `Battle Gear 4\game.exe`, `Chase H.Q. 2\game.exe`, `Raiden IV (type x)\game.exe`, `Homura (Type X)\Game.exe`, `King of Fighters XIII Climax for NesicaxLive\game.exe`. El resto son otras placas de PC (Sega RingEdge/Lindbergh, Raw Thrills, Namco ES3…) con nombres propios.
- **Conclusión para Coverly:** `game.exe` es la norma pero no prueba nada (cualquier juego de PC puede llamarse así, y `exe` ya está en `EVERYDAY_EXTS`: suelto no es evidencia). La carpeta **es** el juego, y **su nombre es el título**, a veces con una marca de placa (`(Type X)`, `(type x)`, `for NesicaxLive`).

### 1.3 Identificar el juego sin abrir nada

| Señal | ¿Se puede usar? | Notas |
|---|---|---|
| Nombre de la carpeta del juego | **Sí, es la fuente principal** | Limpiar marcas de placa: `(Type X)`, `(Type X2)`, `(type x)`, `for NesicaxLive` → etiqueta, no título. |
| Carpeta padre `type-x/`, `typex/`, `teknoparrot/`, `pcarcade/` | Sí, como sistema | Es la única prueba fuerte de que el contenido es Type X. `teknoparrot` y `pcarcade` también llevan otras placas. |
| ES-DE «directorios interpretados como archivos» | Sí, solo el nombre | En ES-DE se renombra la carpeta igual que el archivo que se lanza: `Raiden IV.exe/` con `Raiden IV.exe` (o un `.bat`/`.lnk` en Windows) dentro. Es un juego, no una carpeta para recorrer ([USERGUIDE, «Directories interpreted as files»](https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md#directories-interpreted-as-files)). |
| RetroBat: carpeta con el nombre del perfil de TeknoParrot | Sí, solo el nombre | El generador busca `GameProfiles/<nombre de la carpeta>.xml` y avisa «make sure the game folder is named like the GameProfile file»; admite las extensiones `.teknoparrot`, `.parrot`, `.game` en la carpeta. El nombre es un id corto (`BBCF`, `KingofFightersXIII`), no un título. |
| `UserProfiles/*.xml` de TeknoParrot del usuario | Posible, opcional | XML en claro que escribe el cargador, con `GameName` y `GamePath` (campos de `GameProfile.cs` en emulatorlauncher). Sería leer un archivo de configuración del usuario, no del juego. No es necesario para la primera versión. |
| Hash del `.exe` o de los datos | **No** | No hay DAT de ningún tipo, los ejecutables suelen estar modificados, y abrirlos no aporta un título. |
| Contenido de INI, `.dat`, contenedores | **No** | No se abre nada dentro de la carpeta del juego. |

**Archivos de protección: solo por nombre, nunca se leen.** Cualquier archivo con pinta de llave, licencia o volcado de dongle dentro de una carpeta de Type X se trata como `is_key_material` (se reconoce por nombre o extensión, no se abre, no se hashea, no se mueve aparte del juego). La lista actual (`consoles/mod.rs`) no tiene nombres propios de Type X, y no he encontrado una fuente primaria fiable con nombres concretos, así que **no propongo añadir ninguno**: la regla segura es no abrir ningún archivo de una carpeta de Type X, que es más estricto que una lista.

### 1.4 Cómo lo representan los frontends

| Frontend | Sistema / carpeta | Extensiones | Notas |
|---|---|---|---|
| ES-DE | `type-x` («Taito Type X») | Linux: `.AppImage .desktop .exe .sh`; Windows: `.bat .lnk` | `<platform>arcade</platform>` para el scraper. Lanza con Wine/Proton/UMU en Linux, con un acceso directo o script en Windows y macOS. Fuente: `resources/systems/{linux,windows}/es_systems.xml`. |
| ES-DE | `pcarcade` («PC Arcade Systems») | Linux: `… .exe .lindbergh .sh`; Windows: `.bat .lindbergh .lnk` | Carpeta genérica de arcade de PC (también Lindbergh). |
| RetroBat | `teknoparrot` | `.teknoparrot .parrot .game .7z .zip .rar` | Un juego = una carpeta con el nombre del perfil. Mezcla Type X con RingEdge, Lindbergh, Raw Thrills, Namco… |
| Batocera | — | — | No hay `type-x` ni `teknoparrot`. Solo el sistema genérico `windows` (Wine, x86). |
| Knulli | — | — | Igual que Batocera: `windows` existe pero depende de `BR2_PACKAGE_WINE_TKG`. |
| LaunchBox | Plataforma «Taito Type X» | — | Existe en la base de datos de LaunchBox (plataforma 169). Los juegos se añaden a mano como juegos de PC. |
| muOS, Onion, GarlicOS, MinUI/NextUI, Anbernic stock | — | — | No tienen carpeta. |

## 2. Cave: CV1000 frente a las versiones de PC

### 2.1 CV1000 (`cave3rd`) son ROMs de verdad

- **MAME** (`src/mame/cave/cv1k.cpp`): Mushihime-Sama, Ibara, Ibara Kuro Black Label, Espgaluda II, Puzzle! Mushihime-Tama, Pink Sweets, Mushihime-Sama Futari (1.0, 1.5, Black Label), Muchi Muchi Pork!, Deathsmiles, Deathsmiles MegaBlack Label, DoDonPachi Dai-Fukkatsu (1.0, 1.5, Black Label) y Medal Mahjong Moukari Bancho. Akai Katana y DoDonPachi SaiDaiOuJou están comentados (no son jugables en MAME).
- **FBNeo** (`src/burn/drv/cave/d_cv1k.cpp`): 32 sets, incluidos `akatana`, `ddpsdoj`, `sdojak` y `matsuri15`, que MAME no tiene activos.
- **DAT de libretro**: `mushisam`, `futari15`, `ddpdfk` y `espgal2` están en `MAME.dat` y en `FBNeo - Arcade Games.dat`; `akatana` y `ddpsdoj` solo en FBNeo. Coverly ya los identifica por nombre corto, y con `DEFAULT_ARCADE = ["fbneo", "mame"]` un set suelto va a `fbneo`.
- **Portadas**: `MAME/` y `FBNeo - Arcade Games/` de libretro-thumbnails tienen `Named_Boxarts`, `Named_Snaps` y `Named_Titles` para «Mushihime-Sama (2004_10_12.MASTER VER.)», «DoDonPachi Dai-Fukkatsu Ver 1.5 (2008_06_23 MASTER VER 1.5)» y «Deathsmiles (2007_10_09 MASTER VER)» (los nueve devuelven 200). El nombre es el del DAT con `/` cambiado a `_`, que es la regla que ya aplica Coverly.
- **Frontends**: Batocera tiene el sistema `cave3rd` («Cave CV1000») con MAME y con Demul bajo Wine; su `sega-arcade.mame.emulator.yml` le quita `bin` a las extensiones arcade (quedan `zip`/`7z`). El comentario del sistema pide sets «compatible with MAME 0.284 or later» y avisa de que hace falta un equipo x86_64. Knulli copia el sistema, pero su único emulador ahí es Demul, condicionado a `BR2_PACKAGE_DEMUL` (un programa de Windows x86), así que en sus dispositivos ARM no aparece **[inferencia]**. ES-DE y RetroBat no tienen carpeta: van en `mame`/`fbneo`/`arcade`.
- **ScreenScraper** tiene «Cave» como sistema 47 (un grupo por fabricante dentro de arcade), pero Coverly ya busca estos juegos por `mame` (75).

**¿Funciona ya `cave3rd` con lo que hay?** Sí en identificación, nombre y arte. Falta una cosa: en un perfil de Batocera, `roms/cave3rd/` no resuelve a ningún sistema, así que el recorrido de Organizar la trata como un montón y propondría llevar sus sets a `fbneo/` **[inferencia por `organize/walk.rs`: solo se saltan las carpetas que resuelven]**. La decisión D16 del lote de sistemas 1 dijo que «ningún frontend tiene carpeta para Cave»; Batocera sí la tiene, así que esa premisa no se cumple.

### 2.2 Las versiones de PC de Cave

- Las de Steam (Mushihimesama, DoDonPachi Resurrection, Akai Katana Shin, Espgaluda II, Deathsmiles…) son **juegos de Windows normales**: instalador o carpeta de Steam, sin relación de archivos con los sets de CV1000. Van en un sistema `windows`, que Coverly no tiene, y su arte sale de SteamGridDB (por id de Steam o por nombre).
- **Deathsmiles II** es otra cosa: corrió en un PC propio de Cave (Athlon 64 X2 + tarjeta CompactFlash). MAME lo tiene como `deathsm2` en `cave/cavepc.cpp`, marcado `MACHINE_NOT_WORKING`, y está en `MAME.dat`. Ya cae en `mame` como cualquier set.
- Las «for NesicaxLive» de juegos de Cave (p. ej. Akai Katana Shin) son **Type X**, no CV1000.

## 3. Fuentes de portadas

| Fuente | Type X | CV1000 | Versiones de PC |
|---|---|---|---|
| libretro-thumbnails | **No hay carpeta** (la raíz en vivo no tiene nada con «Taito», «Type X» ni «Cave»; solo `Cave Story/`, que es otro juego) | `MAME/` y `FBNeo - Arcade Games/`, completas | No |
| ScreenScraper | Sistema **112** «Type X» (nom_launchbox «Taito Type X»; noms_commun incluye X⁺, X⁷, X², X² Satellite, X Zero, X³). También **269** «TeknoParrot». Corrige la nota del lote 1 («no ScreenScraper id found»). | 75 (MAME) o 47 (Cave) | 138 «PC Windows» |
| SteamGridDB | Búsqueda por nombre (tiene juegos que no son de Steam) | Innecesario | Por id de Steam o nombre |
| LaunchBox Games DB | Plataforma «Taito Type X» (id 169) | — | — |

Las fuentes de ScreenScraper y SteamGridDB ya están en Coverly (detrás de la clave del usuario). Para Type X la búsqueda tendría que ser **por título** (el de la carpeta), porque no hay hash ni id.

## 4. Perfiles de dispositivo y frontend

Columnas de `SYSTEM_FOLDERS` en `profiles/builtin.rs`: `[esde, batocera, muos, onion, minui]`.

| Sistema | ES-DE | Batocera/Knulli | muOS | Onion | MinUI | Anbernic stock | RetroArch |
|---|---|---|---|---|---|---|---|
| `typex` | `type-x` | — | — | — | — | — | — (sin núcleo) |
| `cave3rd` | — (va a `mame`/`fbneo`) | `cave3rd` | — | — | — | — | `MAME` |

RetroBat no es un perfil de Coverly hoy. Si se añade, su carpeta sería `teknoparrot` y la regla de nombre de carpeta = id de perfil choca con la de ES-DE (título legible): habría que decidir cuál manda.

**¿Se juegan en una Anbernic? No para Type X.** Ninguna build ARM de ES-DE, Batocera, Knulli, muOS, Onion o MinUI trae un cargador de Type X, y los cargadores son programas de Windows x86. Para CV1000, los núcleos FBNeo y MAME sí existen en ARM, pero el hardware es pesado (SH-3 más el blitter `epic12`) y no tengo una medida de rendimiento en un H700 o similar: **sin verificar**. En la Ayaneo (x86, Windows) sí funcionan los dos.

## 5. Lo que hace hoy Coverly con esto

- Un set CV1000 suelto: se identifica por nombre corto y va a `fbneo` (o `mame` según el perfil). Correcto.
- Una carpeta `cave3rd/` en una tarjeta de Batocera: no resuelve, así que se recorre como un montón **[inferencia]**.
- Una carpeta de Type X dentro de un montón: `game.exe` es extensión cotidiana (sin evidencia), el resto son DLL y datos, así que todo queda «desconocido» y en su sitio, igual que `Cave Story/` en D15. No hay riesgo de que se desperdigue, pero tampoco se reconoce.
- Una carpeta `type-x/` en un árbol de ES-DE: no resuelve a nada, se recorre y todo queda «desconocido» **[inferencia]**.

## 6. Opciones para decidir con Miguel

### Opción A (recomendada): `typex` propio como «juego = carpeta» + `cave3rd` como `model2`

- **Type X**: sistema `typex` («Taito Type X», `family = "arcade"`, alias `TYPEX`, `type-x`, `typex`; `screenscraper = 112`; sin `libretro`, sin DAT, sin extensiones fuertes). Solo se reconoce **por su carpeta**: cada subcarpeta directa de `type-x/` es un juego, y también una carpeta `Título.exe/` (o `.bat`/`.lnk`) de ES-DE. No hay regla de contenido (no se abre nada). Título = nombre de la carpeta, con las marcas de placa pasadas a etiqueta.
- **Cave CV1000**: sistema `cave3rd` («Cave CV1000», alias `CAVE3RD`, `cave3rd`; `libretro = "MAME"`; `dats = ["metadat/mame/MAME.dat"]`, colocado **después** de `mame` como `model2`/`model3`, para que `MAME.dat` siga dando `mame`). Un set suelto sigue yendo a `fbneo`/`mame`; dentro de `cave3rd/` se queda.
- **Ventajas:** Batocera conserva su carpeta; ES-DE ve sus juegos de Type X; el resto de perfiles no cambia. Sigue el patrón ya probado de D15 y D16.
- **Contras:** un sistema nuevo sin hash (todo depende del nombre de la carpeta), y un sistema `cave3rd` que solo existe en dos frontends.

### Opción B: un sistema `pcarcade` genérico (Type X, NESiCAxLive, Lindbergh, RingEdge…)

- Una carpeta-juego por título en `pcarcade/` (ES-DE) o `teknoparrot/` (RetroBat), con la placa como etiqueta si el nombre la trae. `cave3rd` igual que en A.
- **Ventajas:** encaja con cómo lo usa la gente (un solo cargador para todo) y con RetroBat.
- **Contras:** mezcla placas; ScreenScraper 112 deja de valer como sistema único (habría que usar 269 «TeknoParrot» o buscar por nombre), y el `type-x` de ES-DE quedaría como alias de algo más amplio. Los nombres de RetroBat son ids de perfil, no títulos.

### Opción C: no tocar Type X; solo `cave3rd`

- Las carpetas de Type X siguen quedando «desconocidas» y en su sitio. Solo se añade `cave3rd` (o un alias con ámbito para Batocera) para que Organizar no vacíe esa carpeta.
- **Ventajas:** lo mínimo; cero riesgo de identificar mal un juego de PC.
- **Contras:** no responde al pedido de 2.2; un usuario de ES-DE con `type-x/` no ve nada.

**Recomendación: A.** Es la que menos inventa: Type X se reconoce solo donde el usuario ya lo ha puesto (su carpeta) y nunca por contenido; `cave3rd` arregla un caso real de Batocera sin tocar la identificación.

## 7. Esbozo de la implementación (tarea 3, si se elige A)

1. **`systems.toml`**: entradas `typex` y `cave3rd` como arriba. `typex` con un campo nuevo o la regla de D15 para «cada hijo directo es un juego» (hoy D15 solo deja en su sitio una carpeta con un archivo probado; aquí la prueba es la carpeta del sistema). Si no hay campo, una constante `FOLDER_PER_GAME_SYSTEMS = ["typex"]` junto a `FOLDER_GAME_SYSTEMS`.
2. **Recorrido y escaneo**: dentro de una carpeta que resuelve a `typex`, cada subcarpeta es un elemento (no se baja a sus archivos); una carpeta `X.exe/` con `X.exe` dentro, o `X.bat`/`X.lnk`, también. Fuera de `typex/` no se reconoce nada (sin heurística de `game.exe`). Dedupe: tratar `typex` como `ports` (no comparar archivos dentro).
3. **Nombres**: limpiar `(Type X)`, `(Type X2)`, `(Type X3)`, `(type x)` y `for NesicaxLive` (sin distinguir mayúsculas) y pasarlos a una etiqueta de placa; quitar la extensión de una carpeta `.exe`/`.bat`/`.lnk`/`.parrot`/`.teknoparrot`. Nada de renombrar la carpeta en disco sin plan y confirmación (los cargadores y RetroBat dependen del nombre).
4. **Arte**: sin libretro. ScreenScraper con sistema 112 y búsqueda por nombre; SteamGridDB por nombre. `cave3rd` usa `MAME` en libretro, como `model2`.
5. **Perfiles**: `SYSTEM_FOLDERS` con `("typex", [Some("type-x"), None, None, None, None])` y `("cave3rd", [None, Some("cave3rd"), None, None, None])`. El resto, `Absent`.
6. **Seguridad**: ningún archivo dentro de una carpeta `typex` se abre, se hashea ni se sube; `is_key_material` sigue aplicándose por nombre. Comprobarlo con un test que use una `MetaProbe` que falle si se lee cualquier byte.
7. **Datos de prueba sintéticos**: `type-x/Raiden IV (Type X)/game.exe` (archivo vacío) + `data/` vacía → un juego «Raiden IV» con etiqueta Type X; `type-x/Chaos Code.exe/Chaos Code.exe` (estilo ES-DE); `type-x/Akai Katana Shin for NesicaxLive/Game.exe` → etiqueta NESiCAxLive; un `game.exe` suelto en un montón → desconocido; `roms/cave3rd/mushisam.zip` (cero bytes, identificado por nombre) → se queda en `cave3rd/` con el perfil de Batocera; el mismo set suelto → `fbneo`. Ningún binario real.
8. **Tests de sistemas** como en el lote 1: `MAME.dat` sigue dando `mame` con `cave3rd` presente; ningún alias nuevo es una palabra cotidiana (`typex`, `cave3rd` son distintivos: globales, no `scoped_aliases`).

## Fuentes

- ES-DE: [`es_systems.xml` de Linux](https://gitlab.com/es-de/emulationstation-de/-/blob/master/resources/systems/linux/es_systems.xml), [`es_systems.xml` de Windows](https://gitlab.com/es-de/emulationstation-de/-/blob/master/resources/systems/windows/es_systems.xml), [USERGUIDE](https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md)
- Batocera: [`es_systems.yml`](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/emulationstation/batocera-es-system/es_systems.yml), [`sega-arcade.mame.emulator.yml`](https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/emulators/mame/sega-arcade.mame.emulator.yml)
- Knulli: [`es_systems.yml`](https://github.com/knulli-cfw/knulli-linux/blob/knulli-main/package/emulationstation/knulli-es-system/es_systems.yml)
- RetroBat: [`es_systems.cfg`](https://github.com/RetroBat-Official/retrobat/blob/main/system/templates/emulationstation/es_systems.cfg), [generador de TeknoParrot en emulatorlauncher](https://github.com/RetroBat-Official/emulatorlauncher/blob/master/emulatorLauncher/Generators/TeknoParrot.generator.cs), [`GameProfile.cs`](https://github.com/RetroBat-Official/emulatorlauncher/blob/master/emulatorLauncher/Teknoparrot/GameProfile.cs)
- MAME: [`cave/cv1k.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/cave/cv1k.cpp), [`cave/cavepc.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/cave/cavepc.cpp)
- FBNeo: [`cave/d_cv1k.cpp`](https://github.com/libretro/FBNeo/blob/master/src/burn/drv/cave/d_cv1k.cpp)
- libretro: [`MAME.dat`](https://github.com/libretro/libretro-database/blob/master/metadat/mame/MAME.dat), [`FBNeo - Arcade Games.dat`](https://github.com/libretro/libretro-database/blob/master/metadat/fbneo-split/FBNeo%20-%20Arcade%20Games.dat), [thumbnails.libretro.com](https://thumbnails.libretro.com/)
- ScreenScraper (vía Skyscraper): [`screenscraper_platforms.json`](https://github.com/Gemba/skyscraper/blob/master/screenscraper_platforms.json)
- LaunchBox: [plataforma Taito Type X](https://gamesdb.launchbox-app.com/platforms/details/169-taito-type-x)
- Hardware: [Wikipedia](https://en.wikipedia.org/wiki/Taito_Type_X), [Arcade Fever, «Inside Taito Type X, part 1»](https://arcadefever.blogspot.com/2009/05/inside-taito-type-x-part-1.html) **[comunidad]**, [System16](https://system16.com/hardware.php?id=677)
