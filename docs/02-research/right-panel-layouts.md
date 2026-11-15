# Panel derecho (ficha del juego): inventario, investigación y diseños

Investigación para el bloque 9 de `TODO.md` (pedido el 2026-11-15). Solo investigación y maquetas: no cambia código de la app. Se volverá sobre esto después de los bloques 3 y 3.1, que traen la mayor parte de los datos nuevos.

Fuentes de este documento:

- El panel actual medido en el mock del navegador (`pnpm dev`, `?mockPick=addons|extras|pcarcade`) a 1440×900 y 900×700, y su código (`src/components/library/DetailDrawer.tsx`, `src/components/library/game/*`, `ArtWorkspace.tsx`, `.drawer*` en `src/styles/globals.css`).
- Una revisión con las skills `impeccable` (modo Operate, refinamiento) y `frontend-design`. El detector de `impeccable` no encontró nada en `DetailDrawer.tsx`, `ArtWorkspace.tsx` ni `game/` (0 avisos).
- Documentación oficial, wikis y guías de diseño de otras apps (enlaces en cada punto). No se enlaza ningún sitio de emuladores, ROMs ni claves.

Las maquetas (HTML estático con los tokens de la app, claro y oscuro) están en [`right-panel-layouts/mocks.html`](right-panel-layouts/mocks.html) y las capturas del panel actual en [`right-panel-layouts/`](right-panel-layouts/). Se abren en el navegador sin servidor; sin conexión, las fuentes caen a las del sistema.

> **Decisión de Miguel (2026-11-15):** se implementan **los ocho diseños** (A actual, B pestañas horizontales, C rail vertical, D resumen fijo + secciones plegables, E panel ancho de dos columnas redimensionable, F ficha completa, G densidad compacta, H secciones a medida + barra de comandos) y el usuario elige cuál usar. Se mantiene el plan: un registro único de secciones alimenta todos los diseños, y todo se revisa después de los bloques 3 y 3.1 porque cambian los datos. Las tareas por diseño están en el bloque 9 de `TODO.md`; no se empieza sin la orden de Miguel.

## 1. El panel actual

### Qué es

`DetailDrawer` es un `<aside class="drawer">` de ancho fijo (`22rem`, 352 px) con un solo scroll vertical. Se abre con Enter o con doble clic; con el Inspector (⌘I) queda fijo y sigue a la selección. No se puede redimensionar ni plegar por partes, salvo el bloque «Mods, trucos, extras y partidas», que es un `<details>`.

Ya existe un mecanismo para abrir el panel en una sección concreta: cada sección lleva `data-panel-section`, y `usePanelSection` hace scroll hasta ella y le da el foco (por ejemplo, al pulsar el resumen de complementos de una portada se abre en Versión). Es la base natural para pestañas, saltos o un rail.

### Bloques, en orden

| # | Bloque | Componente | Cuándo aparece | Alto medido |
| --- | --- | --- | --- | --- |
| 1 | Cabecera: nombre, «Ver en el Finder», cerrar | `DetailDrawer` | Siempre | 28 px |
| 2 | Línea de región / ID de título | `RegionLine` | Siempre (vacía si no hay datos) | 16 px |
| 3 | Versión: la más nueva, lista de actualizaciones, juego base, aviso «falta la última» | `VersionSection` | Con actualizaciones o datos de versión | 189 px |
| 4 | Revisiones: cuál es la predeterminada, «Usar esta» | `RevisionsSection` | Con más de una revisión | 172 px |
| 5 | DLC: cada uno con su arte, «Buscar arte», «Leer el arte incluido» | `DlcSection` | Con DLC | 156 px |
| 6 | Demo relacionada (o juego completo de una demo) | `DemoSection` | Con demo | 98 px |
| 7 | Mods, trucos, extras y partidas (plegable, cuatro casillas con su número) | `FixedFoldersSection` | Siempre | 19 px plegado / 209 px abierto |
| 8 | Extras: etiquetas, categorías (Banda sonora, Arte, Vídeos, Temas, Guías…), añadir, «No encontrados» | `ExtrasSection` | Con extras abiertos desde 7 | 1.067 px con 11 extras |
| 9 | Taller de arte: huecos (portada, captura, pantalla de título), «Desde archivo…», buscar, candidatas | `ArtWorkspace` | Siempre | 617–632 px |
| 10 | Identidad: método, confianza, «Identificar», «Olvidar la manual» | `IdentityBlock` | Siempre | 93 px sin identificar |
| 11 | Archivo: ruta, sistema, extensión, tamaño, fecha, regiones | `DetailDrawer` | Siempre | 162–187 px |
| 12 | Reproductor (cuando suena un extra) | `MiniPlayer variant="panel"` | Con audio en marcha | fijo abajo |

### Longitud medida (alto total del scroll del panel)

| Juego de prueba | Ventana | Alto del panel | Pantallas de scroll |
| --- | --- | --- | --- |
| Lantern Harbor (PS4: 2 actualizaciones, DLC, demo) | 1440×900 | 1.572 px (1.762 con las carpetas abiertas) | 1,9–2,2 |
| Tidewalker (PS3, 11 extras abiertos) | 1440×900 | 2.314 px | 2,9 |
| Tidewalker con el reproductor | 900×700 | 2.355 px | 3,9 |
| Star Courier (SNES, 2 revisiones) | 1440×900 | 1.227 px | 1,5 |
| Raiden IV (Taito Type X, carpeta) | 1440×900 | 1.000 px | 1,2 |
| mushisam (Cave CV1000) | 1440×900 | 1.000 px | 1,2 |

A 900×700 el panel mantiene sus 352 px y la cuadrícula se queda en una sola columna.

Capturas en [`right-panel-layouts/`](right-panel-layouts/): `current-1440-modern-top`, `-folders`, `-mid`, `-bottom`, `current-1440-extras`, `current-900-extras-player`, `current-1440-retro` y `current-1440-typex` (`.webp`).

### Qué se ve en la revisión

Lo que funciona:

- La gramática de huecos (relleno = hay arte, discontinuo = falta) se respeta en los huecos, en las miniaturas de DLC y en la cuadrícula.
- Las secciones aparecen solo si tienen datos: un juego retro o de Type X tiene un panel corto.
- Las listas usan la mono para nombres de archivo y versiones, y la sans para el resto; los avisos llevan texto además de color.
- Teclado y foco: el panel fijo no roba el foco a la vista, Escape no lo cierra, y el salto a sección mueve el foco al título.

Lo que cuesta:

1. **No hay resumen arriba.** Al abrir un juego moderno lo primero es la lista de versiones; la portada y el estado del arte aparecen después de Versión, DLC, Demo y las carpetas (a 1440×900, por debajo del primer pliegue). El título de la cabecera es lo único que identifica el juego.
2. **El taller de arte divide el panel.** Son 617 px de buscador y candidatas siempre abiertos, en medio de datos que se leen. En un juego sin complementos es casi todo el panel; en uno con complementos queda enterrado.
3. **Los extras abiertos triplican el panel** (de 1.000 a 2.300 px) y empujan Identidad y Archivo fuera de alcance.
4. **Identidad y Archivo están al final**, y son lo que más se consulta para saber «qué archivo es este».
5. **Cada bloque futuro (3, 3.1, 4, 5, 6) alarga la misma columna**: ficha con sinopsis, galería, logros, partidas, estado por dispositivo y por frontend.
6. Detalles: en Type X la extensión sale como «.» y el tamaño como «0 B» (es una carpeta); la casilla «Partidas» explica un bloque futuro dentro de la ficha.

Persona de referencia: quien recorre una biblioteca grande con ↑/↓ y el Inspector fijo, para ver de un vistazo si cada juego tiene arte, la última actualización y sus extras. Hoy tiene que hacer scroll en cada juego para llegar al estado del arte.

## 2. Cómo lo resuelven otros

Cada punto dice qué patrón usa y qué sirve para Coverly. «(Secundaria)» marca lo que solo se pudo comprobar en fuentes de terceros o en fragmentos de búsqueda; lo que no se pudo comprobar (Kodi, Info de Fotos, inspector de OpenEmu, Big Box, la barra de Lutris, las pestañas de los temas de Playnite) no se usa.

### Gestores de juegos y frontends

- **Steam (página del juego en la biblioteca).** Arte de cabecera con el logo encima, una barra de jugar y enlaces justo debajo, y luego resumen de la última sesión, amigos y actividad; lo menos importante, al final. Para Coverly: una franja de estado y acciones bajo el resumen. [Steam Library update](https://store.steampowered.com/libraryupdate?l=english)
- **Playnite (modo escritorio).** En la vista Detalles el panel de detalles está siempre a la derecha; en la vista Cuadrícula se abre a demanda desde la portada. Es justo el comportamiento del Inspector (⌘I) frente a abrir con Enter. [Manual de Playnite](https://api.playnite.link/docs/manual/gettingStarted/playniteDesktopMode.html)
- **Temas de Playnite (Stardust, Mythic).** Las secciones extra (capturas, vídeos, estadísticas, logros, duración) llegan con complementos y solo aparecen si tienen datos. Para Coverly: secciones que aparecen al tener datos, como ya hace el panel. [Stardust](https://github.com/darklinkpower/Stardust)
- **LaunchBox (panel Game Details).** Ctrl+T muestra u oculta el panel entero; en Opciones › Visuals › Game Details se elige qué secciones y campos aparecen (medios, notas, metadatos, botones, valoraciones, insignias, logros, documentos, juegos relacionados). Reordenar exige un tema propio, y los usuarios lo piden. Para Coverly: una lista de mostrar/ocultar cubre casi todo; reordenar puede ir después. [Personalizar Game Details](https://launchbox.featurebase.app/help/articles/7963348-customizing-launchbox-desktop-themes-layouts-game-details-and-image-types), [Vistas de detalles](https://launchbox.featurebase.app/help/articles/3164650-game-details-and-platform-details-views)
- **GOG Galaxy 2.0.** Vista del juego con pestañas; el progreso (logros) tiene pestaña propia, junto a la lista de DLC. (Secundaria) [Historial de cambios](https://mac.softpedia.com/progChangelog/GOG-Galaxy-Changelog-140696.html)
- **RetroArch (listas).** Tres tipos de miniatura (caja, pantalla de título, captura); la mayoría de interfaces muestran dos a la vez, configurables por sistema o por lista, y un visor a pantalla completa recorre todas. Para Coverly: en el resumen, dos o tres huecos; el resto detrás de un visor (ya existe la vista rápida con Espacio). [Docs de libretro](https://docs.libretro.com/guides/roms-playlists-thumbnails/)
- **ES-DE.** Desde la 2.0 no hay vistas fijas; los temas declaran variantes y pueden cambiar solas cuando un juego no tiene medios (`noVideos`, `noMedia`). Para Coverly: si un juego no tiene datos de una sección, el diseño se adelgaza en vez de enseñar bloques vacíos. [THEMES.md](https://gitlab.com/es-de/emulationstation-de/-/raw/master/THEMES.md)
- **Pegasus.** «Sin vistas ni diseños predefinidos»: el tema es QML a pantalla completa y la app solo da los datos. Para Coverly: separar los datos (registro de secciones) de cómo se pintan, para poder tener varios diseños. [Temas de Pegasus](https://pegasus-frontend.org/docs/themes/overview/)
- **Heroic.** Página del juego con descripción, editora, tamaño y tiempo jugado; los ajustes avanzados van aparte. [Heroic](https://heroicgameslauncher.com/)
- **Lutris.** Al elegir un juego aparecen sus acciones a la derecha, «Jugar» primero; las últimas versiones añaden barras laterales plegables. (Secundaria) [Linux Magazine](https://www.linux-magazine.com/Online/Features/Lutris)
- **Daijishō.** «Vista detallada» y pulsación larga para más detalles: el detalle completo como gesto secundario. [Repositorio](https://github.com/magneticchen/Daijishou)

### Gestores de medios y archivos

- **Plex.** Ficha con filas cortas; lo que no cabe (reparto y equipo) va a una página propia desde una tarjeta «Ver todo» al final de la fila. Para Coverly: extras y galería en fila corta con «Ver todo». (Secundaria) [Plex](https://support.plex.tv/articles/202462186-viewing-item-details/)
- **Jellyfin.** Los extras van en una fila «Special Features»; los usuarios se quejan de que un reparto largo los empuja fuera de la vista y los reordenan con CSS. Para Coverly: ordenar por valor y no dejar que una lista larga entierre lo demás (lo que hoy pasa con Extras y el taller de arte). (Secundaria) [Foro 1](https://forum.jellyfin.org/t-special-features-page), [Foro 2](https://forum.jellyfin.org/t-css-solution-for-cast-order-after-special-features?pid=61828)
- **Lightroom Classic (panel derecho).** Paneles apilados y plegables; el modo «solo» deja uno abierto y cierra los demás; clic derecho en una cabecera para mostrar u ocultar paneles; Mayús+Tab oculta todo. Es el modelo más cercano a un panel largo y apilado como el de Coverly. (Secundaria) [Adobe](https://helpx.adobe.com/lightroom-classic/help/workspace-basics.html)
- **Get Info del Finder.** Secciones plegables con triángulo (General, Más información, Nombre y extensión, Comentarios) y la ventana recuerda cuáles están abiertas. Para Coverly: recordar el plegado de cada sección. (Secundaria) [Apple](https://support.apple.com/guide/mac-help/mchlp2304), [Eclectic Light](https://eclecticlight.co/2026/06/09/reading-the-finders-get-info-dialog/)
- **Panel de detalles del Explorador de Windows.** Propiedades del archivo elegido; Alt+Mayús+P lo muestra u oculta. (Secundaria) [Eleven Forum](https://www.elevenforum.com/t/show-or-hide-details-pane-in-file-explorer-in-windows-11.1774/)
- **Calibre (detalles del libro).** Preferencias › Interfaz › Aspecto › Detalles del libro elige qué se ve; Alt+Mayús+D lo oculta; doble clic lo abre en una ventana aparte; soltar una imagen pone la portada. Para Coverly: elegir campos, atajo para ocultar, ventana aparte y soltar arte sobre un hueco. [Manual de Calibre](https://manual.calibre-ebook.com/gui.html)

### Patrones de interfaz

- **Paneles en las HIG de Apple.** Un inspector muestra lo seleccionado y se actualiza solo; una ventana de Información mantiene su contenido aunque cambie la selección. Para Coverly: el panel sigue a la selección (como hoy con ⌘I) y una ficha aparte (F) puede dejar un juego fijo. [HIG: Panels](https://developer.apple.com/design/human-interface-guidelines/panels)
- **Vistas divididas en las HIG de Apple.** Dejar ocultar paneles, ofrecer varias formas de mostrarlos (botón, menú, atajo), tamaños mínimo y máximo razonables y separadores finos. [HIG: Split views](https://developer.apple.com/design/human-interface-guidelines/split-views)
- **Inspectores de Xcode.** Una fila horizontal de iconos en lo alto del área del inspector (Archivo, Ayuda rápida, Identidad, Atributos…), con botones que cambian según lo seleccionado. Es la versión horizontal de la opción C. [Ayuda de Xcode](https://help.apple.com/xcode/mac/current/en.lproj/dev49de0094f.html)
- **Barra lateral derecha de Figma.** Pestañas Diseño y Prototipo; las secciones de propiedades aparecen según lo seleccionado. [Ayuda de Figma](https://help.figma.com/hc/en-us/articles/360039832014)
- **Vistas de VS Code.** Secciones que se arrastran para reordenar, menú «…» para mostrar u ocultar, vistas que se mueven entre barras laterales, y la paleta de comandos llega a todo. Para Coverly: H (reordenar, ocultar) y una barra de comandos con «Ir a Arte». [Interfaz de VS Code](https://code.visualstudio.com/docs/getstarted/userinterface)
- **NN/g, pestañas.** Solo cuando no hace falta ver dos pestañas a la vez; pocas pestañas; la primera es la que más se mira, así que lo crítico no puede quedar en otra. Para Coverly: identidad y estado fuera de las pestañas, en el resumen fijo. [Tabs, Used Right](https://www.nngroup.com/articles/tabs-used-right/)
- **NN/g, acordeones en escritorio.** Bien si se necesitan pocas secciones e independientes; mal si se necesita casi todo. [Accordions on Desktop](https://www.nngroup.com/articles/accordions-on-desktop/)
- **NN/g, revelación progresiva.** Lo frecuente primero; más de dos niveles pierde a la gente. Para Coverly: nada de plegable dentro de pestaña dentro de plegable. [Progressive Disclosure](https://www.nngroup.com/articles/progressive-disclosure/)
- **NN/g, cabeceras fijas.** Pequeñas, opacas, con buen contraste, sin animación y solo con lo que se usa a menudo. Para Coverly: un resumen fijo compacto. [Sticky Headers](https://www.nngroup.com/articles/sticky-headers/)
- **Scrollspy.** Marca el enlace de la sección visible con IntersectionObserver. Es la barra de saltos de D. [Bootstrap Scrollspy](https://getbootstrap.com/docs/5.3/components/scrollspy/)
- **Controles segmentados en las HIG de Apple.** Unos 5–7 segmentos como mucho; texto o imagen, no ambos; en macOS, para cambiar de vista en el área principal, una vista de pestañas. [HIG: Segmented controls](https://developer.apple.com/design/human-interface-guidelines/segmented-controls)
- **Patrón Tabs del WAI-ARIA APG.** Flechas entre pestañas (con vuelta), Inicio/Fin opcionales, activación automática solo si los paneles ya están cargados, y `aria-orientation="vertical"` con ↑/↓ para pestañas verticales. [APG Tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/)
- **Patrón Accordion del WAI-ARIA APG.** Un botón dentro de un título, con `aria-expanded` y `aria-controls`; Enter o Espacio pliegan; Tab pasa de cabecera en cabecera. [APG Accordion](https://www.w3.org/WAI/ARIA/apg/patterns/accordion/)
- **react-resizable-panels.** Paneles con `collapsible`, `minSize` y `maxSize`, distribución que se guarda, y un separador con `role="separator"` usable con teclado. Es una opción para E. [Repositorio](https://github.com/bvaughn/react-resizable-panels)
- **Lista y detalle de Microsoft.** Lado a lado desde 641 epx; por debajo, el detalle pasa a una página aparte. Para Coverly: con la ventana estrecha, abrir la ficha completa (F) en vez de un panel de 352 px. [List/details](https://learn.microsoft.com/en-us/windows/apps/design/controls/list-details)
- **Drawer de Fluent 2.** El drawer en línea va junto al contenido cuando se necesitan los dos; el superpuesto es modal y con cuidado. Para Coverly: panel en línea (como hoy); superposiciones solo para editores temporales. [Fluent 2 Drawer](https://fluent2.microsoft.design/components/web/react/core/drawer/usage)
- **Side sheets de Material 3.** Estándar (junto al contenido) o modal (sobre un velo). (Secundaria) [M3 Side sheets](https://m3.material.io/components/side-sheets)
- **Densidad (Material, Gmail).** Predeterminada, cómoda y compacta; la compacta para vistas con muchos datos. Es la opción G. (Secundaria) [Material density](https://m2.material.io/blog/material-density-web), [Gmail](https://gmail.googleblog.com/2011/11/changing-information-density-in-gmails.html)

### Qué se saca en limpio

1. **Resumen fijo arriba** con lo que identifica el juego y su estado (Steam, NN/g cabeceras fijas). Lo crítico nunca dentro de una pestaña (NN/g pestañas).
2. **Secciones plegables que se recuerdan**, con modo «solo una abierta» y un menú para mostrar u ocultar (Lightroom, Finder, LaunchBox, VS Code), más una barra de saltos que marca la sección visible (scrollspy).
3. **Pestañas solo para trabajos separados** que no se miran a la vez (contenido, arte, extras, archivo), unas cinco como mucho en horizontal; si hay más, en vertical o con iconos (HIG, Xcode, VS Code).
4. **Sin datos, sin sección** (Figma, ES-DE, temas de Playnite).
5. **Datos separados de cómo se pintan** (Pegasus, ES-DE): es lo que permite ofrecer varios diseños.
6. **Panel que se puede ocultar, ensanchar y sacar a una ventana o página** (HIG, Calibre, lista y detalle de Microsoft, react-resizable-panels).

## 3. Diseños propuestos

Las ocho opciones usan el mismo juego de prueba en las maquetas. Todas comparten una idea: un **registro único de secciones** (id, título, resumen de una línea, número o aviso, si tiene datos ahora). El diseño solo decide cómo se agrupan y se muestran; cambiar de diseño no cambia qué hay. Ese registro es lo que hace barato soportar varios diseños, que es lo que pidió Miguel.

Reparto de secciones que usan B, C, E y F:

| Sección | Hoy | Llega después |
| --- | --- | --- |
| Resumen (cabecera fija) | Título, sistema, región, ID de título, versión en uso, estado de los huecos de arte | Año, desarrolladora, jugadores (3); logros (3); «en 2 dispositivos» (4); aviso de cuenta que falta (8) |
| Contenido | Versión, Revisiones, DLC, Demo, Mods, Trucos | Integridad frente al DAT, variante (beta, hack…), ids externos (3) |
| Arte | Portada, captura, pantalla de título; buscar y candidatas | Fondo, logo, icono, caja 3D, ancha, cuadrada, galería con autor (3, 3.1) |
| Extras | Categorías, etiquetas, añadir, reproductor | Manual, flyer y mapas bajo demanda (3) |
| Partidas | La casilla «vacía» | Copias con fecha y dispositivo, partidas de la demo (5) |
| Dispositivos y exportación | — | Estado por dispositivo, qué se lleva, estado en cada frontend (4, 6) |
| Archivo e identidad | Ruta, tamaño, extensión, fecha, región; identificar y confianza | Fuente de cada campo y edición manual (3, §4.2 de `source-capabilities.md`) |

Una sección sin datos se muestra apagada con su bloque («Partidas · bloque 5») o se oculta, según el ajuste; nunca deja un hueco vacío sin explicar.

### A. Actual (referencia)

```
┌ Lantern Harbor (USA)      [▢][×] ┐
│ CUSA31415                        │
│ Versión ............ 2 act.      │
│  v1.10  …(v1.10).pkg  [La más nueva]
│  v1.05  …(v1.05).pkg       1,1 GB│
│ DLC ............ 1 en DLC/       │
│ Demo relacionada                 │
│ ▸ Mods, trucos, extras y partidas│
│ [Portada][ Captura ][ Título ]   │
│ ┌ Taller de arte (617 px) ┐      │
│ Identidad · Archivo              │
│ ▶ reproductor                    │
└──────────────────────────────────┘
```

- **Cómo absorbe 3–6:** añadiendo bloques a la columna. Con la ficha del bloque 3 y la galería superaría las 4 pantallas.
- **A favor:** nada escondido; orden lineal para lector de pantalla; ya existe.
- **En contra:** ver «Qué cuesta» arriba.
- **Teclado y accesibilidad:** Tab recorre todo; sin atajos para saltar de sección más allá del salto desde la cuadrícula.
- **Esfuerzo:** ninguno. Se conserva como diseño «Lista» si se ofrecen varios.

### B. Pestañas horizontales

```
┌ Lantern Harbor (USA)      [▢][×] ┐
│ ┌────┐ PS4 · EE. UU. · CUSA31415 │
│ │port│ v1.10 [la más nueva]      │
│ └────┘ 2 act · 1 DLC · demo · 11 │
│ ■ portada □ captura □ título     │
├ Contenido 4 │ Arte 1/3 │ Extras 11 │ Partidas │ Archivo ┤
│ (solo la pestaña elegida)        │
└──────────────────────────────────┘
```

- **Reparto:** resumen fijo; pestañas Contenido, Arte, Extras, Partidas, Archivo (y Dispositivos cuando exista).
- **Cómo absorbe 3–6:** cada bloque es una pestaña o entra en una existente. El límite es el ancho: en 352 px caben 4–5 etiquetas; en la maqueta, «Archivo» ya queda cortada. Con 7 hay que desplazar la fila o añadir «Más».
- **A favor:** cada pestaña cabe en una pantalla; patrón muy conocido (GOG Galaxy, Plex, Jellyfin).
- **En contra:** esconde; hay que recordar dónde está cada cosa; el salto desde la cuadrícula tiene que cambiar de pestaña.
- **Teclado y accesibilidad:** patrón Tabs del WAI-ARIA APG: ←/→ entre pestañas con foco itinerante, Inicio/Fin, Tab al panel; cada pestaña con su número en el nombre accesible («Extras, 11»).
- **Esfuerzo:** medio. Registro de secciones + componente de pestañas (ya hay `tablist` en la barra lateral de vistas).

### C. Rail vertical de pestañas

```
┌ Lantern Harbor (USA)      [▢][×] ┐
│ Arte  1 de 3 huecos          │ i │
│ [port][capt][tít]            │ ≡4│
│ [fondo][logo][icono] (3.1)   │ ▣2│ ← aviso: faltan 2
│ ┌ Captura: buscar ───────┐   │ ♫ │ 11
│ │ candidatas…            │   │ ──│
│ └────────────────────────┘   │ P │ (Partidas, bloque 5, apagado)
│                              │ D │ (Dispositivos, bloque 4)
│                              │ L │ (Logros, bloque 3)
│                              │ F │ (Archivo)
└──────────────────────────────────┘
```

- **Reparto:** como B, pero con iconos en una columna de 46 px en el borde exterior.
- **Cómo absorbe 3–6:** es el que mejor escala: 8–10 secciones sin recortar, y los avisos (falta la última actualización, conflicto de partida, «distinta versión en la Anbernic») se ven aunque la sección esté cerrada.
- **A favor:** no gasta alto; estados visibles; patrón de la barra de actividades de VS Code (Xcode usa la misma idea en una fila horizontal de iconos).
- **En contra:** iconos solos (necesitan tooltip y nombre); 46 px menos de contenido o un panel más ancho; menos descubrible que palabras.
- **Teclado y accesibilidad:** `role="tablist"` con `aria-orientation="vertical"`, ↑/↓, Inicio/Fin; tooltips con el nombre y el número; los avisos también en el nombre accesible, no solo en el color de la insignia.
- **Esfuerzo:** medio (igual que B, más los iconos de cada sección).

### D. Resumen fijo + secciones plegables (punto de partida recomendado)

```
┌ Lantern Harbor (USA)   [⚙][▢][×] ┐
│ ┌────┐ PS4 · EE. UU.             │
│ │port│ v1.10 [la más nueva]      │
│ └────┘ ■ portada □ capt □ título │
├ (Contenido) Arte Extras Partidas Archivo ┤ ← saltos, marca la visible
│ ▾ Versión ........ 2 act · v1.10 │
│    v1.10 …  [La más nueva]       │
│ ▸ DLC ...... 1 · Lighthouse Keeper│
│ ▸ Demo ........ Lantern Harbor (Demo)
│ ▸ Mods y trucos .. 1 mod · 2 trucos
│ ▸ Arte ................. 1 de 3  │
│ ▸ Extras ...... 11 · 6 categorías│
│ ▸ Partidas ....... vacía · bloque 5
│ ▸ Archivo e identidad . 17,1 GB  │
│ ▶ reproductor                    │
└──────────────────────────────────┘
```

- **Reparto:** cabecera fija con portada, sistema, región, versión en uso y estado de los tres huecos; una barra de saltos que marca la sección visible (scrollspy); cada sección plegable muestra en su cabecera un resumen de una línea.
- **Cómo absorbe 3–6:** cada bloque nuevo es una fila de una línea mientras está plegado. La ficha del bloque 3 (año, desarrolladora, jugadores) entra en el resumen fijo; la sinopsis y la galería, en secciones.
- **A favor:** es lo más cercano a hoy (reutiliza `data-panel-section`, `usePanelSection` y el `<details>` de las carpetas); no esconde nada porque cada sección plegada dice lo esencial; el estado plegado se recuerda por sección, no por juego, como los paneles de Lightroom.
- **En contra:** sigue siendo un scroll si se abre todo (de ahí la opción «solo una abierta», como el modo solo de Lightroom); cabecera + saltos ocupan unos 170 px fijos.
- **Teclado y accesibilidad:** cada cabecera es un botón con `aria-expanded` (patrón Accordion del APG); la barra de saltos es una `nav` con enlaces; el salto desde la cuadrícula abre y enfoca la sección como hoy.
- **Esfuerzo:** bajo a medio. Es una reorganización de lo que ya existe más el resumen fijo.

### E. Panel ancho de dos columnas (redimensionable)

```
┌ Lantern Harbor (USA) ─────────────────────────── [▢][×] ┐
‖ ┌──────────┐ │ Contenido 4 │ Extras 11 │ Partidas │ Archivo
‖ │ portada  │ │ Versión ......... 2 actualizaciones       │
‖ │ grande   │ │ v1.10 · …(v1.10).pkg    [La más nueva]    │
‖ └──────────┘ │ DLC · Demo                                │
‖ [capt][tít][fondo]                                       │
‖ PS4 · EE. UU. │ (ficha del bloque 3)                      │
‖ [Rellenar arte] [Identificar]                            │
└──────────────────────────────────────────────────────────┘
 ‖ = borde que se arrastra
```

- **Reparto:** el borde izquierdo se arrastra; por encima de ~560 px el panel pasa a dos columnas (arte y resumen a la izquierda, datos a la derecha con B, C o D); por debajo vuelve a una columna.
- **Cómo absorbe 3–6:** el arte de 3.1 (fondo, logo, galería) gana sitio propio en la columna izquierda.
- **A favor:** en pantallas grandes usa el ancho que hoy sobra en la cuadrícula; el arte deja de competir con las listas.
- **En contra:** quita columnas a la cuadrícula; dos puntos de corte que probar (container queries); no resuelve la longitud por sí solo, se combina con B, C o D.
- **Teclado y accesibilidad:** el separador es `role="separator"` enfocable con ←/→ y su valor (`aria-valuenow`), y doble clic para volver al ancho por defecto.
- **Esfuerzo:** medio. Ancho guardado en la vista, container queries y el separador.

### F. Ficha completa (página u overlay)

```
┌──────────────────────────────────────────────────────────────┐
│ [← Biblioteca]                         ┆ Sin fondo (3.1)    ┆ │
│  ┌──────┐                                                    │
│  │portada│ PS4 · EE. UU. · CUSA31415                         │
│  │      │ Lantern Harbor        [Ver en el Finder][Rellenar] │
│  └──────┘ v1.10 · 1 DLC · demo · 11 extras                   │
│ Resumen │ Contenido 4 │ Arte 1/3 │ Extras 11 │ Partidas │ …  │
│ sinopsis + versión │ ficha + arte │ partidas · dispositivos  │
└──────────────────────────────────────────────────────────────┘
```

- **Reparto:** «Abrir ficha» (o doble clic con el panel ya abierto) ocupa el área central: fondo arriba, portada y título, pestañas y tres columnas. El panel lateral sigue como vista rápida (A, B, C, D o G). Con la ventana estrecha puede sustituir al panel, como el patrón de lista y detalle de Microsoft, y fijar un juego aunque cambie la selección, como la ventana de Información de las HIG.
- **Cómo absorbe 3–6:** es el sitio natural para todo lo del bloque 3 que no cabe en 352 px: sinopsis, galería con autor, logros con iconos, el fondo con el logo encima («El logo va sobre el fondo», tarea de 3).
- **A favor:** sitio de sobra; el panel puede quedarse corto y rápido; Escape vuelve con la selección intacta.
- **En contra:** una superficie nueva entera que diseñar y mantener; se pierde de vista la lista; sin fondo descargado, la cabecera es un hueco discontinuo (correcto según la gramática, pero vacío).
- **Teclado y accesibilidad:** foco al título al abrir, Escape cierra y devuelve el foco a la portada de origen, pestañas con el patrón APG.
- **Esfuerzo:** alto. Tiene sentido cuando exista la ficha del bloque 3, no antes.

### G. Compacto (densidad)

```
┌ Lantern Harbor (USA) [Compacto|Completo] ┐
│ [port] PS4 · EE. UU. · v1.10 ■□□ │
│ ≡ Versión ......... 2 act · v1.10│
│ ≡ DLC ......................... 1│
│ ≡ Demo ....................... sí│
│ ≡ Mods y trucos . 1 mod · 2 trucos
│ ▣ Arte ................ faltan 2 │
│ ♫ Extras ..................... 11│
│ ◌ Partidas ............. bloque 5│
│ ≡ Archivo ................ 17,1 GB
└──────────────────────────────────┘
```

- **Qué es:** un ajuste de densidad, no un diseño aparte: solo el resumen y una línea por sección, que al pulsarla abre la sección en el diseño elegido.
- **Cómo absorbe 3–6:** una línea por bloque nuevo.
- **A favor:** cabe entero a 900×700; ideal con el Inspector fijo recorriendo la biblioteca; barato si D ya tiene los resúmenes de una línea.
- **En contra:** para editar hay que abrir la sección; un ajuste más.
- **Teclado y accesibilidad:** cada línea es un botón con su resumen en el nombre accesible; la densidad es un grupo de botones con `aria-pressed`.
- **Esfuerzo:** bajo, si va después de D.

### H. Secciones a medida y barra de comandos

```
┌ Lantern Harbor (USA)   [⚙]   [×] ┐   ┌ Lantern Harbor (USA)        [×] ┐
│        ┌ Diseño [Secciones|Pestañas|Rail]   │ Buscar: arte            ⌘K │
│        │ ⋮⋮ ☑ Contenido     │   │ Acciones                      │
│        │ ⋮⋮ ☑ Arte          │   │ ▸ Buscar arte: captura      ↵ │
│        │ ⋮⋮ ☑ Extras        │   │   Buscar arte: pantalla título│
│        │ ⋮⋮ ☐ Partidas  (5) │   │ Ir a                          │
│        │ ⋮⋮ ☑ Archivo       │   │   Sección Arte                │
│        │ [Restablecer] [Compacto|Completo] │ Extras › Artwork/ (1)  │
└──────────────────────────────────┘   └───────────────────────────────┘
```

- **Qué es:** dos complementos válidos para cualquier diseño. «Personalizar panel» oculta, muestra y reordena secciones (como los campos de Calibre o los paneles de Lightroom) y es donde se elige el diseño y la densidad. La barra de comandos (⌘K con el foco en el panel) busca acciones y secciones por nombre.
- **Cómo absorbe 3–6:** cada sección nueva aparece en la lista; quien no use partidas o dispositivos la oculta.
- **A favor:** cada uno quita lo que no usa; la barra de comandos llega a todo sin ratón.
- **En contra:** más estados que probar; sin «Restablecer» el panel puede quedar irreconocible.
- **Teclado y accesibilidad:** reordenar con botones «Subir/Bajar» además de arrastrar; la barra de comandos como `combobox` + `listbox`.
- **Esfuerzo:** medio para «Personalizar»; medio-alto para la barra de comandos (conviene que sea la misma que la de la app, si llega a existir).

### Comparación

| | Longitud con datos de 3–6 | Esconde | Escala a 8+ secciones | Ancho que necesita | Esfuerzo |
| --- | --- | --- | --- | --- | --- |
| A. Actual | Muy larga | No | Mal | 352 px | — |
| B. Pestañas horizontales | Una pestaña por pantalla | Sí | Regular (fila desplazable) | 352 px | Medio |
| C. Rail vertical | Una sección por pantalla | Sí, con avisos visibles | Bien | 352 + 46 px | Medio |
| D. Resumen + plegables | Corta plegada, larga abierta | No (resumen por sección) | Bien | 352 px | Bajo-medio |
| E. Dos columnas | Depende de B/C/D | Según combinación | Bien | 560+ px | Medio |
| F. Ficha completa | Sin límite práctico | No | Bien | Área central | Alto |
| G. Compacto | Una pantalla | Sí (detalle) | Bien | 352 px | Bajo |
| H. A medida + comandos | La que elija el usuario | Lo que se oculte | Bien | 352 px | Medio |

## 4. Recomendación

> Escrita antes de la decisión del 2026-11-15 (implementar los ocho). Sigue valiendo como orden de trabajo y como diseño predeterminado.

1. **Empezar por D** (resumen fijo + secciones plegables + barra de saltos) como diseño predeterminado. Arregla lo que más cuesta hoy (sin resumen, arte enterrado, Archivo al final) sin cambiar el modelo mental del panel, y reutiliza `data-panel-section`, `usePanelSection` y el plegable de las carpetas.
2. **Construirlo sobre un registro de secciones** desde el primer día. Con él, **B (pestañas horizontales) y C (rail vertical)** son dos formas más de pintar el mismo registro, que es exactamente lo que pidió Miguel en el punto 1, y **A** sigue disponible como «Lista» (punto 2).
3. **Sí a varios diseños, pero pocos:** «Secciones» (D, predeterminado), «Pestañas» (B), «Rail» (C) y «Lista» (A). Se elige desde el panel (botón de personalizar) y desde Configuración; se recuerda entre sesiones; al cambiar se mantiene la sección abierta. G (compacto) es un ajuste de densidad que vale para todos.
4. **H (mostrar, ocultar, reordenar)** después, cuando haya secciones de 3–6 que alguien quiera ocultar.
5. **E y F esperan al bloque 3/3.1:** el panel ancho y la ficha completa ganan sentido cuando haya sinopsis, galería, fondo y logos. Antes serían mucho marco para poco contenido.
6. **Pequeños arreglos** que valen para cualquier diseño: no mostrar «.» ni «0 B» en juegos que son carpetas (Type X), y sacar el taller de arte (buscador y candidatas) de la lectura: que se abra al elegir un hueco.

Todo esto se revisa al terminar 3 y 3.1, porque cambian los datos que hay que repartir.
