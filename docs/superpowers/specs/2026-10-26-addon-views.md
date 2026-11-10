# Coverly ROMs — Updates, DLC, demos and revisions in the six views, the game panel and the filters; 3DS region by product code (Design Spec, part C)

> Status: **Design, implementing on `feat/addon-views`** (self-review §16) · Date: 2026-10-26 (roadmap date of item 2.1; design approved by Miguel on 2026-11-09; spec written 2026-11-09) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §2.1 «Actualizaciones, DLC, demos y revisiones dentro de su juego», **part C only**: «Cómo se ve en las seis vistas (diseño aprobado, 2026-11-09)», the task «Interfaz: el juego con sus actualizaciones y DLC plegados debajo, versión instalada y la más nueva disponible», the three «Portadas y arte de actualizaciones, DLC y demos» tasks and «Región de 3DS por el código de producto». Part D (Extras CRUD, `extras.json`, media viewer) is a later run and is not started here.
> Branch: `feat/addon-views`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.
> Builds on: game folders part A ([`2026-10-26-game-folders.md`](2026-10-26-game-folders.md), PR #45: `FixedFolder`, `RomEntry.children`, `RomEntry.role`, `version_group`/`revision` D15, demos D12, region D10, version display D11), mods/cheats part B ([`2026-10-26-mods-cheats-translators.md`](2026-10-26-mods-cheats-translators.md), PR #46: `Mods/`/`Cheats/` filled and linked), system icons and views ([`2026-10-18-system-icons-and-views.md`](2026-10-18-system-icons-and-views.md): the six views, cover sizes, roving focus), the embedded-art source (systems batch 2 D17, batch 3 D21: `sources/embedded.rs`, `ArtLocator`).
> Visual source of truth: the approved mockup `https://claude.ai/artifact/UPUWX45c9Mj5B1o5hgFrTg` (local copy `views-21c/index.html` with `design-notes.md` and `research-1..3.md` in the session scratchpad). This spec reproduces its hierarchy, copy and interactions with the app's own tokens and primitives; it imports none of the mockup's CSS, fonts or ad-hoc colours.

## 1. Purpose

After parts A and B a game is a folder with its base, `Updates/`, `DLC/`, `Mods/`, `Cheats/`, `Extras/` and `Saves/`, and the scan already knows a base's children (`RomEntry.children`), orphan children (`RomEntry.role`) and retro revisions (`version_group`, `revision`). None of it reaches the webview: `EntryView` has no children, the six views draw one tile per scan entry, two revisions of one game are two tiles, an orphan update is a game, the status bar counts «ROMs», and the drawer knows nothing about versions.

Part C delivers:

1. **One game, counted once, in every view.** Updates, DLC and revisions live inside their game and never add to totals. Demos are games of their own, linked to the full game, and the status bar says so.
2. **The approved design in the six views**: summary under the cover (Portadas), DLC tray (Galería), treegrid (Detalles), typed column (Columnas), child art rows (Huecos de arte), DLC edges behind the box (Estantes).
3. **The game panel**: installed version vs newest available, version list, revisions with a persisted default, DLC with thumbnails, related demo, and a read-only group for Mods/Cheats/Extras/Saves.
4. **Filters** «Tiene actualizaciones», «Le falta la última actualización», «Tiene DLC», «Solo demos», and a sidebar entry «Complementos sin juego base».
5. **Art of children**: the icon an update, DLC or demo carries in the clear is read without keys into its own slot beside the child file, with its origin; shown in the panel, the views and Organizar.
6. **3DS region by product code**: a verified letter table (§5) that gives region and, where the letter has one, country; used for names and the panel; unknown letters flagged, never guessed.

The standing rules carry over unchanged: **keys are untouchable** (never read, include, download or decrypt keys, tickets or licences; art only from what is in the clear), **nothing is overwritten**, **every change is undoable**.

## 2. The owner's asks (verbatim from `TODO.md` §2.1)

> ### Cómo se ve en las seis vistas (diseño aprobado, 2026-11-09)
>
> Maqueta aprobada por Miguel: https://claude.ai/artifact/UPUWX45c9Mj5B1o5hgFrTg (las seis vistas, ficha, filtros, claro/oscuro, 390 px, y la sección «Decisiones» con las referencias de la industria). Respuestas a sus preguntas abiertas:
>
> 1. **Versión instalada** = la más nueva que hay en la biblioteca. La del dispositivo llega con el bloque 4.
> 2. **La más nueva disponible** sale de la lista offline de títulos de 3.1 donde exista; si no, solo se compara dentro de la biblioteca.
> 3. **Las demos cuentan** en el total de juegos, y la barra de estado lo dice.
> 4. La barra de estado dice **«juegos»**, no «ROMs»: «9 juegos · dentro de ellos: 5 actualizaciones y 5 DLC (no cuentan como juegos)».
> 5. **Estantes:** los cantos de los DLC asoman detrás de la caja; «Al lado» como opción.
> 6. **Portadas pequeñas:** el recuento se reduce a un punto en la esquina.
> 7. **Revisiones retro:** por defecto la Rev más alta; se puede cambiar en la ficha.
> 8. Entrada **«Complementos sin juego base»** para actualizaciones y DLC cuyo juego no está en la biblioteca.
>
> Debe resolver, como mínimo:
>
> - **Portadas y Galería:** un indicador discreto en la portada (p. ej. un recuento «+3 DLC · v1.05») que no tape el arte, y el detalle al abrir el juego.
> - **Detalles (tabla):** columnas de versión y de complementos; la fila del juego que se despliega para listar actualizaciones y DLC (como un árbol), sin que cuenten como juegos en los totales.
> - **Columnas:** sistema → juego → (base / Updates / DLC / Demo) → vista previa, aprovechando la carpeta maestra de 2.1.
> - **Huecos de arte:** mostrar también los huecos de arte de cada DLC y actualización cuando tienen arte propio.
> - **Estantes:** una caja por juego, con los DLC como «complementos» al lado o detrás, sin llenar la estantería de cajas sueltas.
> - **Ficha del juego (panel lateral):** la versión instalada frente a la más nueva que hay en la biblioteca, la lista de DLC con su miniatura, la demo relacionada, las revisiones y las partidas (5).
> - **Filtros:** «tiene actualizaciones», «le falta la última actualización», «tiene DLC», «solo demos».
> - **Accesibilidad:** el recuento y el estado también en texto para lectores de pantalla, nunca solo con color o icono; navegación con teclado dentro del árbol.
>
> ### Portadas y arte de actualizaciones, DLC y demos
>
> - **Demos:** son juegos aparte, con su propia portada y arte. Se buscan como cualquier juego (libretro-thumbnails y ScreenScraper tienen entradas `(Demo)`), y si no hay, se usa el icono incluido o, como último recurso, la del juego completo marcada como «del juego completo».
> - **DLC:** usar el arte incluido en el propio paquete cuando está en claro (por ejemplo `icon0.png` de un `addcont` de PS4/PS5/Vita, `ICON0.PNG` de un PKG de PS3, el SMDH de un CIA de 3DS si lo trae; en Switch el icono va dentro de un NCA cifrado y no se lee). Si no trae, buscar por nombre en ScreenScraper o SteamGridDB, y si tampoco, mostrar la del base con la marca DLC.
> - **Actualizaciones:** casi siempre comparten la portada del base; si traen un icono propio (algunas cambian el icono o el banner), guardarlo como arte de esa versión sin pisar el del juego base.
> - **Dónde se guarda:** en la carpeta del juego, junto a cada archivo (`DLC/…`, `Updates/…`), con el mismo esquema de slots que el resto (portada, título, captura, banner) y su origen anotado («incluido en el paquete», «ScreenScraper»…).
> - **Frontends:** al organizar para un dispositivo, solo se copia el arte que su frontend sabe mostrar (la mayoría solo muestra el del juego base); el resto queda en la biblioteca.
> - **Interfaz:** en la ficha del juego, cada actualización y DLC con su miniatura; la demo con su propia portada y un enlace al juego completo.
>
> - [ ] Leer el arte incluido en actualizaciones, DLC y demos (sin claves, solo lo que va en claro) y buscar el que falte en las fuentes existentes.
> - [ ] Slots de arte por archivo hijo (actualización, DLC) además del del juego, con el origen anotado.
> - [ ] Mostrarlo en la ficha y en Organizar, y copiarlo al dispositivo solo cuando su frontend lo use.
>
> ### Tareas (las de la parte C)
>
> - [ ] Interfaz: el juego con sus actualizaciones y DLC plegados debajo, versión instalada y la más nueva disponible. Hecho a medias en la parte A (PR #45): Organizar pliega actualizaciones y DLC bajo su juego con la versión; falta la ficha y «la más nueva disponible» (parte C, diseño aprobado arriba).
> - [ ] Región de 3DS por el código de producto (Miguel, 2026-11-09: «entre más info y más precisa, mejor»). La última letra del código (`CTR-P-AXXE`, `KTR-…`) da la región y, cuando existe, el país: `E` EE. UU., `P` Europa, `J` Japón, `K` Corea, `C` China, `T` Taiwán, `A` todas las regiones, y las de un solo país (`S` España, `D` Alemania, `F` Francia, `I` Italia, `H` Países Bajos, `R` Rusia, `U` Australia, `V`/`X`/`Y`/`Z` variantes europeas). Usarla para poner la región en el nombre y en la ficha, con el país cuando la letra lo indique, solo si el nombre no la trae ya; la tabla, verificada contra fuentes (GameTDB, 3dbrew) antes de implementarla, y la letra desconocida se marca en vez de adivinarse.

Workflow brief for this run, summarised: reproduce the approved mockup with the app's tokens and primitives; a game counts as 1, updates/DLC/revisions never count, demos do and the bar says so; Portadas summary below the cover (warning tone and «↑» when the newest update is missing), a dot at the smallest sizes; Galería DLC tray; Detalles treegrid with Versión and Contenido columns and a ghost row for a missing update; Columnas system → game → typed groups → preview; Huecos de arte child rows; Estantes edges behind, «Al lado» option; panel as listed; four filters plus «Complementos sin juego base»; accessibility in text, keyboard in the tree, reduced motion; 3DS region table verified against 3dbrew and GameTDB and cited in code. If the 3.1 title list is not in the code, compare within the library and leave a clean seam. Extras CRUD is part D; Saves is block 5 (read-only counts here). Anything that needs the owner: safest default, implemented, listed in §14 and the PR.

## 3. What exists today (read before changing)

| Layer | Where | What matters here |
|---|---|---|
| Scan model | `crates/coverly-core/src/scan/mod.rs` `RomEntry { children: Vec<ChildEntry { path, role: ChildRole { Update, Dlc }, size, modified }>, role: Option<ChildRole>, version_group, revision, is_dir, … }` | Children are measured from metadata only; **no header is read** for them in the scan. Game folders' `Mods/`, `Cheats/`, `Extras/`, `Saves/` are skipped (not counted). |
| Wire | `src-tauri/src/library/view.rs` `EntryView { id, system, rel_path, dir, name, ext, size, mtime, regions, art }`, mirrored in `src/ipc/types.ts`; `src/ipc/contract.sample.json` + `contract.test.ts` pin the shape. | **Nothing** about children, orphans, revisions or demos crosses the bridge. |
| Views | `src/components/library/views/{CoversView,CoverTile,GalleryView,DetailsView,ColumnsView,MountsView,ShelvesView,QuickLook}.tsx`, `useViewKeys.ts` (roving focus), `src/lib/rowModel.ts`, `grouping.ts`, `shelves.ts`, `coverSizes.ts` (fixed tile heights per stop `xs s m l xl`; `xs`/`s` have `meta: 'badge'`, no info line), `detailsColumns.ts`, `folderTree.ts`. | One tile/row per `EntryView`. Cover tile heights are fixed for virtualisation, so any new line changes `metrics()`. |
| Panel | `src/components/library/DetailDrawer.tsx` (sections Arte, Identidad, Archivo; `drawer-section`, `drawer-subtitle`). | No version or content sections. |
| Filters / sidebar / status | `src/lib/filters.ts`, `src/stores/filterStore.ts`, `FilterPanel.tsx`; `Sidebar.tsx` (`views.folders`, `views.systems`, `views.date`, `views.allRoms`); `StatusBar.tsx` (`status.shown` «{shown} de {total} ROMs»). | |
| i18n | `locales/{es,en}/ui.json`, `parity.test.ts`; plurals are `…One` / plain key pairs (`countOne`), chosen in code. | |
| Region | `consoles/mod.rs` `nintendo_region_letter` (shared by 3DS, Wii U, GameCube/Wii: `E P J K W C A D F S I U H`), `region_from_product_code` (base titles only, `ctr.rs` `base_region`), `identify/names.rs` `Region` (19 values, **no Russia**), `Region::no_intro_word`. | The shared table maps `U` → Australia and has no `R`, `V`, `X`, `Y`, `Z`, `T`. |
| Embedded art | `sources/embedded.rs` (`SYSTEMS`: 3ds ps3 ps4 ps5 psvita switch …; re-runs the budgeted readers to get an `ArtLocator { Range, ZipEntry, File }`, reads at most 2 MiB, decodes under limits, re-encodes PNG). | Today only `Media::Cover` of a top-level entry. Switch NCAs are never read. |
| Library profile | `profiles/builtin.rs` `coverly-library`: slots `{system_dir}/{rel_dir}/.media/{stem}.png`, `-screenshot.png`, `-title.png`; `art_dir_names = [".media"]`. | A child at `<Game>/DLC/x.cia` has `rel_dir = <Game>/DLC`, so its `.media/` sits beside it, as the brief asks. |
| Title list (3.1) | `catalog/titledb.rs` keeps `id, name, publisher, release, region, icon_url` from `blawar/titledb`; **no versions**. GameTDB is not integrated. | «La más nueva disponible» has no offline source in the code yet (D6). |
| Organizar | `src/components/organize/SystemGroups.tsx` folds children by `game.of` with the version; `GameLink.version`, `newest_here`. | Child rows have no thumbnail. |

## 4. Decisions

| # | Topic | Decision | Why |
|---|---|---|---|
| D1 | The game model | A **view-layer projection** `buildGames(entries) → Game[]` in a new `src/lib/games.ts`. A `Game` is one top-level entry that is not an orphan child, plus its children (from the wire, D2) and, for retro revisions, the other entries of its `version_group` folded under the **default revision** (D7). Fill, gamelists, audit, Duplicados and Organizar keep working on scan entries unchanged: a revision is still its own file with its own art slots. Every view, filter, count and selection works on `Game`; the selected id is the game's representative entry id, so existing drawer/art code keeps its handle. | The core model of part A is right for writing; the folding is about what the user sees. Doing it once, in one pure tested module, keeps the six views consistent (the mockup's single `GAMES` list). |
| D2 | Wire additions | `EntryView` gains optional fields, all `#[serde(skip_serializing_if)]` so the shape of an ordinary retro entry is unchanged: `children?: ChildView[]` (`{ rel_path, name, role: 'update' \| 'dlc', version?, size, mtime, art: ChildArtMarks }`, sorted newest version first, then name), `orphan?: 'update' \| 'dlc'` (from `RomEntry.role`), `orphan_of?: number` (the game's entry id when D6 links the orphan by title key), `version_group?`, `revision?`, `demo?: { of?: number /* entry id */, of_name?: string }`, `version?` (the base's own version, only when its name carries one), `addons?: { mods, cheats, extras, saves }` (file counts, D9), `title_key` is **not** sent (grouping keys are computed in Rust). `ChildView.version` is parsed from the child's name `(v1.05)` / `[v131072]` (part A names every child with its version, D11 there), never from a header in the scan. `contract.sample.json` gains one game with every field. | No new IPC round-trip for the views: everything the six views and filters need arrives with the library. Header-derived facts (title id, product code, child icons) are lazy (D3). |
| D3 | Lazy details | New command `game_details(entry_id) → GameDetailsView { title_id?, product_code?, region?: RegionVerdict (D12), latest?: LatestView (D6), children_art: … }`, read when the panel opens: the base's and children's headers through the existing budgeted console readers (`identify_dir_on_disk` / readers), memoised by `(path, size, mtime)` in the existing facts cache. Cancelled when the selection changes. | The scan stays header-free (part A D8 cost rule). The panel is the only place that needs a title id or a product code. |
| D4 | Counting | **Juegos** = games (D1) that are not orphans; demos count. Updates, DLC, revisions beyond the default and orphans never count. Status bar (owner's answer 4, plus the demo clause of answer 3): «9 juegos, 2 de ellos demos · dentro de ellos: 5 actualizaciones, 5 DLC y 1 revisión más (no cuentan como juegos)». With filters: «4 de 9 juegos». Each clause is omitted when its count is 0. Orphans add «· 2 complementos sin juego base» as a button that opens that sidebar entry. The existing «{n} sin portada» and scan-time spans stay, counted over games. Bulk actions (fill selection, select all) act on games only, never on child rows. | Owner's answers 3, 4 and 8. The Lightroom collapsed-stack bug (research-3): selecting a stack must not silently include hidden children. |
| D5 | Installed version | **Installed = the newest version present in the library** for that game: the highest `ChildView.version` among its updates, else the base's own `version`, else none («sin actualizaciones»). Ordering: `compareVersions` in `src/lib/versions.ts`, numeric per dot-separated segment with a leading `v` stripped (`01.05` < `01.10`, `1.0.2` < `1.3.0`; segments equal as numbers but different as text, like `1.05` and `1.5`, fall back to comparing the text), Switch raw decimals as one integer; two versions that do not parse compare equal and neither is called «la más nueva». The device version is block 4. | Owner's answer 1. Same order as Rust `version_order` for the display forms part A writes (a Vitest table pins the shared cases). |
| D6 | Newest available | A seam in core: `pub trait LatestVersions { fn latest(&self, system: &str, base_title_id: &str) -> Option<LatestKnown { version: String, source: LatestSource } > }` in a new `crates/coverly-core/src/library/latest.rs`, with `NoTitleList` as the only implementation in part C (3.1 has no version data yet: `titledb.rs` keeps none, GameTDB is not integrated). The panel shows the «Es la última que existe, según la lista de títulos» / «Existe la v1.3.0 y no está en tu biblioteca» callouts **only** when a source answered. Without one, **in-library comparison**: a game «le falta la última» when an orphan update of the same system whose game folder name has the same title key (`parse_tags(..).title_key`, computed in Rust and sent as `orphan_of?: number` on the orphan entry) has a higher version than the game's installed version: the newer update is in the library, just not in the game's `Updates/`. Panel text: «La v1.3.0 está en tu biblioteca, fuera de la carpeta del juego.» with a button that selects it under «Complementos sin juego base». | Owner's answer 2. Never claim a «latest» nobody told us; the in-library case is real and actionable. When 3.1 lands, it implements the trait and the UI needs no change (§10). |
| D7 | Retro revisions | Entries sharing a `version_group` (more than one) fold into one game. **Default = the highest revision** (`revision` ordered: untagged < `Rev 1` < `Rev 2`…, `Rev A` < `Rev B`, `v1.0` < `v1.1`; unparseable revisions sort by name after parseable ones), unless the user chose another in the panel. The choice is persisted per library in the core settings: `LibrarySettings.revision_defaults: BTreeMap<String /* version_group */, String /* rel_path */>` (`#[serde(default)]`), set by `set_default_revision(group, rel_path)` (validates that `rel_path` is a current member; a stale choice falls back to the highest Rev silently and is pruned on next save). In part C the default decides which file represents the game in the six views (its art, its name, its row). The panel copy says what it does today: «La predeterminada es la que se ve en la biblioteca y la que se copiará al dispositivo cuando llegue la sincronización.» | Owner's answer 7. Settings already persist per library (`ignored_dirs`); block 4 reads the same map. |
| D8 | Demos | A top-level entry is a demo when `parse_tags(name)` has a `(Demo)`, `(Trial)` or `(Kiosk Demo)` tag (the same rule as part A D12 for names; the header kind is not read in the scan). Linked to its full game by title key in the same system (Rust, sent as `demo.of`); the demo's own art is filled like any game. Display fallback when it has no cover: the full game's cover dimmed with the «del juego completo» label (never written to disk). A demo's panel has a «Demo» section: «Abrir el juego completo» card, or «El juego completo no está en tu biblioteca.» | Brief and approved panel. |
| D9 | Mods/Cheats/Extras/Saves counts | The scan counts regular files under a game folder's `Mods/`, `Cheats/`, `Extras/`, `Saves/` (metadata only, depth ≤ 4, at most 10 000 per folder; when capped the count is sent as `10000` with `capped: true` and the UI writes «más de 10 000»). Panel group «Mods, trucos, extras y partidas», **read-only**, collapsed by default (`<details>`), with the block-5 placeholder «Aquí se verán las partidas de este juego, con fecha y emulador, cuando llegue el bloque 5.» No Extras CRUD (part D). | The approved panel; read-only keeps part D's scope intact. |
| D10 | Child art slots | Two slots per child file, `Icon` (square) and `Banner`, stored **only in the `coverly-library` layout**, beside the child: `<Game>/DLC/.media/<stem>-icon.png`, `<stem>-banner.png`, with the origin in `<Game>/DLC/.media/<stem>.art.json` (`{ "icon": { "origin": "package" \| "screenscraper" \| "steamgriddb" \| "user", "at": <unix> } }`). A new `ChildMedia { Icon, Banner }` enum, not new `Media` values: profile slots for top-level games are untouched. Other layouts get no child art (they have no game folders). Writes go through the existing art writer (`write::ensure_within`, journal, undo); an existing file is never overwritten without the usual backup. `.media/` under `Updates/`/`DLC/` is already skipped by the scan (`art_dir_names`). | Brief «Dónde se guarda»; separate enum avoids touching every profile and the fill matrix. |
| D11 | Reading child art | **In the clear only**, through `sources/embedded.rs` extended to take a child path: the reader re-runs on the child, takes its `ArtLocator` (3DS CIA meta SMDH large icon when the CIA carries a meta section, PS3 PKG `ICON0.PNG`, Vita/PS4/PS5 folder or zip `sce_sys/icon0.png`, Wii U folder `meta/iconTex.tga`, Xbox 360 STFS thumbnail) and writes `Icon` with origin `package`. `Banner` only from plaintext files of folder children (`PIC1.PNG`/`sce_sys/pic1.png`, Wii U `meta/bootTvTex.tga`). Switch: none (NCAs encrypted). Never a ticket, `.tik`, `.rap`, zRIF or any `is_key_material` file. Triggers: (a) Organizar apply with `coverly-library`, after a child lands in its folder (journaled, undone with it); (b) panel button «Leer el arte incluido» for children already in the library (a small job with undo). **Network search for a child is manual only**: «Buscar arte» on a child row opens the existing drawer search with the query `<game> <child name>` and writes on an explicit pick, origin `screenscraper`/`steamgriddb`; never automatic, because DLC names match poorly (§14 Q3). | Brief task 1, keys rule, «nothing overwritten». |
| D12 | 3DS region | A new 3DS-only function `ctr_region(code) -> RegionVerdict { letter, region: Option<Region>, country: Option<Region>, certainty: Certain \| Likely \| Unknown }` with the table of §5, used by `ctr.rs` for base titles **and demos** (3dbrew: category `B`/`T` codes are cartridge and eShop demos with the same game-code shape). The shared `nintendo_region_letter` stays for Wii U, GameCube and Wii (no behaviour change there). Names (Organizar, part A D10): the region joins `<Game>` only when `certainty == Certain` and `parse_tags(stem).regions` is empty, as the country's No-Intro word when the letter names a country (`(Spain)`, `(Germany)`, `(Russia)`), else the region's (`(Europe)`, `(USA)`, `(World)`). `Likely` and `Unknown` never touch a name. Panel: «Región: Europa, España» / «Región: Europa (variante europea, según GameTDB)» / «Letra de región «T» sin identificar» (warning tone, with the code). Organizar: `ItemNote::RegionLetterUnknown { letter }` («Letra de región «T» sin identificar; no se añade región al nombre.»). New `Region::Russia` (No-Intro `Russia`, wire `russia`) added to the Rust enum, `REGIONS` and both locales. | Owner's ask, «más info y más precisa», verified table; the brief's own letters `T` and `U` are **not** confirmed for 3DS by either source (§5), so they are flagged rather than trusted. |
| D13 | «Complementos sin juego base» | A sidebar entry under the views list, shown only when the library has orphans, with its count. It lists orphan updates/DLC (Detalles-style list in every view mode: a game-less child cannot have a cover tile). Each row says what it is, its version, its folder, and «Su juego no está en la biblioteca» or, when D6 found a game by title key, «Es de «Tidewalker»; está fuera de su carpeta» with a button «Ver el juego». No move action in part C (Organizar already places them; §14 Q4). | Owner's answer 8 (Xbox «Leftover add-ons»). |
| D14 | Filters | Four chips in a «Mostrar juegos que» row above the views (the mockup's placement), combined with AND and with the existing filter panel, each with its live count over the current scope: «Tiene actualizaciones», «Le falta la última actualización», «Tiene DLC», «Solo demos». Built on `ToggleGroup` (`type="multiple"`) so each chip is `aria-pressed`. A polite live region announces «{n} juegos con estos filtros» / «Ningún juego cumple estos filtros.» and «Quitar filtros» clears them (existing key). Chips with count 0 stay enabled (pressing gives the empty state); «Le falta…» carries a tooltip «Compara solo dentro de tu biblioteca hasta que haya lista de títulos.» while D6 has no source. Persisted with the other filters in `filterStore`. | Brief; honest about the in-library scope. |
| D15 | Organizar thumbnails | Child rows in `SystemGroups.tsx` show a 24 px thumbnail of the icon the child carries in the clear, fetched lazily with the existing embedded-candidate preview for the **source** file (no write). None → no thumbnail, no placeholder noise. | Brief «Mostrarlo … en Organizar». |
| D16 | Device copy | No device profile shows child art today, so no translator or profile copies it. A `Profile.child_art: bool` (default `false`, `#[serde(default)]`) is added so a future frontend can opt in; translators skip `.media/` under `Updates/`/`DLC/` unless it is set. | Brief «solo se copia el arte que su frontend sabe mostrar». |
| D17 | Reduced motion | The only new motions are the treegrid chevron rotation and the shelf edges fanning 3 px on hover/selection, both on `--cr-duration-fast` / `--cr-ease-standard`, and both zeroed by the existing reduced-motion token override and `useReducedMotion`. Panel scroll-to-section uses `behavior: 'auto'` under reduced motion. | Brief, design-notes «Motion». |

## 5. 3DS product-code region table (verified 2026-11-09)

Product code shape (3dbrew «Serials»): `[CTR|KTR]-[category]-[type][id][region]` plus a sub id for DLC and demos; category `P` cartridge, `N` digital-only, `B` cartridge demo, `T` eShop demo, `M` DLC, `U` patch. The region is the last letter of the 4-character game code. `CTR-P-CTAP` is the generic default of secondary NCCHs and is ignored (no region from it).

Sources, cited in the code comment above the table:

- **3dbrew, «Serials»** (`https://www.3dbrew.org/wiki/Serials`, fetched 2026-11-09): `E` English (US), `P` PAL (Europe/Australia), `J` Japan, `K` Korea, `C` Chinese (China/Taiwan), `W` Taiwan (Taiwan/Hong Kong, marked «(?)»), `Y` and `Z` «Multiple regions», `A` all (region-free).
- **GameTDB 3DS database** (`https://www.gametdb.com/3dstdb.zip`, snapshot 2026-11-09): last letter of every 4-character id against its `<region>` field: `E` 1304 NTSC-U; `P` 1302 PAL; `J` 1566 NTSC-J; `K` 138 NTSC-K; `W` 29 NTSC-T; `A` 68 none / 2 PAL; `C` 3, no region; `D` 33 PAL (languages DE…); `F` 83 PAL; `S` 10 PAL; `I` 11 PAL; `H` 2 PAL (NL); `R` 1 PAL-R (EN, RU); `V` 6 PAL; `X` 14 PAL; `Y` 32 PAL / 1 NTSC-U; `Z` 170 PAL / 5 NTSC-U / 5 NTSC-T. **No** 3DS id ends in `T`, `U`, `B`, `G`, `L`, `M`, `N`, `O`, `Q`.
- **WiiBrew, «Title database», Region Codes** (`https://wiibrew.org/wiki/Title_database`): meaning of the single-country letters in Nintendo game codes: `D` German-speaking, `F` French-speaking, `I` Italian-speaking, `S` Spanish-speaking regions («only if separate versions exist»), `T` = **Korea with English language**, `W` Taiwan and Hong Kong.

| Letter | Region | Country | Certainty | Evidence |
|---|---|---|---|---|
| `E` | USA | — | Certain | 3dbrew, GameTDB 1304/1304 |
| `P` | Europe | — | Certain | 3dbrew, GameTDB 1302/1302 |
| `J` | Japan | — | Certain | 3dbrew, GameTDB |
| `K` | Korea | — | Certain | 3dbrew, GameTDB |
| `W` | Taiwan | — | Certain | 3dbrew (Taiwan/Hong Kong), GameTDB NTSC-T 29/29 |
| `A` | World | — | Certain | 3dbrew region-free; GameTDB 68 untagged |
| `D` | Europe | Germany | Certain | GameTDB PAL 33/33; WiiBrew German-speaking |
| `F` | Europe | France | Certain | GameTDB PAL 83/83; WiiBrew French-speaking |
| `S` | Europe | Spain | Certain | GameTDB PAL 10/10; WiiBrew Spanish-speaking |
| `I` | Europe | Italy | Certain | GameTDB PAL 11/11; WiiBrew Italian-speaking |
| `H` | Europe | Netherlands | Certain | GameTDB PAL 2/2, Dutch only |
| `R` | Europe | Russia | Certain | GameTDB PAL-R 1/1 (EN, RU) |
| `V`, `X` | Europe | — | Certain | GameTDB PAL 6/6 and 14/14 (Nordic/multi-language PAL variants) |
| `Y`, `Z` | Europe | — | Likely | GameTDB mostly PAL (32/33, 170/180); 3dbrew «multiple regions» |
| `C` | China | — | Likely | 3dbrew «China/Taiwan»; GameTDB 3 entries, untagged |
| anything else (`T`, `U`, `B`, `G`, `L`, `M`, `N`, `O`, `Q`, digits) | — | — | Unknown, flagged | not attested for 3DS |

Differences from the brief, implemented safely and listed for Miguel (§14 Q1): the brief's `T` Taiwán is contradicted (3DS Taiwan is `W`; on Nintendo game codes `T` is «Korea with English»); `U` Australia is not attested on 3DS (Australian 3DS games use `P`); `C` is «China/Taiwan» in 3dbrew, so it is shown but not written into names. Country letters write the country into the name (`(Spain)`), and the panel shows region and country.

## 6. Architecture and files

| Area | Files | Change |
|---|---|---|
| Region | `crates/coverly-core/src/consoles/ctr_region.rs` (new), `consoles/ctr.rs`, `consoles/mod.rs`, `identify/names.rs`, `organize/plan.rs`, `organize/target.rs` | `ctr_region` + `RegionVerdict` (D12, §5); `base_region` uses it for `Base` and `Demo`; `Region::Russia`; `ItemNote::RegionLetterUnknown`; names use certainty and country. `FACTS_VERSION` bumped (a unit test pins the number) because the region of a fact can change. |
| Scan | `scan/mod.rs` | `FixedFolderCounts { mods, cheats, extras, saves, capped }` on `RomEntry` (D9); demo flag and demo/orphan links computed after the walk per system by title key (pure, tested). |
| Latest seam | `crates/coverly-core/src/library/latest.rs` (new), `lib.rs` | `LatestVersions`, `LatestKnown`, `LatestSource`, `NoTitleList` (D6). |
| Settings | `settings/model.rs`, `settings/apply.rs` | `revision_defaults` (D7), pruned on save. |
| Child art | `art/children.rs` (new), `sources/embedded.rs`, `organize/apply.rs`, `write/undo_organize.rs` | `ChildMedia`, slot paths, `.art.json` origin, embedded read for a child path (D10, D11); apply hook after a child lands. |
| Profiles | `profiles/mod.rs`, `translate/*` | `Profile.child_art` default false; translators skip child `.media/` (D16). |
| Tauri | `src-tauri/src/library/view.rs`, `library/mod.rs`, `library/details.rs` (new) | `EntryView` optional fields (D2), `ChildView`, commands `game_details`, `set_default_revision`, `read_child_art` (job), `child_art_url` (serves a child slot like the existing art URLs). |
| CLI | `crates/coverly-cli/src/library.rs` / `organize.rs` | `coverly scan` summary says «juegos» with the inner counts and orphans; `organize` prints `RegionLetterUnknown`. |
| Wire TS | `src/ipc/types.ts`, `contract.sample.json`, `contract.requests.json`, `tauriBackend.ts`, `backend.ts` | Types and calls. |
| Mock | `src/ipc/mock/addonViewsFixture.ts` (new), `mockBackend.ts` | `?mockPick=addons`: the mockup's library (Lantern Harbor with 2 updates + DLC + demo, Paper Skies demo without its game, Tidewalker missing v1.3.0 with the newer update orphaned, Neon Rhythm with 3 DLC, Kite Days, Star Courier with 2 revisions, Pebble League, Moss Garden, two orphans, one 3DS base with letter `T`), plus `game_details` answers. |
| Model | `src/lib/games.ts`, `src/lib/versions.ts`, `src/lib/addonSummary.ts` (new) | `buildGames`, counts, `compareVersions`, `summaryParts(game)` and `spokenSummary(game, t)` (the accessible phrase). |
| Views | `CoverTile.tsx`, `CoversView.tsx`, `GalleryView.tsx`, `DetailsView.tsx`, `ColumnsView.tsx`, `MountsView.tsx`, `ShelvesView.tsx`, `src/lib/coverSizes.ts`, `rowModel.ts`, `shelves.ts`, `folderTree.ts`, `useViewKeys.ts` | Per §8. |
| Panel | `DetailDrawer.tsx` + new `src/components/library/game/{VersionSection,RevisionsSection,DlcSection,DemoSection,FixedFoldersSection,ChildThumb}.tsx` | Per §8.7. |
| Chrome | `StatusBar.tsx`, `Sidebar.tsx`, new `ContentFilters.tsx`, `filters.ts`, `filterStore.ts`, new `OrphansList.tsx` | D4, D13, D14. |
| Organizar | `SystemGroups.tsx` | D15. |
| Styles | `src/styles/globals.css` (new component classes only, built on `--cr-*` tokens) | §8.1. |
| i18n | `locales/{es,en}/ui.json` | §9. |

## 7. Data flow

1. **Scan** (Rust) builds `RomEntry` as today plus fixed-folder counts; a post-pass per system links demos and orphans to games by title key.
2. **Library view** (Tauri) maps each `RomEntry` to `EntryView` with the optional fields; children's versions from their names; `orphan_of`, `demo.of` as entry ids.
3. **Webview** `buildGames(entries, revisionDefaults)` → `Game[]` (memoised on library generation + defaults). Filters and scope run over games; views render games; the status bar counts from the same list.
4. **Selection** opens the panel with what the wire carries at once (versions, DLC list, demo, counts), then calls `game_details(id)` for title id, product code, region verdict and `latest` (from `LatestVersions`; `NoTitleList` gives none), and child art marks. A stale answer (selection changed) is dropped by request id.
5. **Revision default** → `set_default_revision` → settings saved → library store updates `revisionDefaults` → games rebuilt; live region «Rev 1 es ahora la predeterminada.»
6. **Child art**: Organizar apply (library layout) or «Leer el arte incluido» → embedded reader on each child → PNG to `.media/<stem>-icon.png` + `.art.json`, journaled → `ChildArtMarks` refreshed → panel, Galería tray, Huecos de arte rows update.
7. **3DS region**: reader → `ctr_region` → `ConsoleId.region` (Certain only) and `RegionVerdict` → Organizar names/notes and the panel.

## 8. UX/UI design

### 8.1 Design plan (frontend-design pass, reviewed against generic defaults)

The subject is a librarian's catalogue of a physical-feeling game collection; the app already has its identity (Buckram, Manila, Greyboard ramps; Archivo display, Atkinson Hyperlegible Next UI, Atkinson Hyperlegible Mono for paths, versions and counts; the shelf rule and the dashed missing slot). Part C adds no new palette or face; it spends its one bold move on the **stack**: a game reads as one object with its contents tucked behind it.

- **Colour (existing tokens only):** surface `--cr-surface` / `--cr-surface-sunken` for the summary chip; text `--cr-text-secondary` for summaries; `--cr-warning` / `--cr-warning-bg` (and `-11` text step for contrast) for «↑ falta v1.3.0», always with the word; `--cr-success-bg` for the «Es la última» callout; `--cr-manila` only for «Nuevo» (content found in the last scan), kept under the brand's ~2 % area rule; `--cr-border` dashed with `--cr-border-width-slot` for missing child slots and the ghost row.
- **Type:** Atkinson Hyperlegible Next 13 px for labels; Mono (`--cr-font-mono`) for versions, counts and paths, because they are data the user compares column to column; Archivo only for the panel's big version number («v1.2», display size) — the one typographic accent.
- **Layout:** cover summary sits in the existing `cover-tile__info` band under the art (never over it); Detalles is a single treegrid with children indented 24 px and a 1 px hairline guide; panel sections in the approved order (Versión → Revisiones → DLC → Demo relacionada → Mods, trucos, extras y partidas) separated by the existing `drawer-section` rule. Left-aligned throughout; numbers right-aligned in table cells.
- **Motion:** only D17 (chevron rotation, 3 px fan of shelf edges), both reduced-motion aware.

Review against generic defaults (changes made): no pill badge on the cover corner (the generic «badge on card» — the brief forbids covering art; the summary goes under it); no Manila for counts (Manila means «just done», so counts are neutral sunken chips); no separate tiles for DLC in Estantes (the generic «everything is a card» — DLC are edges behind one box); no all-caps eyebrows for panel sections (sentence-case `drawer-subtitle` as today); the meta-string middle dot appears only where the owner approved copy uses it («+3 DLC · v1.05», the status bar), nowhere else.

### 8.2 Portadas

- Under the cover, in the info band, a neutral mono chip with the summary parts in order: `+N DLC`, `N rev.`, `vX` (installed). Missing newest update → the version part in warning tone with «↑» (`v1.0.2 ↑`). New-since-last-scan content → Manila «Nuevo» tag on the system/region line (as the mockup).
- Click or Enter on the chip opens the panel scrolled to its Versión/content section (focus moves to the section heading).
- `xs` and `s` sizes (no info band): a 8 px dot in the art's top-right corner (`--cr-text-secondary`, warning colour with a 1 px ring when the latest is missing) that is `aria-hidden`; the full phrase is in the tile's accessible name. `coverSizes.ts`: `m`/`l`/`xl` tiles reserve one 18 px summary line **when any game in the library has content** (a library flag), so fixed heights hold and libraries without updates look exactly as today.
- Demos: «Demo de Lantern Harbor» line; no chip.
- Accessible name per tile (D-a11y below): «Tidewalker, Switch. 1 DLC, 1 actualización, la más nueva v1.0.2, falta la última actualización, v1.3.0».

### 8.3 Galería

- Under the big cover, a tray of 48 px child thumbnails: each DLC, and each update with its own art, captioned («Actualización v1.2», «Isla del faro»); a child with no art shows the dashed missing slot; a DLC using the base art shows it dimmed with «del juego base». Each thumbnail is a button that opens the panel at that child.
- The side facts list gains Versión, DLC (count), Demo («En la biblioteca» / «No está en la biblioteca»).
- Filmstrip captions carry only `+3`, `↑`, `2 rev.`, `demo` in mono.

### 8.4 Detalles (treegrid)

- `role="treegrid"`, `aria-rowcount`, rows with `aria-level`, `aria-setsize`, `aria-posinset`, `aria-rowindex`; only parents carry `aria-expanded`. Row-first focus with roving `tabIndex`.
- Keys (APG treegrid): ↓/↑ next/previous visible row; → on a collapsed parent expands, on an expanded parent moves to its first child; ← on a child goes to its parent, on an expanded parent collapses; Home/End first/last row; Enter opens the panel (on a child row, the panel at that child); Space toggles selection on game rows only. A one-line key hint under the table (approved copy).
- New columns **Versión** («v1.2», «v1.0.2 ↑ falta v1.3.0», «Rev 1», «—») and **Contenido** («2 act., 1 DLC, demo aparte», «2 revisiones»), both in `detailsColumns.ts` (sortable: Versión by installed version, Contenido by child count), on by default when the library has content.
- Child rows: kind label («Actualización», «DLC», «Revisión», «Demo», «Carpeta»), thumbnail, name, version, «la más nueva» note, size. A missing newest update is a **ghost row** (dashed top border, warning text «↑ v1.3.0 falta», «no está en la biblioteca», no size). The «Carpeta» row summarises «Mods, trucos, extras y partidas» counts. Child rows are not selectable for bulk actions and never counted.
- Virtualisation: `rowModel.ts` flattens expanded games into rows with fixed height; expanded state persisted per session in `viewStore` (the selected game starts expanded, as in the mockup).
- 390 px: Sistema, Contenido and Tamaño columns hide (the mockup's check).

### 8.5 Columnas

System → game (with `+1 DLC` mono suffix) → typed groups **Juego base, Actualizaciones (n), DLC (n), Revisiones (n), Demo**, then a separator and **Trucos, Mods, Extras, Partidas** with counts (read-only, not navigable deeper in part C) → file list → preview (art, «Archivo», «Tamaño», «Arte: Propio, incluido en el paquete» / «del juego base»). «Demo» is a link that jumps to the demo game's own column path. Empty groups are omitted. Keyboard as today in `ColumnsView` (→ enters, ← leaves).

### 8.6 Huecos de arte

- Under each game's slot row, indented sub-rows (with the L-shaped hairline guide) for each DLC and each update **with its own art**: Icon (square) and Banner slots; the origin tag («paquete», «SS», «SGDB», «tuyo») under each filled slot; missing slots dashed with «falta». A DLC with no own art: dimmed base art with «usa el del juego base».
- A toggle in the view bar «Huecos del contenido (6)» (pressed by default when any exist) hides the sub-rows; persisted in `viewPrefs`. Slot actions (search, from file, remove) reuse the drawer's art actions with the child as target (D11: search is manual).

### 8.7 Estantes

- One box per game. Default «Detrás de la caja»: up to 3 DLC edges offset behind the box (7 px down/9 px right each), then «+n» on the last edge; the label line «Con 3 complementos» and, when missing, «↑ falta v1.3.0» in warning tone. Hover/selection fans the edges 3 px (D17).
- «Al lado» (segmented control «Complementos: Detrás de la caja | Al lado», `ToggleGroup type="single"`, persisted in `viewPrefs`): small child icons beside the box.
- Demos: their own box, «Demo de …».

### 8.8 Game panel (DetailDrawer)

Above the existing Arte/Identidad/Archivo sections, in this order:

1. **Head** additions: system and region line from `game_details` («3DS, Europa» / «3DS, Europa, España»), title id in mono; unknown letter → warning line «Letra de región «T» sin identificar (CTR-P-AXXT)».
2. **Versión** (`h3` «Versión», small «2 actualizaciones» / «sin actualizaciones»): big «v1.2» + «la más nueva de tu biblioteca» («(la del juego base)» without updates); callout only when a source answered (D6) or the in-library case applies; version list newest first with «La más nueva» pill, sizes, the base row with «Juego base», and a missing version as a «Falta» row.
3. **Revisiones** («2 archivos, un juego»): one row per revision with a toggle button `aria-pressed` «Predeterminada» / «Usar esta» (`aria-label` «Usar Rev 1 como predeterminada»), and the D7 note.
4. **DLC** («1 en la carpeta DLC/»): thumbnail (own art, base art dimmed, or dashed), name, file in mono (truncated with the full path in `title`), origin («incluido en el paquete», «arte del juego», «sin arte» in warning tone), «Nuevo» tag; per row «Buscar arte»; section button «Leer el arte incluido» when any child lacks art and its system can carry it.
5. **Demo relacionada**: a card button with the demo's cover, «Juego aparte, 184 MB», which selects the demo. On a demo: section «Demo» with «Abrir el juego completo» or the not-in-library sentence, and «La demo es un juego aparte con su propia carpeta y su propia portada. Sus partidas no sirven para el juego completo.»
6. **Mods, trucos, extras y partidas** (`<details>`, collapsed): four folder cells with counts or «vacía», the block-5 placeholder, then the game folder path in mono.

Sections get ids so the cover chip and Galería tray can scroll to them; scroll respects reduced motion.

### 8.9 Accessibility

- Every tile, box, row and slot row has an accessible name with the full phrase built by `spokenSummary` (title, system, then: «demo de X» / «demo; el juego completo no está en la biblioteca», «N DLC», «N actualizaciones, la más nueva vX», «falta la última actualización, vY», «N revisiones, se usa Rev 1», «tiene demo», «contenido nuevo desde el último escaneo»). Visible chips, dots and arrows are `aria-hidden`.
- Warnings always combine «↑», a word («falta») and colour (WCAG 1.4.1); the visible label text is contained in the accessible name (2.5.3).
- Keyboard: treegrid (§8.4), roving focus in every view (`useViewKeys`), chips and toggles are native buttons; focus ring `--cr-border-width-focus`.
- Live regions: filter results and revision default changes (polite).
- Contrast: warning text uses the `-11` step on surfaces (checked ≥ 4.5:1 in both themes in the E2E).

## 9. Copy (es / en)

Spanish follows the house rules: sentence case, no exclamation marks, exact counts, numbers in mono. Plurals as `…One` / plain pairs.

| Key | es | en |
|---|---|---|
| `status.games` / `gamesOne` | `{n} juegos` / `1 juego` | `{n} games` / `1 game` |
| `status.gamesOf` | `{shown} de {total} juegos` | `{shown} of {total} games` |
| `status.demos` / `demosOne` | `{n} de ellos demos` / `1 de ellos demo` | `{n} of them demos` / `1 of them a demo` |
| `status.inside` | `dentro de ellos: {list} (no cuentan como juegos)` | `inside them: {list} (not counted as games)` |
| `status.updates` / `updatesOne` | `{n} actualizaciones` / `1 actualización` | `{n} updates` / `1 update` |
| `status.dlc` | `{n} DLC` | `{n} DLC` |
| `status.revisions` / `revisionsOne` | `{n} revisiones más` / `1 revisión más` | `{n} more revisions` / `1 more revision` |
| `status.orphans` / `orphansOne` | `{n} complementos sin juego base` / `1 complemento sin juego base` | `{n} add-ons without their game` / `1 add-on without its game` |
| `content.filters.label` | `Mostrar juegos que` | `Show games that` |
| `content.filters.updates` | `Tiene actualizaciones` | `Have updates` |
| `content.filters.missing` | `Le falta la última actualización` | `Are missing the latest update` |
| `content.filters.missingHint` | `Compara solo dentro de tu biblioteca hasta que haya lista de títulos.` | `Compares only within your library until a title list is available.` |
| `content.filters.dlc` | `Tiene DLC` | `Have DLC` |
| `content.filters.demos` | `Solo demos` | `Demos only` |
| `content.filters.result` / `resultOne` | `{n} juegos con estos filtros.` / `1 juego con estos filtros.` | `{n} games match these filters.` / `1 game matches these filters.` |
| `content.filters.none` | `Ningún juego cumple estos filtros.` | `No game matches these filters.` |
| `content.summary.dlc` | `+{n} DLC` | `+{n} DLC` |
| `content.summary.rev` | `{n} rev.` | `{n} rev.` |
| `content.summary.missing` | `↑ falta v{v}` | `↑ v{v} missing` |
| `content.new` | `Nuevo` | `New` |
| `content.kind.update` / `dlc` / `revision` / `demo` / `folder` | `Actualización` / `DLC` / `Revisión` / `Demo` / `Carpeta` | `Update` / `DLC` / `Revision` / `Demo` / `Folder` |
| `content.newest` | `la más nueva` | `the newest` |
| `content.notInLibrary` | `no está en la biblioteca` | `not in the library` |
| `content.cell.updates` / `updatesOne` | `{n} act.` / `1 act.` | `{n} upd.` / `1 upd.` |
| `content.cell.demoApart` | `demo aparte` | `separate demo` |
| `content.cell.revisions` | `{n} revisiones` | `{n} revisions` |
| `content.demoOf` | `Demo de {name}` | `Demo of {name}` |
| `content.folderSummary` | `Mods, trucos, extras y partidas` | `Mods, cheats, extras and saves` |
| `content.keysHint` | `Teclado: ↑ ↓ mueven entre filas, → despliega, ← pliega o sube al juego, Intro abre la ficha. Las filas de contenido no cuentan en el total ni entran en las acciones por lotes.` | `Keyboard: ↑ ↓ move between rows, → expands, ← collapses or goes up to the game, Enter opens the panel. Content rows do not count in the total and are left out of bulk actions.` |
| `content.shelves.label` | `Complementos` | `Add-ons` |
| `content.shelves.behind` / `beside` | `Detrás de la caja` / `Al lado` | `Behind the box` / `Beside it` |
| `content.shelves.with` / `withOne` | `Con {n} complementos` / `Con 1 complemento` | `With {n} add-ons` / `With 1 add-on` |
| `content.mounts.toggle` | `Huecos del contenido ({n})` | `Content slots ({n})` |
| `content.mounts.hint` | `Actualizaciones con arte propio y todos los DLC.` | `Updates with their own art and every DLC.` |
| `content.slot.icon` / `banner` | `Icono` / `Banner` | `Icon` / `Banner` |
| `content.slot.missing` | `falta` | `missing` |
| `content.slot.inherits` | `usa el del juego base` | `uses the base game's` |
| `content.origin.package` / `ss` / `sgdb` / `user` / `base` | `incluido en el paquete` / `ScreenScraper` / `SteamGridDB` / `tuyo` / `arte del juego` | `in the package` / `ScreenScraper` / `SteamGridDB` / `yours` / `game art` |
| `content.noArt` | `sin arte` | `no art` |
| `panel.version` | `Versión` | `Version` |
| `panel.updates` / `updatesOne` / `noUpdates` | `{n} actualizaciones` / `1 actualización` / `sin actualizaciones` | `{n} updates` / `1 update` / `no updates` |
| `panel.newestInLibrary` | `la más nueva de tu biblioteca` | `the newest in your library` |
| `panel.baseVersion` | `(la del juego base)` | `(the base game's)` |
| `panel.latestOk` | `Es la última que existe, según la {source}.` | `It is the latest there is, according to the {source}.` |
| `panel.latestMissing` | `Existe la v{v} y no está en tu biblioteca. Dato de la {source}.` | `v{v} exists and is not in your library. From the {source}.` |
| `panel.latestElsewhere` | `La v{v} está en tu biblioteca, fuera de la carpeta del juego.` | `v{v} is in your library, outside the game's folder.` |
| `panel.showIt` | `Ver dónde está` | `Show where it is` |
| `panel.newestPill` / `basePill` / `missingPill` | `La más nueva` / `Juego base` / `Falta` | `Newest` / `Base game` / `Missing` |
| `panel.revisions` / `revisionsSub` | `Revisiones` / `{n} archivos, un juego` | `Revisions` / `{n} files, one game` |
| `panel.revDefault` / `revUse` / `revUseLabel` | `Predeterminada` / `Usar esta` / `Usar {rev} como predeterminada` | `Default` / `Use this` / `Use {rev} as the default` |
| `panel.revNote` | `La predeterminada es la que se ve en la biblioteca y la que se copiará al dispositivo cuando llegue la sincronización.` | `The default is the one shown in the library and the one that will be copied to the device once sync arrives.` |
| `panel.revChanged` | `{rev} es ahora la predeterminada.` | `{rev} is now the default.` |
| `panel.dlcSub` / `dlcSubOne` | `{n} en la carpeta DLC/` / `1 en la carpeta DLC/` | `{n} in the DLC/ folder` / `1 in the DLC/ folder` |
| `panel.readArt` | `Leer el arte incluido` | `Read the included art` |
| `panel.searchArt` | `Buscar arte` | `Search for art` |
| `panel.relatedDemo` | `Demo relacionada` | `Related demo` |
| `panel.demoCard` | `Juego aparte, {size}` | `Separate game, {size}` |
| `panel.demo` / `openFull` / `fullInLibrary` | `Demo` / `Abrir el juego completo` / `{name}, en tu biblioteca` | `Demo` / `Open the full game` / `{name}, in your library` |
| `panel.fullMissing` | `Es una demo de {name}. El juego completo no está en tu biblioteca.` | `It is a demo of {name}. The full game is not in your library.` |
| `panel.demoNote` | `La demo es un juego aparte con su propia carpeta y su propia portada. Sus partidas no sirven para el juego completo.` | `The demo is a separate game with its own folder and cover. Its saves do not carry over to the full game.` |
| `panel.folders` | `Mods, trucos, extras y partidas` | `Mods, cheats, extras and saves` |
| `panel.folderEmpty` / `files` / `filesOne` / `filesMany` | `vacía` / `{n} archivos` / `1 archivo` / `más de {n} archivos` | `empty` / `{n} files` / `1 file` / `more than {n} files` |
| `panel.savesLater` | `Aquí se verán las partidas de este juego, con fecha y emulador, cuando llegue el bloque 5.` | `This game's saves will show here, with date and emulator, once block 5 arrives.` |
| `panel.region` / `regionCountry` / `regionLikely` | `{system}, {region}` / `{system}, {region}, {country}` / `{system}, {region} (variante europea, según GameTDB)` | `{system}, {region}` / `{system}, {region}, {country}` / `{system}, {region} (European variant, per GameTDB)` |
| `panel.regionUnknown` | `Letra de región «{letter}» sin identificar ({code})` | `Region letter “{letter}” not identified ({code})` |
| `orphans.title` | `Complementos sin juego base` | `Add-ons without their game` |
| `orphans.noGame` | `Su juego no está en la biblioteca.` | `Its game is not in the library.` |
| `orphans.ofGame` | `Es de «{name}»; está fuera de su carpeta.` | `It belongs to “{name}”; it is outside its folder.` |
| `orphans.showGame` | `Ver el juego` | `Show the game` |
| `organize.notes.regionLetterUnknown` | `Letra de región «{letter}» sin identificar; no se añade región al nombre.` | `Region letter “{letter}” not identified; no region is added to the name.` |
| `regions.russia` | `Rusia` | `Russia` |
| `spoken.*` | the phrases of §8.9 | same in English |

The «{source}» value is a key per `LatestSource` (unused while `NoTitleList` is the only implementation; the keys ship so 3.1 needs no copy work).

## 10. The seam for 3.1

`LatestVersions` lives in core; Tauri holds a `Box<dyn LatestVersions + Send + Sync>` in its state, `NoTitleList` today. When 3.1 adds an offline title list with versions (titledb `versions.json` for Switch, GameTDB for 3DS/Wii U…), it implements the trait and registers it; `game_details` then returns `latest`, the panel shows its callouts, and «Le falta la última actualización» also counts games whose installed version is below `latest` (the filter predicate already takes `latest?` into account, tested with a fake source). No UI or copy change is needed.

## 11. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Library without any children, revisions or demos | Views, tile heights and status bar copy look as today except «juegos» for «ROMs»; no chips row (the content filters are hidden when every count is 0). |
| Child name without a version | Listed without a version; never «la más nueva»; installed version comes from the others. |
| Two updates with unparseable versions | Neither is «la más nueva»; listed by name; no warning. |
| Revision default points to a file that was renamed or deleted | Falls back to the highest Rev; the stale key is pruned on the next settings save. |
| Revisions in different regions | Different `version_group` (region is part of the key): two games. |
| Demo whose title key matches two games (two regions) | Linked to the one with the same region tag; else none («el juego completo no está en tu biblioteca» is avoided: «demo de X» without a link). |
| Orphan update with a title key matching a game in another system | Never linked across systems. |
| Folder game (`Game.ps3/`) with `Updates/` | Same model; `ChildView.rel_path` relative to the library root. |
| Huge `Mods/` | Counting stops at 10 000 per folder (`capped`), «más de 10 000 archivos». |
| `game_details` fails (file gone, unreadable, budget out) | Panel keeps the wire data, hides title id/region lines, no error toast; a stale answer for a previous selection is ignored. |
| 3DS encrypted NCSD | Product code is in the plaintext NCCH header: region read as today. CIA with encrypted content 0: no product code, no region, no flag. |
| Product code `CTR-P-CTAP` | Generic default: no region, no flag. |
| Unknown region letter | `RegionLetterUnknown` note in Organizar; panel warning line; name unchanged. |
| Child art write target exists | Existing art backup rule (copy + undo), never a silent overwrite. |
| Child is a key/licence file | Never opened (`is_key_material` check before the reader). |
| Switch DLC | No icon (encrypted NCA): dimmed base art «usa el del juego base». |
| Read-only volume | «Leer el arte incluido» is disabled with the existing read-only reason. |
| Narrow window (390 px) | No horizontal page scroll; Detalles hides three columns; Columnas scrolls in its frame; panel is the existing sheet. |
| Treegrid with 10 000 games expanded | Virtualised flat row model; expansion state bounded to the visible set plus the selection. |

## 12. Test plan

**Rust (cargo test):**
- `ctr_region`: every row of §5 (letter → region, country, certainty), `KTR-` prefix, `CTR-P-CTAP` → none, lower case, malformed codes, digits → Unknown; base and demo kinds get it, update/DLC do not.
- Names: `(Spain)` for `S` when the stem has no region; nothing for `Y`/`Z`/`C`/`T`; nothing when the stem already has `(Europe)`; `RegionLetterUnknown` note; `Region::Russia` no-intro round trip; `FACTS_VERSION` pin updated.
- Scan: fixed-folder counts with a cap; demo linking by title key (same system, region preference); orphan → game by title key; fixtures built with pinned mtimes, paths compared by component.
- Settings: `revision_defaults` serde default, prune of stale keys.
- Child art: slot paths for a child under `DLC/` and `Updates/` (component comparison), `.art.json` round trip, embedded read for a CIA with a meta SMDH and a Vita zip child, refusal for a `.tik`, journal + undo of a child art write; translators skip child `.media/` unless `child_art`.
- `LatestVersions`: `NoTitleList` returns none; a fake source feeds `game_details`.
- Tauri: `EntryView` serialisation skips empty optionals (snapshot equals today's for a retro entry); `game_details` with a missing file.

**Vitest:**
- `versions.ts` table (shared cases with `version_order`), `games.ts` (`buildGames` counts, revision folding and default, demos, orphans excluded), `addonSummary.ts` (parts and the spoken phrase in es/en).
- `StatusBar` copy for every clause combination; `ContentFilters` (AND, counts, live region, clear); `Sidebar` orphans entry shown only with orphans.
- `CoverTile` chip, warning, dot at `xs`/`s`, accessible name; `coverSizes` heights with and without the content line.
- `DetailsView` treegrid: roles and aria attributes, every key of §8.4, child rows excluded from select-all, ghost row.
- `ColumnsView` typed groups and demo link; `MountsView` child rows and toggle; `ShelvesView` edges, «+n», «Al lado» persisted; `GalleryView` tray.
- Panel sections: version callouts with and without a source, revisions toggle `aria-pressed` and call, DLC origins, demo both ways, folders group, region lines incl. unknown letter.
- `SystemGroups` child thumbnail lazy load; i18n parity test; contract test with the new sample.

**E2E (Chrome DevTools MCP, isolated context `addon-views`, `pnpm dev` on port 1472, `?mockPick=addons`):** the six views in light and dark at 1440 and 390 px compared with the mockup's screenshots; keyboard walk of the treegrid; filters and status bar numbers; revision default change survives a reload (mock persistence); «Complementos sin juego base»; reduced motion (emulated) removes the fan and rotation; no console errors; no horizontal scroll at 390 px; warning text contrast checked.

**Gates:** `pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. Real-library checks only read `~/Documents/ROMS` and `/Volumes/*`; write tests run on `cp -RXp` copies under `target/`, deleted afterwards.

## 13. Out of scope

Extras CRUD, `extras.json`, the media viewer (part D); saves and their dates (block 5, placeholder only); device version and sync of the revision default (block 4); the offline title list with versions (3.1; seam only); automatic network art for children (manual search only); moving orphans from the «Complementos sin juego base» list (Organizar does it).

## 14. Open questions for Miguel (defaults implemented)

- **Q1 3DS letters `T`, `U`, `C`, `Y`, `Z`.** The brief lists `T` Taiwán and `U` Australia; 3dbrew and GameTDB show 3DS Taiwan as `W` and no 3DS game ending in `T` or `U`. Default: `T`/`U` flagged as unknown, `C`/`Y`/`Z` shown in the panel only (not in names). Confirm or give a source.
- **Q2 Country in the name.** Default: a single-country letter writes the country (`(Spain)`), which is the No-Intro form for those releases. Alternative: always the region (`(Europe)`) in the name and the country only in the panel.
- **Q3 Network art for DLC.** Default: manual search only. Automatic would risk wrong matches for DLC names.
- **Q4 Orphans list actions.** Default: read-only list with «Ver el juego»; moving them is left to Organizar.
- **Q5 Filters row placement.** Default: the mockup's chip row above the views, alongside the existing filter panel (not inside it).
- **Q6 «Le falta la última» without a title list.** Default: in-library comparison only (an orphan newer update), with the tooltip saying so; the chip stays visible.

## 15. Commits (planned, conventional and granular)

`feat(core): 3DS region and country by product code, verified table`; `feat(core): count fixed folders and link demos and orphans in the scan`; `feat(core): latest-version seam with an in-library default`; `feat(core): revision defaults in library settings`; `feat(core): art slots for updates and DLC read from the package`; `feat(tauri): send each game's content and add game details`; `feat(ui): game model, versions and accessible summaries`; `feat(ui): status bar counts games`; `feat(ui): content filters and add-ons without their game`; one `feat(ui)` per view; `feat(ui): game panel sections`; `feat(organize): child thumbnails and unknown region note`; `feat(mock): add-ons library fixture`; `test(e2e): …` evidence in the PR. Each commit pushed at once.

## 16. Self-review

- Placeholders: none; every key has es and en text, every decision a default.
- Consistency checked: counting (D4) matches owner's answers 3 and 4 and the mockup; installed version (D5) matches answer 1; «newest available» (D6) never invents data and matches answer 2; revisions default (D7) matches answer 7 and is persisted; Estantes default (§8.7) matches answer 5; small covers (§8.2) match answer 6; orphans entry (D13) matches answer 8.
- The brief's 3DS letter list and the verified table differ on `T`, `U` and `C`: the table wins, the difference is stated in §5 and Q1, and nothing unverified enters a name.
- Keys rule: no step reads a ticket, licence or encrypted area; Switch child art is explicitly none.
- Part boundaries: no Extras CRUD, no saves reading, no title list; Mods/Cheats/Extras/Saves are counts only.
- Scan cost: no header read added to the scan; header facts are lazy (D3) and memoised.
