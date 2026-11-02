# Settings tab: Integraciones (accounts, keys and services)

Status: design spec, 2026-11-02. Branch `feat/integrations-tab`. Roadmap
item **8. Integraciones (cuentas, claves y servicios)** (`TODO.md`), the
brief Miguel approved on 2026-11-02. The file name keeps the 2026-10-26
date of the settings-tab spec series it belongs to.

Today «Cuentas» is a block at the end of Settings › Fuentes. This item
moves every account, key and opt-in online service into one new tab,
**Integraciones**, fed by one registry that the tab, the settings search,
the notices on other screens and the CLI all read. Stored credentials
keep their keychain names, so nothing has to be entered again.

---

## 1. Goals

1. One place to see and fix every service Coverly talks to on the user's
   behalf: what it is for, whether it works and why not.
2. Secrets go in once and never come back out: not to the webview, not to
   the logs, not to an export, not to a CLI's output or argument list.
3. Every screen that needs a missing account or hits a used-up quota links
   straight to the card that fixes it, and the card links back to Fuentes,
   Red and Agradecimientos.
4. Adding a planned service (IGDB, RetroAchievements…) later means one
   registry entry, its credit and its copy, with no new UI code.

Non-goals: building any planned service; new keychain keys for them; a
settings export/import feature (there is none today, see D17); moving the
update channel out of General; changing what a fill or an organise run
sends to a service.

## 2. The owner's asks (from `TODO.md` §8, verbatim)

> Petición de Miguel (2026-11-02): mejorar la parte de «Cuentas» de Fuentes
> y crear una pestaña nueva en Ajustes, **«Integraciones»**, con todas las
> integraciones que necesitan usuario, contraseña, clave de API o permiso,
> bien diseñada y enlazada desde cada sitio donde se usa.
>
> **Qué tiene que ser la pestaña**
>
> - **Una tarjeta por servicio**, agrupadas en portadas y arte, nombres y
>   metadatos, logros, catálogos y actualizaciones. Cada tarjeta muestra:
>   - el nombre del servicio, para qué lo usa Coverly en una línea y qué
>     desbloquea (por ejemplo, «fondos y logos» o «nombres de Switch»);
>   - un **estado visible**: sin configurar, conectado, error (con el motivo
>     y cómo arreglarlo), cuota agotada o desactivado;
>   - los **campos que pida**: usuario, contraseña, clave de API, client ID y
>     secret. Van ocultos, con «Mostrar», y nunca se rellenan de vuelta con
>     el valor guardado (solo «guardado ✓»);
>   - **«Probar conexión»**, con un resultado claro: funciona, credenciales
>     incorrectas, sin red o servicio caído;
>   - **«Cómo conseguir la clave»**, que enlaza a la página oficial de cuenta
>     o API del servicio (nunca a webs de ROMs, claves o emuladores) con los
>     pasos en una línea;
>   - la **cuota y los límites** cuando el servicio los da: peticiones de hoy
>     y el máximo, e hilos (ScreenScraper);
>   - **activar o desactivar** el servicio sin borrar la clave, y **«Olvidar
>     credenciales»**, que la borra del llavero con confirmación dentro de la
>     propia página;
>   - una nota de privacidad: qué se envía a ese servicio (nombre, hash,
>     title ID…) y qué nunca;
>   - enlace a su entrada en Agradecimientos y a su posición en el orden de
>     fuentes.
> - **Enlaces cruzados** en los dos sentidos: desde Fuentes, «Configurar en
>   Integraciones»; desde el relleno, Organizar, Duplicados, la ficha del
>   juego y la CLI, un aviso que enlaza a la tarjeta («SteamGridDB sin clave:
>   configurar»); desde Integraciones, a Red, Fuentes y Agradecimientos;
>   cada tarjeta con su enlace profundo, como `SettingsLink`.
> - **El buscador de Ajustes** encuentra cada integración por su nombre y
>   por sus campos («api key», «contraseña», «token»).
> - **Diseño** con frontend-design y los tokens de la app; ver antes
>   Playnite, LaunchBox, RomM, Raycast, Linear, VS Code (Cuentas) y GitHub
>   (aplicaciones conectadas); estado vacío útil; sin logos oficiales si sus
>   normas no lo permiten.
>
> **Detrás de la pantalla**: un registro único de integraciones (id, tipo,
> campos, prueba, enlaces de ayuda, id de crédito, funciones y enlaces a
> ajustes) del que salen la pestaña, el buscador, los avisos y la CLI, con
> un test que falla si falta un crédito o un proveedor con credenciales;
> ampliar `CredentialKey` sin romper las guardadas; mensajes del llavero
> por sistema operativo (bloqueado, no disponible, permiso denegado);
> `coverly integrations list|test|set|forget` con el secreto por stdin o
> variable de entorno; exportar/importar y registros sin secretos;
> migración automática; en Fuentes queda solo el orden y «Configurar».

The table of services (SteamGridDB, ScreenScraper, Catálogo de Switch,
IGDB, RetroAchievements, TheGamesDB, RAWG, Microsoft Store / GameTDB /
Wikidata, Actualizaciones) is in `TODO.md` §8 and drives §5.

Workflow scope on top of the brief: only **working** integrations get a
card; planned ones are a compact «Próximamente» list with no inputs and no
new keychain keys (YAGNI), and the registry must make adding them trivial.

Ask → section map: tab and cards → §5, §6; status → D6, §6.3; fields →
D8; test → D9; help → D10; quota → D11; toggle and forget → D12, D13;
privacy → §7.4; links → D14, §8.3; search → D15; registry → D2–D4, §4;
`CredentialKey` → D5; keychain messages → D16; CLI → D18–D20; export and
logs → D17; migration → D21; Fuentes → D7.

## 3. Research: how others present integrations

Read from the products' current documentation and UI as known on
2026-11-02 (no screenshots copied, nothing reused verbatim).

| Product | Pattern | What we take | What we avoid |
|---|---|---|---|
| Playnite (Add-ons › Libraries / Metadata) | One row per add-on, its own settings view behind it; auth state as a line of text («Authenticated» / «Not authenticated») with a button | Status as words, not only a colour dot; per-service help | Each add-on invents its own layout |
| LaunchBox (Tools › Options › Metadata / Media) | Flat checkbox lists; accounts (EmuMovies, ScreenScraper) in separate dialogs | Nothing to copy beyond «one place for accounts» | Accounts scattered across dialogs |
| RomM (Settings › Metadata providers) | One block per provider with «enabled / not configured» and a pointer to the env var to set | Planned providers listed but greyed; the key's origin named | Config only through env vars |
| Raycast (Extensions › preferences) | Left list, right detail; required fields marked; password fields masked with no value echoed back | Masked fields that never refill; required vs optional | Two-pane master/detail is too heavy for four services |
| Linear (Settings › Integrations) | Directory of cards with «Connected» badges; each opens a page | A short «what it does» line per service | Marketplace grid and logos |
| VS Code (Accounts menu) | Signed-in accounts listed with «Manage trusted extensions» and «Sign out» | «Forget» as a plain, final action per account | Hidden in a menu |
| GitHub (Settings › Applications) | Connected apps with last used date, permissions granted and «Revoke» with confirm | A privacy line per service; revoke with in-page confirm; «last checked» time | Organisation-level complexity |

Conclusion: a single column of expandable service rows (Playnite/VS Code
density) whose open state shows fields, test, help and privacy (Raycast
detail without the second pane), with GitHub's «what this can see» and
in-page revoke.

## 4. Decisions

| # | Question | Decision | Why |
|---|----------|----------|-----|
| D1 | Tab id, label, place | `integrations`, «Integraciones» / "Integrations". In `SETTINGS_SECTIONS` right after `sources`, group `covers` (Portadas). | Three of the four cards are data sources; next to Fuentes keeps the two halves (order vs accounts) adjacent. Red follows in Aplicación, one tab away. |
| D2 | Where the registry lives | One data file, `integrations/integrations.json`, compiled into Rust (`include_str!`, like `credits/credits.json`) and imported by TS (`src/lib/integrations.ts`). Rust parses and validates it in `coverly_core::integrations`. | One source for tab, search, notices and CLI without code generation; the same pattern credits already uses and tests. URLs in JSON keep the D30 «no URL literal in `.rs`» test green. |
| D3 | Registry shape | Per entry: `id`, `kind` (`api-key` \| `account` \| `dataset` \| `link`), `group` (`art` \| `names` \| `achievements` \| `catalogs` \| `updates`), `status` (`working` \| `planned`), `fields` (`[{ key: CredentialKey, input: 'user' \| 'password' \| 'api-key' \| 'client-id' \| 'secret' }]`), `test` (`steamgriddb` \| `screenscraper` \| null), `help_url` (https, official host), `credit` (credit id), `source` (a `KNOWN_SOURCES` id or null), `setting` (a settings path that enables it or null), `powers` (feature ids: `fill`, `organize-identify`, `duplicates-names`, `organize-names`, `updates`), `roadmap` (TODO anchor, planned only), `needs` (planned only: the input kinds it will ask for, display text only, never inputs). User-visible text (name aside) lives in `locales/*/ui.json` under `integrations.service.<id>.*`, so es/en parity stays tested. | Every field the brief lists; text in locales like the rest of the chrome, not in the data file (credits keep their `{es,en}` because the README reads them; integrations have no second reader). |
| D4 | Registry tests | Rust (`integrations::problems`): every entry's `credit` exists in `credits.json`; every `KNOWN_SOURCES` id that needs a credential (from `sources_info`'s `needs`, plus ScreenScraper's account) is the `source` of a working entry; every `CredentialKey` belongs to exactly one working entry; planned entries have no `fields` and no `test`; `help_url` passes the credits `url_problem` (https, no query, not on the denylist) and its host equals the host of its credit's `url`; ids are unique kebab-case. TS: every working entry has its locale keys; every entry appears in the search index. | The brief's test, plus the guards that make «official page only» mechanical rather than a review item. |
| D5 | `CredentialKey` | Unchanged: `steamgriddb-api-key`, `screenscraper-user`, `screenscraper-password`. No keys for planned services. A test pins the three account names and `SERVICE = com.maecly.coverlyroms`. | YAGNI (workflow scope). Pinning the names is the migration guarantee (D21). |
| D6 | Status model | Computed in Rust (`integrations::state`, pure, tested), one of: `off` (disabled), `unavailable` (this build cannot use it: ScreenScraper without developer credentials), `not-configured` (a required field is not stored), `saved` (stored, not checked this session), `connected` (last check or last job this session succeeded), `error` (last check or job said `invalid` / `offline` / `unavailable`; carries the reason), `quota` (daily quota used up). Precedence: `off` > `unavailable` > `not-configured` > `quota` > `error` > `connected` > `saved`. | The brief's five states plus two honest ones: «saved, not checked» (we do not test on tab open, D9) and «not in this build». |
| D7 | What stays in Fuentes | The order list and each source's on/off switch. SteamGridDB and ScreenScraper rows gain «Configurar en Integraciones» (a link that focuses their card) and their «needs a key» hint becomes «Sin clave» / «Sin cuenta». The Switch catalogue group and the Cuentas block move out entirely. | The owner: «En Fuentes queda solo el orden y el enlace «Configurar»». The on/off switch of a source is part of the order (it decides whether the fill asks it). |
| D8 | Secret fields | Masked (`type=password`), «Mostrar» toggles only what the user typed this time; empty after save, error or forget; a stored value shows as «Guardada» with a check icon and never refills. Saving an empty field does nothing (the button is disabled); clearing is «Olvidar credenciales». The user name of ScreenScraper is treated as a secret too. | Existing `CredentialRow` behaviour, kept. The user name is enough to target an account; same handling is simpler than two rules. |
| D9 | «Probar conexión» | Explicit only; never on tab open or on focus. Uses the existing `test_source_key` (one request, 10 s cap, no retries). Outcomes → copy: `ok` «Funciona», `invalid` «Credenciales incorrectas», `missing` «Falta la clave» / «Falta la cuenta», `offline` «Sin red», `rate-limited` «Cuota agotada o demasiadas peticiones», `unavailable` «El servicio no responde». The result sets the card state for the session (D6). | Opening Settings must not send anything to anyone. The six `KeyCheck` values already cover the brief's four outcomes plus two. |
| D10 | «Cómo conseguir la clave» | A link to the service's own page plus one line of steps. SteamGridDB: `https://www.steamgriddb.com/profile/preferences/api` — «Inicia sesión con Steam, abre Preferencias › API y genera una clave.» ScreenScraper: `https://www.screenscraper.fr/membreinscription.php` — «Crea una cuenta gratuita y usa aquí tu usuario y contraseña.» Opened with the existing external-link command (the URL comes from the registry by id, never from the webview). Both URLs are re-checked by hand when implementing; if one has moved, the service's home page from `credits.json` is used and the change noted in the PR. | Official pages only (D4 test). Opening by id keeps the webview unable to open arbitrary URLs. |
| D11 | ScreenScraper quota | `check_account` returns the `ssuser` numbers it already parses: `requeststoday`, `maxrequestsperday`, `maxthreads` (as `SsQuota`). Runs update the same numbers through `note_user`. The app keeps the latest `SsQuota` and the time it was seen in memory (`AppState`), never on disk. The card shows «Hoy: 1 234 de 20 000 peticiones» and «Hilos: 1» in mono, plus «visto hace 5 min». No numbers until a check or a run has produced them. | Real numbers from the service, never estimated. Memory only: no new state file about accounts. |
| D12 | Enable / disable | One switch per card bound to the existing setting: `sources.order[id].enabled` for SteamGridDB and ScreenScraper, `catalogs.switch.enabled` for the catalogue. Turning a service off never touches the keychain. For the catalogue, turning it off still deletes the downloaded file (existing behaviour, adjustments §9.2), and its hint says so: «Al desactivarlo se borra el catálogo descargado; se puede volver a descargar.» | No second flag to drift from Fuentes. The catalogue's delete-on-off is an existing owner decision. |
| D13 | «Olvidar credenciales» | Button on cards with stored fields. Click turns the card's action row into an inline confirm: «¿Olvidar las credenciales de ScreenScraper? Se borran del llavero de este equipo.» with «Olvidar» (danger) and «Cancelar»; focus moves to «Cancelar»; Escape cancels. Clears every field of that integration (`clear_credential` per key). No `window.confirm`, no dialog. | The brief asks for in-page confirmation. Forgetting an account forgets the whole account (user + password), never half. |
| D14 | Deep links | Each card has DOM id `integration-<id>`. `openSettings('integrations', { focus: 'integration-<id>' })` opens the tab, expands that card, scrolls to it and focuses its heading button. Notices use a small `IntegrationLink` (wraps that call; same Back behaviour as `SettingsLink`). | The existing focus mechanism (settings-ui D10/D11) does the rest. |
| D15 | Search | `SETTINGS_INDEX` gains one entry per working integration (`id: integration-<id>`, section `integrations`, keywords from `integrations.service.<id>.keywords`: e.g. «api key, clave de api, token, steam» / «usuario, contraseña, cuenta, password, login»), the moved Switch catalogue settings (new ids `setting-integrations-switch-catalog*`), and one entry for «Próximamente» whose keywords list the planned names (so «igdb» finds the list and its roadmap link). | Search by service and by field name, as asked. Planned names are findable without pretending they work. |
| D16 | Keychain errors per OS | `CredentialError` gains `Denied`. `classify` maps: `NoDefaultStore` → `Unavailable`; `NoStorageAccess` → `Locked`; `PlatformFailure` whose text says user cancel / auth failed / access denied (macOS `errSecUserCanceled`, `errSecAuthFailed`; Windows `ERROR_ACCESS_DENIED`) → `Denied`. `IpcError` gets `keychain-denied`. The UI picks copy by `errors.keychain-<kind>.<os>` (os from `appInfo.platform`: `macos`, `windows`, `linux`), falling back to the generic key. Copy in §7.3. | Each message says what to do on that machine. The detail text of `Failed` stays as it is (already secret-free). |
| D17 | Export / import, logs, crash reports | There is no settings export/import today, and secrets are not in `settings.json`. A Rust test serialises a fully populated `Settings` and fails if any `CredentialKey` account name or secret-looking field appears; the same test will guard a future export because it serialises the same type. Logs: `credential set/cleared: <account>` stays; new lines only say `set` / `unset` / the `KeyCheck`. The React `ErrorBoundary` report and the Rust panic hook never include the credential status values beyond booleans (they do not today; a test asserts the boundary's report text for a store holding a typed value). | Nothing to build for export; a guard so nobody adds secrets later. |
| D18 | Can the CLI use the keychain? | Yes. `KeychainStore` moves from `src-tauri/src/keychain.rs` to `coverly_core::credentials::keychain` behind a `keychain` cargo feature (on in `src-tauri` and `coverly-cli`), same `SERVICE` and account names, so app and CLI share entries. `COVERLY_NO_KEYCHAIN=1` turns it off (tests set it). | `integrations set` is pointless without a store; one implementation keeps names identical. |
| D19 | CLI key precedence | For `fill`, `organize --online` and `integrations test`: environment variable first (`COVERLY_SGDB_KEY`, `COVERLY_SS_USER`, `COVERLY_SS_PASSWORD`, unchanged), then the keychain. A keychain that fails or is off counts as empty with one stderr note. | Scripts and CI keep working as before; desktop users get their saved keys. Owner decision listed in §12 (macOS asks once to let `coverly` read an item the app created). |
| D20 | CLI shape | `coverly integrations list [--json]`; `test <id> [--json]`; `set <id> [--field <user\|password\|api-key>] [--from-env <VAR>]`, reading the value from stdin (no echo when stdin is a TTY; one line, trimmed) or from the named variable; `forget <id> [--yes]` (asks «y/N» on a TTY without `--yes`; refuses on a non-TTY without `--yes`). No command takes a secret as an argument. Outputs say `set` / `not set`, never values. Exit codes: 0 success; 1 test not ok, keychain error or unknown id (message on stderr); 2 usage. | Matches the brief; the codes follow the existing 0/1/2 meaning in `main.rs`. |
| D21 | Migration | None needed: same keychain service and account names (D5). On first open the tab reads `credential_status` and the stored keys show as «Guardada». A test stores keys in a `MemoryStore` under the three names and checks the integrations view reports them. | The brief's «aparecen solas». |
| D22 | ScreenScraper without developer credentials | Card state `unavailable` with «Esta versión no incluye acceso a ScreenScraper. Las versiones oficiales sí.» Fields stay editable (saved for a build that has it); «Probar conexión» is disabled with that reason. | The user can prepare the account; nothing pretends to work. |
| D23 | Switch catalogue card | No credentials and no «Probar conexión»: its status line is the existing `SwitchCatalogStatusLine`, its actions «Descargar» / «Buscar actualización» / «Borrar catálogo», the icons switch and, folded, the nlib fallback switch and URL. State mapping: `off` → off; `ready` → connected; `failed-*` / `rejected` / `remove-failed` → error with the existing reason; `pending` / `downloading` → saved with progress. | A download is its own test; a second button would do the same thing. |
| D24 | Actualizaciones card | A `link` entry: one line («Comprueba si hay una versión nueva de Coverly ROMs en GitHub»), the current channel as text, «Cambiar en General» (focuses `setting-general-update-channel`) and the credit link. No toggle here. | The owner: «Enlazado desde aquí, sin moverlo». |
| D25 | Logos | None. Each card uses an app icon by kind (lucide `KeyRound` for api-key, `UserRound` for account, `Database` for dataset, `RefreshCw` for updates) and the service name in text. | SteamGridDB and ScreenScraper publish no logo licence for third-party apps; names and our own icons are always allowed. |
| D26 | Which cards start open | Collapsed by default except: a card in `error` or `quota`; the deep-link target; every card when none is configured (empty state). Open state lives in `navStore.openFolds` (session only, the existing mechanism). | Problems are never hidden; a calm tab otherwise. |
| D27 | Notices on other screens | One component, `IntegrationNotice`, takes `{ integration, problem }` with `problem` ∈ `missing` \| `rejected` \| `quota` \| `off` and renders «SteamGridDB sin clave», «ScreenScraper rechazó la cuenta», «Cuota de ScreenScraper agotada por hoy», «Catálogo de Switch desactivado» with «Configurar» linking the card. Back end: gather records structured `SourceTrouble { source, kind }` beside its existing warning strings (strings unchanged); fill and organise plans carry `integration_notices` built from them via the registry (`source` → integration id). | Text stays one place; screens never parse warning strings. |
| D28 | Where notices appear | Fill wizard: before starting, when SteamGridDB is enabled without a key (replaces today's silent «no vote»); fill summary: from `integration_notices`. Organise: online identification on with ScreenScraper `not-configured` / `unavailable` / `quota`; plan `catalogue-missing` for Switch links the catalogue card. Duplicados: a Switch group present while the catalogue is off («Los nombres salen del archivo; activa el catálogo de Switch para usar los oficiales»). Game sheet: out of scope (no game sheet exists yet; the component is ready for it). | Where the missing service changes the result the user is looking at. |
| D29 | Status after jobs | When a fill or organise job ends, its `SourceTrouble`s update the in-memory integration health (D6): `MissingKey` → no change (state already says it), `Auth` → `error/invalid`, `Exhausted` → `quota`, success with a source → `connected`. | The card tells the truth about the last real use, not only the last button press. |

## 5. Content of the tab

### 5.1 Order on the page

1. Intro line and, when nothing is configured, the empty-state note (§6.6).
2. **Portadas y arte**: SteamGridDB, ScreenScraper.
3. **Catálogos**: Catálogo de Switch.
4. **Actualizaciones**: Actualizaciones de Coverly (link card).
5. **Próximamente**: one compact list.
6. Footer links: «Proxy y límites en Red», «Orden de fuentes en Fuentes»,
   «Créditos en Agradecimientos».

Groups with no working service («Nombres y metadatos», «Logros») are not
rendered; their planned services sit in Próximamente with a group label.

### 5.2 Per card

| Card | Fields | Test | Help | Toggle | Unlocks | Credit | Fuentes link |
|---|---|---|---|---|---|---|---|
| SteamGridDB | Clave de API | yes | steamgriddb.com API page | `sources.order.steamgriddb` | Portadas hechas por la comunidad | `steamgriddb` | position N of M |
| ScreenScraper | Usuario, Contraseña | yes (dev build) | screenscraper.fr sign-up | `sources.order.screenscraper` | Medios e identificación por hash en Organizar | `screenscraper` | position N of M |
| Catálogo de Switch | — (fallback URL, folded) | — (D23) | — | `catalogs.switch.enabled` | Nombres e iconos de Switch | `titledb` (+ `nlib`, `nintendo-eshop-icons`) | — |
| Actualizaciones | — | — | — | — | Avisos de versión nueva | (GitHub releases link) | — |

### 5.3 Próximamente

IGDB (client ID y secret de Twitch), RetroAchievements (usuario y clave de
la API web), TheGamesDB (clave de API), RAWG (clave de API), Microsoft
Store, GameTDB, Wikidata (sin cuenta). Each row: name, what it will need,
what it is for, in one line; the list ends with «Ver el plan» opening the
repository's `TODO.md` roadmap through the existing `open_link('repo')`
path. No inputs, no toggles, no keychain keys.

## 6. UX and UI

### 6.1 Design plan (frontend-design)

Subject: a desktop tool for people who curate ROM libraries; this tab is
the «keys drawer» of an archivist's desk. Brand «Buckram & Manila»: book
cloth, manila folders, the Slot (dashed = not yet, solid = done).

- **Colour** (existing tokens, no new ones): `--cr-surface-raised`
  `#ffffff` for an open card, `--cr-surface` `#fbfbf9` closed, `--cr-border`
  `#d3d7cf`, `--cr-primary` (Buckram) `#0e5e6f` for actions and the
  connected mark, `--cr-manila` `#e6c171` only for the «Guardada» tag, and
  status from `--cr-success` `#2a6f38`, `--cr-warning` `#9f4b0c`,
  `--cr-danger` `#b3261e`. Dark mode through the same roles.
- **Type**: Archivo (`font-semibold`, `text-dense`) for service names and
  group titles, matching `sectionTitle`; Atkinson Hyperlegible Next for
  body and hints; Atkinson Hyperlegible Mono only for real data — quota
  numbers, the fallback URL, typed keys.
- **Layout**: one left-aligned column, max 44rem, the existing
  `settings-group` rhythm. Each service is a row, not a tile:

```
Portadas y arte
┌─────────────────────────────────────────────────────────────┐
│ [key] SteamGridDB                     ┆Sin configurar┆   ▸  │  closed
│       Portadas hechas por la comunidad                       │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ [user] ScreenScraper                  ▌Conectado▐        ▾  │  open
│        Medios e identificación por hash en Organizar        │
│ ─────────────────────────────────────────────────────────── │
│  Activado  [■■ on]          Fuentes: posición 3 de 4        │
│  Usuario     [••••••••     ] (o)   Guardado ✓                │
│  Contraseña  [••••••••     ] (o)   Guardada ✓                │
│  [Guardar]  [Probar conexión]  [Olvidar credenciales]       │
│  ✓ Funciona · comprobado hace 2 min                          │
│  Hoy: 1 234 de 20 000 peticiones   Hilos: 1                  │
│  Cómo conseguir la cuenta: crea una cuenta gratuita…  ↗     │
│  Qué se envía: nombre del archivo, tamaño, CRC/MD5/SHA-1…   │
│  Crédito en Agradecimientos                                  │
└─────────────────────────────────────────────────────────────┘
Próximamente
  IGDB — client ID y secret de Twitch — arte y fichas
  RetroAchievements — usuario y clave de la API web — logros
  …                                              Ver el plan ↗
```

  (`┆…┆` dashed outline, `▌…▐` solid: §6.2. The «·» in the test line is a
  separate element in the build, not a joined string.)

- **Principles**: the status mark is the one memorable thing; everything
  else is quiet text and existing primitives (`Button`, `Input`, `Switch`,
  `Alert`, `Badge`, `Collapsible`). Words before colour. No card shadows,
  no gradients, no logos.
- **Motion**: only in answer to the user — the card body opens with the
  existing `Collapsible` height transition (150 ms, none under
  `prefers-reduced-motion`); the status mark swaps without animation; the
  test result is announced (`role="status"`), not animated.

### 6.2 Review against generic defaults

- First draft was a 2-column grid of equal rounded cards with a coloured
  left border per status. That is the SaaS-card kit (identical tiles, one
  radius, status stripe). Replaced with a single column of rows that only
  grow when opened, so four services read as a short list, not a
  dashboard.
- First draft used a green/grey dot for status. Replaced with the brand's
  own Slot language on a `Badge`-sized tag: **dashed outline** for
  «Sin configurar» / «Desactivado» / «No disponible» (not yet), **solid
  Buckram** for «Conectado», **solid Manila** for «Guardada» (stored, yours
  to undo), **solid warning/danger** for «Cuota agotada» / «Error». Each
  tag always carries its word and an icon; colour is never alone.
- Rejected: all-caps group eyebrows, monospace for labels, «→» appended to
  links. Group titles use the existing `sectionTitle`; links use the
  existing `ExternalLink` icon only for links that leave the app.
- Radius follows hierarchy: rows `radius.lg` (8), tags `radius.sm` (3),
  inputs and buttons their primitive defaults.

### 6.3 Status tags

| State | Tag (es / en) | Style | Extra line |
|---|---|---|---|
| off | Desactivado / Off | dashed, text-secondary | «Actívalo para usarlo en el relleno.» |
| unavailable | No disponible / Not available | dashed, text-secondary | D22 copy |
| not-configured | Sin configurar / Not set up | dashed, warning text | «Falta la clave de API.» / «Falta la cuenta.» |
| saved | Guardada / Saved | solid manila | «Sin comprobar en esta sesión.» |
| connected | Conectado / Connected | solid primary | «Comprobado hace {time}.» |
| error | Error / Error | solid danger-bg | reason + fix (§7.2) |
| quota | Cuota agotada / Quota used up | solid warning-bg | «Has usado {used} de {max} peticiones hoy. Vuelve mañana o deja que otra fuente siga.» |

### 6.4 Keyboard and screen readers

Each card header is a `button` (`aria-expanded`, `aria-controls`) whose
accessible name is «SteamGridDB, Sin configurar». Tab order inside an open
card: toggle, fields with their «Mostrar», actions, links. The forget
confirm traps nothing: it replaces the action row, focus moves to
«Cancelar», Escape restores the row and focus to «Olvidar credenciales».
Test results and saves are announced through one polite live region per
card.

### 6.5 Narrow windows

At ≤ 40rem the field row stacks (label above input), the status tag drops
under the name, and the quota line wraps; no horizontal scroll.

### 6.6 Empty state

When no working credential is stored and the catalogue is off, the intro is
followed by: «Para empezar, consigue una clave gratuita de SteamGridDB:
añade portadas a juegos que libretro no tiene. ScreenScraper, con cuenta,
identifica ROMs por su hash.» with a button «Configurar SteamGridDB» that
opens and focuses that card. Order chosen because SteamGridDB is
self-service in a minute; ScreenScraper needs an account and a build with
developer credentials.

## 7. Copy (es / en)

Spanish: sentence case, no exclamation marks, exact counts, numbers in
mono. Keys under `settings.section.integrations` and `integrations.*`.

### 7.1 Chrome

| Key | es | en |
|---|---|---|
| `settings.section.integrations` | Integraciones | Integrations |
| `integrations.intro` | Cuentas, claves y servicios que Coverly ROMs usa en tu nombre. Las claves se guardan en el llavero del sistema y nunca se vuelven a mostrar. | Accounts, keys and services Coverly ROMs uses on your behalf. Keys are kept in the system keychain and are never shown again. |
| `integrations.group.art` | Portadas y arte | Covers and art |
| `integrations.group.names` | Nombres y metadatos | Names and metadata |
| `integrations.group.achievements` | Logros | Achievements |
| `integrations.group.catalogs` | Catálogos | Catalogues |
| `integrations.group.updates` | Actualizaciones | Updates |
| `integrations.group.planned` | Próximamente | Coming later |
| `integrations.enabled` | Activado | On |
| `integrations.show` | Mostrar {what} | Show {what} |
| `integrations.save` | Guardar | Save |
| `integrations.saved` | Guardada en el llavero | Saved in the keychain |
| `integrations.test` | Probar conexión | Test connection |
| `integrations.testing` | Probando… | Testing… |
| `integrations.forget` | Olvidar credenciales | Forget credentials |
| `integrations.forgetConfirm` | ¿Olvidar las credenciales de {name}? Se borran del llavero de este equipo. | Forget the {name} credentials? They are deleted from this computer's keychain. |
| `integrations.forgetYes` | Olvidar | Forget |
| `integrations.cancel` | Cancelar | Cancel |
| `integrations.howTo` | Cómo conseguir la clave | How to get the key |
| `integrations.howToAccount` | Cómo conseguir la cuenta | How to get an account |
| `integrations.privacy` | Qué se envía | What is sent |
| `integrations.credit` | Crédito en Agradecimientos | Credit in Acknowledgements |
| `integrations.order` | Fuentes: posición {n} de {total} | Sources: position {n} of {total} |
| `integrations.configure` | Configurar | Set up |
| `integrations.configureIn` | Configurar en Integraciones | Set up in Integrations |
| `integrations.quotaToday` | Hoy: {used} de {max} peticiones | Today: {used} of {max} requests |
| `integrations.threads` | Hilos: {n} | Threads: {n} |
| `integrations.seen` | visto {time} | seen {time} |
| `integrations.links.network` | Proxy y límites en Red | Proxy and limits in Network |
| `integrations.links.sources` | Orden de fuentes en Fuentes | Source order in Sources |
| `integrations.links.credits` | Créditos en Agradecimientos | Credits in Acknowledgements |
| `integrations.planned.plan` | Ver el plan | See the plan |
| `integrations.empty` | Para empezar, consigue una clave gratuita de SteamGridDB: añade portadas a juegos que libretro no tiene. ScreenScraper, con cuenta, identifica ROMs por su hash. | To start, get a free SteamGridDB key: it adds covers for games libretro does not have. ScreenScraper, with an account, identifies ROMs by their hash. |

### 7.2 Test outcomes and errors

| `KeyCheck` | es | en |
|---|---|---|
| ok | Funciona. | It works. |
| invalid | Credenciales incorrectas. Revisa la clave o vuelve a generarla en la web del servicio. | Wrong credentials. Check the key or create a new one on the service's site. |
| missing | Falta la clave. Guárdala arriba y vuelve a probar. | No key yet. Save it above and test again. |
| offline | Sin red. Comprueba la conexión o el proxy en Red. | No network. Check the connection or the proxy in Network. |
| rate-limited | Cuota agotada o demasiadas peticiones. Prueba más tarde. | Quota used up or too many requests. Try again later. |
| unavailable | El servicio no responde. Puede estar caído; prueba más tarde. | The service is not answering. It may be down; try again later. |

### 7.3 Keychain messages per OS

| Key | es | en |
|---|---|---|
| `errors.keychain-locked.macos` | El llavero de macOS está bloqueado. Desbloquéalo en Acceso a Llaveros y vuelve a intentarlo. | The macOS keychain is locked. Unlock it in Keychain Access and try again. |
| `errors.keychain-denied.macos` | macOS no dio permiso para usar el llavero. Vuelve a intentarlo y elige «Permitir» en el aviso. | macOS did not allow access to the keychain. Try again and choose "Allow" in the prompt. |
| `errors.keychain-unavailable.macos` | No se encuentra el llavero de inicio de sesión. Revísalo en Acceso a Llaveros. | The login keychain cannot be found. Check it in Keychain Access. |
| `errors.keychain-locked.windows` | El Administrador de credenciales de Windows no está disponible ahora. Cierra sesión, vuelve a entrar e inténtalo de nuevo. | Windows Credential Manager is not available right now. Sign out, sign back in and try again. |
| `errors.keychain-denied.windows` | Windows denegó el acceso al Administrador de credenciales. Comprueba que tu cuenta no tiene restricciones de directiva. | Windows denied access to Credential Manager. Check that your account has no policy restrictions. |
| `errors.keychain-unavailable.windows` | El Administrador de credenciales de Windows no responde. Reinicia el equipo y vuelve a intentarlo. | Windows Credential Manager is not responding. Restart the computer and try again. |
| `errors.keychain-locked.linux` | El depósito de claves está bloqueado. Desbloquéalo en Contraseñas y claves o en KWalletManager y vuelve a intentarlo. | The keyring is locked. Unlock it in Passwords and Keys or KWalletManager and try again. |
| `errors.keychain-denied.linux` | El servicio de secretos denegó el acceso. Acepta el aviso de desbloqueo; en Flatpak, permite `org.freedesktop.secrets`. | The secret service denied access. Accept the unlock prompt; on Flatpak, allow `org.freedesktop.secrets`. |
| `errors.keychain-unavailable.linux` | No hay un servicio de secretos en marcha. Instala o inicia GNOME Keyring o KWallet y vuelve a abrir Coverly ROMs. | No secret service is running. Install or start GNOME Keyring or KWallet and reopen Coverly ROMs. |

The existing `errors.keychain-*` keys stay as the fallback for an unknown
platform.

### 7.4 Per service (`integrations.service.<id>.*`)

| Key | es | en |
|---|---|---|
| steamgriddb.use | Portadas hechas por la comunidad para juegos que libretro no tiene. | Community-made covers for games libretro does not have. |
| steamgriddb.unlocks | Portadas verticales; más adelante, anchas, fondos, logos e iconos. | Vertical covers; later, wide covers, backgrounds, logos and icons. |
| steamgriddb.steps | Inicia sesión con Steam, abre Preferencias › API y genera una clave. | Sign in with Steam, open Preferences › API and generate a key. |
| steamgriddb.privacy | Se envía el nombre del juego que buscas y tu clave. Nunca rutas, hashes ni el resto de tu biblioteca. | The name of the game being looked up and your key are sent. Never paths, hashes or the rest of your library. |
| steamgriddb.keywords | api key, clave de api, clave, token, steam | api key, key, token, steam |
| screenscraper.use | Medios e identificación por hash en Organizar, con tu cuenta. | Media and hash identification in Organise, with your account. |
| screenscraper.unlocks | Capturas, pantallas de título e identificación exacta de ROMs. | Screenshots, title screens and exact ROM identification. |
| screenscraper.steps | Crea una cuenta gratuita y usa aquí tu usuario y contraseña. | Create a free account and use its user name and password here. |
| screenscraper.privacy | Se envían el nombre del archivo, su tamaño, sus hashes CRC, MD5 y SHA-1 y el sistema, con tu usuario y contraseña. Nunca la ruta completa ni el contenido de los archivos. | The file name, its size, its CRC, MD5 and SHA-1 hashes and the system are sent, with your user name and password. Never the full path or the file contents. |
| screenscraper.keywords | usuario, contraseña, cuenta, password, login, cuota, hilos | user, password, account, login, quota, threads |
| switch-catalog.use | Nombres, editora y fecha de los juegos de Switch por title ID. | Switch game names, publisher and date by title ID. |
| switch-catalog.unlocks | Nombres de Switch e iconos de la eShop. | Switch names and eShop icons. |
| switch-catalog.privacy | Se descarga un archivo público; no se envía nada de tu biblioteca. El respaldo en línea, si lo activas, recibe el title ID que falta. | A public file is downloaded; nothing from your library is sent. The online fallback, if you turn it on, receives the missing title ID. |
| switch-catalog.keywords | titledb, nlib, title id, eshop, iconos | titledb, nlib, title id, eshop, icons |
| updates.use | Comprueba si hay una versión nueva de Coverly ROMs en GitHub. | Checks GitHub for a new version of Coverly ROMs. |
| updates.privacy | Se envía la versión instalada y el sistema operativo, como cualquier descarga. | The installed version and operating system are sent, like any download. |

Notices (§D27): `integrations.notice.missing` «{name} sin clave» /
"{name} has no key"; `.missingAccount` «{name} sin cuenta» / "{name} has
no account"; `.rejected` «{name} rechazó las credenciales» / "{name}
rejected the credentials"; `.quota` «Cuota de {name} agotada por hoy» /
"{name} quota used up for today"; `.off` «{name} desactivado» / "{name} is
off". Each followed by the «Configurar» link.

The privacy lines are checked against the request code
(`steamgriddb.rs` query, `screenscraper.rs` params) during implementation;
a mismatch is fixed in the copy, not in the code.

## 8. Architecture and data flow

### 8.1 Files

```
integrations/integrations.json         (new) the registry (D2, D3)
crates/coverly-core/src/integrations.rs (new) parse, problems(), state(),
                                         IntegrationView, SsQuota
crates/coverly-core/src/credentials.rs  + Denied; keychain module behind
                                         feature `keychain` (D18)
crates/coverly-core/src/credits.rs      url_problem → pub(crate)
crates/coverly-core/src/sources/
  screenscraper.rs                      check_account → AccountCheck
                                         { verdict, quota }; last quota
  mod.rs                                SourceTrouble
crates/coverly-core/src/gather/mod.rs   collect SourceTrouble beside warnings
crates/coverly-core/Cargo.toml          feature keychain = ["dep:keyring"]
crates/coverly-cli/src/integrations.rs  (new) list/test/set/forget
crates/coverly-cli/src/{main,fill,organize}.rs  subcommand; env→keychain
                                         keys; hints «coverly integrations
                                         set <id>»
src-tauri/src/keychain.rs               re-export from core (or removed)
src-tauri/src/integrations.rs (new)     integrations_view, health map
src-tauri/src/ipc.rs, lib.rs            integrations_view, test_integration
src-tauri/src/error.rs                  keychain-denied
src/lib/integrations.ts       (new)     typed loader of the JSON
src/ipc/{types,backend,tauriBackend}.ts IntegrationView, SsQuota,
                                         IntegrationNotice; new calls
src/ipc/mock/mockSettings.ts            integration states; ?mockIntegrations=
                                         empty|saved|error|quota|nodev
src/components/settings/
  IntegrationsSection.tsx     (new)     tab
  IntegrationCard.tsx         (new)     header, body, forget confirm
  SecretField.tsx             (new)     extracted from CredentialRow
  SwitchCatalogGroup.tsx      (new)     moved out of SourcesSection
  SourcesSection.tsx                    order only + Configurar links
  registry.ts                           integrations entries, fold rename
  IntegrationLink.tsx, IntegrationNotice.tsx (new)
src/stores/navStore.ts                  'integrations' section
src/screens/SettingsScreen.tsx          SECTION_COMPONENTS entry
src/components/fill/FillWizard.tsx      notice (D28)
src/components/organize/*, dedupe/*     notices (D28)
locales/{es,en}/ui.json                 §7 keys; old settings.sources.
                                         credential* keys removed
docs/04-architecture/settings.md, README.md, README.es.md, CHANGELOG.md
```

### 8.2 IPC

- `integrations_view() -> IntegrationView[]`: per working entry `{ id,
  enabled, built, fields: { key, set }[], state, reason?, checked_at?,
  quota? }`. Reads keychain presence (blocking pool), settings and the
  in-memory health. Never a value.
- `test_integration(id) -> { verdict: KeyCheck, quota?: SsQuota }`:
  wraps `test_source_key`, updates health.
- Existing `set_credential`, `clear_credential`, `credential_status`
  unchanged (the card calls them per field). `forget_integration(id)`
  clears every key of the entry atomically from the user's point of view
  (each key cleared; on a partial failure the view reports what is still
  set and the error).
- `open_integration_help(id)`: opens the registry `help_url` for `id`.

### 8.3 Flow

```
open tab ─▶ integrations_view ─▶ cards (no network)
save    ─▶ set_credential ─▶ forget cached SourceKeys ─▶ view refresh (saved)
test    ─▶ test_integration ─▶ KeyCheck (+quota) ─▶ health ─▶ card state
job end ─▶ SourceTrouble[] ─▶ health; plan.integration_notices ─▶ screens
notice  ─▶ IntegrationLink ─▶ openSettings('integrations',{focus}) ─▶ card
```

## 9. Edge cases and failure modes

- Keychain unavailable: tab still renders every card; fields disabled; one
  OS-specific alert at the top (D16); toggles still work (they are
  settings). CLI: `list` says «keychain: unavailable», `set` exits 1.
- Keychain prompt while saving (macOS): the call runs on the blocking pool;
  the Save button shows «Guardando…» and stays disabled until it answers.
- Stale test answer after a save or forget: dropped by the existing
  generation counter.
- Only one of ScreenScraper's two fields stored: state `not-configured`,
  extra line «Falta la contraseña» (or user); forget clears both.
- Value with control characters or > 512 chars: `BadValue` → «La clave no
  es válida: tiene caracteres de control o es demasiado larga.»
- SteamGridDB toggled on without a key: allowed (the fill skips it with a
  notice, as today); card shows `not-configured`.
- Quota numbers absent from the answer: no quota line; never zeros.
- Quota seen yesterday: shown with its age («visto ayer»); a quota state
  older than the service's day is not carried over — it expires after 24 h.
- Deep link to a planned id or unknown id: opens the tab without focus.
- Switch catalogue download running while the tab moves: the module-level
  `inflight` tracker moves with `SwitchCatalogGroup` unchanged.
- Search ids renamed (D15): no persisted references exist (`focusSetting`
  is in-memory); the registry completeness test catches stale ids.
- CLI `set` with stdin closed or empty: exit 1, «no value read from stdin».
  With `--from-env VAR` unset: exit 1 naming the variable, not a value.
- CLI on a non-TTY `forget` without `--yes`: refuses, exit 1.
- CLI run inside tests: `COVERLY_NO_KEYCHAIN=1` so CI never touches a real
  keychain.
- Windows paths and mtimes: not involved; CLI tests compare output text,
  no fixtures with paths.

## 10. Test plan

### Rust (`cargo test --workspace`)

- `integrations::problems` is empty for the shipped file; copies missing a
  credit, with a `CredentialKey` owned twice or not at all, a planned entry
  with fields, a help URL on another host or the denylist, each fail
  naming the entry.
- Coverage: every `KNOWN_SOURCES` id whose `sources_info` needs a key, and
  ScreenScraper, maps to a working entry.
- `state()` table test over every precedence pair of D6.
- `CredentialKey` account names and `SERVICE` pinned (D5, D21); a
  `MemoryStore` with the three keys yields `saved` for both services.
- `classify` maps cancel / auth-failed / access-denied platform failures
  to `Denied`; no secret bytes in any message (existing test extended).
- `check_account` parses `requeststoday`, `maxrequestsperday`,
  `maxthreads` into `SsQuota` from a fixture answer; absent numbers → none.
- Gather records `SourceTrouble` for `MissingKey`, `Auth`, `Exhausted`
  while its warning strings stay byte-identical.
- Settings serialisation contains no credential account name or secret
  (D17).
- CLI unit tests with a `MemoryStore`: `set` from a reader and from an env
  lookup, `forget`, `list` text and JSON never contain the value; `test`
  maps verdicts to exit codes. `assert_cmd` tests with
  `COVERLY_NO_KEYCHAIN=1`: `integrations list` succeeds and prints
  `keychain: off`; `set` exits 1; `integrations set steamgriddb sk-123`
  is a usage error (no positional secret).
- `no_url_literal_outside_the_registry` still passes.

### Vitest

- `IntegrationsSection`: renders one card per working entry and the
  Próximamente list with no inputs; each state tag word; stored values
  never appear in the DOM after save (assert the input is empty and no
  text node holds the value); Show toggles only typed text; forget
  confirm in-page (no `window.confirm` spy calls), Escape cancels, focus
  returns; test outcomes copy for all six `KeyCheck`s; quota line
  formatting with exact counts.
- Toggle writes the same setting path Fuentes writes.
- `SourcesSection`: no credential inputs; «Configurar en Integraciones» on
  SteamGridDB and ScreenScraper only; it opens the tab with the card
  focused.
- Registry / search: «api key», «contraseña», «token», «igdb»,
  «screenscraper» land on the right entries in es and en; completeness
  test covers the new ids.
- Keychain alert picks the OS-specific key and falls back.
- `IntegrationNotice` per problem; Fill wizard shows the SteamGridDB
  notice when enabled without a key.
- i18n parity test passes; mock backend contract test covers the new calls.

### E2E (Chrome DevTools MCP, isolated context `integrations`, port 1469)

Against `pnpm dev` with the mock backend:

1. `?mockIntegrations=empty`: empty-state note; «Configurar SteamGridDB»
   opens and focuses the card.
2. Type a key, Show, Save: tag «Guardada», field empty, value absent from
   the DOM (`evaluate_script` over `document.body.innerHTML`).
3. Probar conexión with mock verdicts `ok`, `invalid`, `offline`: copy and
   tag per outcome.
4. `?mockIntegrations=quota`: ScreenScraper open by default, quota line.
5. Forget: confirm row, Escape, then confirm; tag «Sin configurar».
6. Settings search «contraseña» → ScreenScraper card focused.
7. Fuentes → «Configurar en Integraciones» → card; Back returns.
8. Fill wizard notice → card → Back.
9. Keyboard only through one card; screenshot light and dark at 1280 and
   at 600 px wide.

Gates: `pnpm check-all`, `pnpm build`, `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 11. Order of work

1. Registry file, core module, tests (D2–D5).
2. Keychain move + `Denied` + per-OS copy (D16, D18).
3. ScreenScraper quota + health + IPC + mock (D11, D29, §8.2).
4. Tab, cards, Switch group move, Fuentes trim, search (§5, §6, D7, D15).
5. Notices and links (D27, D28).
6. CLI (D19, D20).
7. Docs, E2E, gates.

## 12. Owner decisions (implemented with a default, listed in the PR)

1. **CLI reads the keychain** (D18, D19). Default: yes, after the
   environment variables. On macOS the first read of an item the app
   created shows a system prompt for `coverly`. Alternative: CLI keeps
   env-only and `integrations set/forget` only manage the app's items.
2. **Official «how to get the key» pages** (D10) are the current SteamGridDB
   API preferences and ScreenScraper sign-up pages; if either moves, the
   card falls back to the credit's home page.
3. **Tab position** (D1): after Fuentes in Portadas. Alternative: under
   Aplicación before Red.
4. **Quota memory** (D11): memory only, cleared on restart. Alternative:
   persist the last numbers in the app data dir.

## 13. Self-review (2026-11-02)

- Placeholders: none; every copy row has es and en.
- Contradictions checked: the catalogue's «toggle without deleting» (brief)
  vs its delete-on-off (adjustments §9.2) — resolved in D12 by keeping the
  existing behaviour for downloaded data and saying so; credentials are
  never deleted by a toggle. «Probar conexión on every card» vs no test for
  the catalogue and updates — D23/D24 explain why. «No new keychain keys»
  vs «client ID and secret» — planned entries describe them in `needs`
  (display only); `fields` and `CredentialKey` stay unchanged (D3–D5).
- Scope: game sheet notices deferred (no game sheet yet, D28); settings
  export/import has nothing to change (D17).
