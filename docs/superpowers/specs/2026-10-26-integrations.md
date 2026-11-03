# Settings tab: Integraciones (accounts, keys and services)

Status: design spec, 2026-11-02. Branch `feat/integrations-tab`. Roadmap
item **8. Integraciones (cuentas, claves y servicios)** (`TODO.md`), the
brief Miguel approved on 2026-11-02. The file name keeps the 2026-10-26
date of the settings-tab spec series it belongs to. Adversarially
reviewed against `TODO.md` §8 and the code on 2026-11-02 (§14 lists what
the review changed and why).

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
| D1 | Tab id, label, place | `integrations`, «Integraciones» / "Integrations". In `SETTINGS_SECTIONS` right after `sources`, group `covers` (Portadas); `SETTINGS_GROUPS` is a `Record` over every section, so it gains the key too (`settingsNav.test.ts` covers it). Not in `STANDALONE`: its switches are settings, so while settings load or fail it shows the same state as the other tabs. | Three of the four cards are data sources; next to Fuentes keeps the two halves (order vs accounts) adjacent. Red follows in Aplicación, one tab away. |
| D2 | Where the registry lives | One data file, `integrations/integrations.json`, compiled into Rust (`include_str!("../../../integrations/integrations.json")`, like `credits/credits.json`) and imported by TS (`src/lib/integrations.ts`, `import raw from '../../integrations/integrations.json'`, as `src/lib/credits.ts` does). `tsconfig.json` `include` gains `"integrations"` (today `["src", "locales", "credits"]`, so the import would not typecheck) and Prettier's `format:check` covers the new folder. Rust parses and validates it in `coverly_core::integrations`. | One source for tab, search, notices and CLI without code generation; the same pattern credits already uses and tests. URLs in JSON keep the D30 «no URL literal in `.rs`» test (`credits.rs::no_url_literal_outside_the_registry`, exempting only `net/hosts.rs`, `prefs.rs` and test files) green. |
| D3 | Registry shape | Per entry: `id`, `kind` (`api-key` \| `account` \| `dataset` \| `link`), `group` (`art` \| `names` \| `achievements` \| `catalogs` \| `updates`), `status` (`working` \| `planned`), `fields` (`[{ key: CredentialKey, input: 'user' \| 'password' \| 'api-key' \| 'client-id' \| 'secret', required: true }]`), `test` (`steamgriddb` \| `screenscraper` \| null), `help_url` (https, official host; null for `dataset` and `link`), `credits` (credit ids, ≥ 1 for a working entry except the updater `link`, which has none: `net/hosts.rs` already registers `MAECLY_GITHUB` with `credit: None` — «Coverly's own releases, nothing to credit»; planned entries may list none until they are integrated, because item 7 adds their credit rows only then), `source` (a `KNOWN_SOURCES` id or null), `setting` (the settings path that enables it or null), `powers` (feature ids actually wired today: `fill` (SteamGridDB, ScreenScraper), `organize-identify` (ScreenScraper), `organize-names` (Switch catalogue), `updates`), `roadmap` (TODO anchor, planned only), `needs` (planned only: the input kinds it will ask for, display text only, never inputs). `sources_info` (`src-tauri/src/sources.rs`, today a hard-coded `match` that gives ScreenScraper `needs: []`) reads `needs` from the registry's `fields` instead, so the registry is the one table of «which source needs which key». User-visible text (name aside) lives in `locales/*/ui.json` under `integrations.service.<id>.*`, so es/en parity stays tested. | Every field the brief lists; text in locales like the rest of the chrome, not in the data file (credits keep their `{es,en}` because the README reads them; integrations have no second reader). No `duplicates-names`: nothing in `coverly_core::dedupe` reads the Switch catalogue or any account (checked: no `TitleNames`, `SourceKeys` or catalogue use there). |
| D4 | Registry tests | Rust (`integrations::problems`): every id in an entry's `credits` exists in `credits.json`; every working entry except the updater has ≥ 1 credit; every `net::hosts::ENDPOINTS` entry that carries credentials (a new `credentials: bool` on `Endpoint`, true for `STEAMGRIDDB` and `SCREENSCRAPER`) has its `credit` in some working entry's `credits` — this is the brief's «a network provider that needs credentials and is missing from the registry» made mechanical, independent of `sources_info` (which lives in `src-tauri` and cannot be called from a core test); every `CredentialKey` belongs to exactly one working entry; planned entries have no `fields` and no `test`; `help_url`, when present, passes the credits `url_problem` (made `pub(crate)`: https, no query or fragment, no `@`, not on the denylist) and its host, after stripping `www.`, equals the host of one of its credits' `url`; ids are unique kebab-case. TS: every working entry has its locale keys; every entry appears in the search index. | The brief's test, plus the guards that make «official page only» mechanical rather than a review item. |
| D5 | `CredentialKey` | Unchanged: `steamgriddb-api-key`, `screenscraper-user`, `screenscraper-password`. No keys for planned services. A test pins the three account names and `SERVICE = com.maecly.coverlyroms`. | YAGNI (workflow scope). Pinning the names is the migration guarantee (D21). |
| D6 | Status model | Computed in Rust (`integrations::state`, pure, tested), one of: `off` (disabled), `unavailable` (this build cannot use it: ScreenScraper without developer credentials), `not-configured` (a required field is not stored), `saved` (stored, not checked this session), `connected` (last check or last job this session succeeded), `error` (last check or job said `invalid` / `offline` / `unavailable`; carries the reason), `quota` (daily quota used up). Precedence: `off` > `unavailable` > `not-configured` > `quota` > `error` > `connected` > `saved`. A keychain that fails while reading presence gives `error` with the keychain reason (D16), never `not-configured`: «we could not look» is not «nothing stored». | The brief's five states plus two honest ones: «saved, not checked» (we do not test on tab open, D9) and «not in this build». |
| D7 | What stays in Fuentes | The order list and each source's on/off switch, with today's rule unchanged: a source's switch is disabled until it is usable (`usable = ready \|\| enabled` in `SourcesSection.tsx`: an enabled source can always be turned off). SteamGridDB and ScreenScraper rows gain «Configurar en Integraciones» (a link that focuses their card); the hint `settings.sources.needsKey` («Guarda una clave de API abajo para activarla» — «abajo» would point at nothing once the block moves) becomes «Sin clave» / «Sin cuenta» next to that link. The Switch catalogue group and the Cuentas block move out entirely; their locale keys (`settings.sources.credential*`, `settings.sources.switchCatalog.*`, `settings.sources.check.*`, `stored`, `notStored`, `save*`, `remove*`, `show`, `testKey*`, `testing`) move under `integrations.*` or are deleted, and the parity test keeps es/en aligned. | The owner: «En Fuentes queda solo el orden y el enlace «Configurar»». The on/off switch of a source is part of the order (it decides whether the fill asks it). |
| D8 | Secret fields | Masked (`type=password`), «Mostrar» toggles only what the user typed this time; empty after save, error or forget; a stored value shows as «Guardada» / «Guardado» (per field: «clave» and «contraseña» are feminine, «usuario» masculine, so the word comes from `integrations.field.<input>.saved`, not one shared string) with a check icon and never refills. Saving an empty field does nothing (the button is disabled); clearing is «Olvidar credenciales». The user name of ScreenScraper is treated as a secret too. Inputs keep `autoComplete="off"`, `spellCheck={false}` and a `name` that is not `password`/`username`; the app never turns on WebView2's password autosave (off by default) so Windows does not offer to store the key a second time. | Existing `CredentialRow` behaviour, kept. The user name is enough to target an account; same handling is simpler than two rules. |
| D9 | «Probar conexión» | Explicit only; never on tab open or on focus. Uses the existing `test_source_key` (one request, `key_check_net` cap, no retries). Outcomes → copy: `ok` «Funciona», `invalid` «Credenciales incorrectas», `missing` «Falta la clave» / «Falta la cuenta», `offline` «Sin red», `rate-limited` «Cuota agotada, demasiadas peticiones o API cerrada por ahora», `unavailable` «El servicio no responde». `rate-limited` is also what ScreenScraper's 401/423 («API closed for now / for non-members», `screenscraper.rs::send`) map to, so its copy must not promise «vuelve mañana». The result sets the card state for the session (D6). | Opening Settings must not send anything to anyone. The six `KeyCheck` values already cover the brief's four outcomes plus two. |
| D10 | «Cómo conseguir la clave» | A link to the service's own page plus one line of steps. SteamGridDB: `https://www.steamgriddb.com/profile/preferences/api` — «Inicia sesión con Steam, abre Preferencias › API y genera una clave.» ScreenScraper: `https://www.screenscraper.fr/membreinscription.php` — «Crea una cuenta gratuita y usa aquí tu usuario y contraseña.» Opened with the existing external-link command (the URL comes from the registry by id, never from the webview). Both URLs are re-checked by hand when implementing; if one has moved, the service's home page from `credits.json` is used and the change noted in the PR. | Official pages only (D4 test). Opening by id keeps the webview unable to open arbitrary URLs. |
| D11 | ScreenScraper quota | Today `check_account` does **not** parse its answer: it only looks at the status (`Ok(r) if 2xx => KeyCheck::Ok`), and `note_user` is only reached from the JSON path of a run (`get_json`). So: `check_account` returns `AccountCheck { verdict, quota: Option<SsQuota> }`, parsing `response.ssuser.{requeststoday, maxrequestsperday, maxthreads}` from the 2xx body with the same `num` helper `note_user` uses; a body that is not JSON keeps `verdict: Ok` and `quota: None`. Runs report the last numbers `note_user` saw through the source (a getter, no second parse). The app keeps the latest `SsQuota` and the time it was seen in memory (`AppState`), never on disk. The card shows «Hoy: {used} de {max} peticiones» and «Hilos: {n}» in mono, numbers formatted with `Intl.NumberFormat(locale)` (es: `1234`, `20.000`; en: `1,234`, `20,000` — never a hand-made «1 234»), plus «visto {relative}» from the existing `formatRelative`. No numbers until a check or a run has produced them. Only builds with developer credentials can produce them (none of the official builds yet, D22). | Real numbers from the service, never estimated. Memory only: no new state file about accounts. |
| D12 | Enable / disable | One switch per card bound to the existing setting: `sources.order[id].enabled` for SteamGridDB and ScreenScraper, `catalogs.switch.enabled` for the catalogue. Same rules as Fuentes: the switch of a source is disabled until it is usable (key stored, built), an enabled one can always be turned off; turning off the **last** enabled source is refused by validation (`sources.order` `Required`) and the card shows that field error next to its switch (it already exists in Fuentes). Turning a service off never touches the keychain. For the catalogue, turning it off still deletes the downloaded file (existing behaviour, adjustments §9.2) **and** resets its icons and fallback switches to off (`SwitchCatalogSettings::normalise`); turning it on again does not restore them. Its hint says both: «Al desactivarlo se borra el catálogo descargado y se apagan los iconos y el respaldo; se puede volver a descargar.» | No second flag to drift from Fuentes. The catalogue's delete-on-off and normalise are existing owner decisions; the brief's «sin borrar la clave» holds (it has no key). |
| D13 | «Olvidar credenciales» | Button on cards with stored fields. Click turns the card's action row into an inline confirm: «¿Olvidar las credenciales de ScreenScraper? Se borran del llavero de este equipo.» with «Olvidar» (danger) and «Cancelar»; focus moves to «Cancelar»; Escape cancels (the handler stops at the card: Settings has no global Escape today, only `SettingsSearch` clears its own field). Clears every field of that integration (`forget_integration`, §8.2). No `window.confirm`, no dialog. Today's per-field «Quitar» buttons go away. | The brief asks for in-page confirmation. Forgetting an account forgets the whole account (user + password), never half. |
| D14 | Deep links | Each card is an `article` with `data-setting-row` and `data-advanced-fold="integration-<id>"`; its header button has DOM id `integration-<id>`. `openSettings('integrations', { focus: 'integration-<id>' })` then needs no new code: `SettingsScreen`'s focus effect finds the id, sees the closest `[data-advanced-fold]` is closed, opens it through `navStore.setFold`, and on the next render `arrive()` scrolls (smooth, or `auto` under reduced motion), focuses the header and marks the row for 1.2 s. The card body is rendered with `forceMount` and `hidden` while closed (like `AdvancedFold`, SP-3), because the focus effect and `registry.test.tsx` both need every `setting-…` control mounted; a Radix `CollapsibleContent` without `forceMount` would unmount the Switch catalogue controls and break search landing and the completeness test. Notices use a small `IntegrationLink` (wraps that call; `data-settings-opener` so Back returns focus to it, like `SettingsLink`). | The existing focus mechanism (settings-ui D10/D11) does the rest. |
| D15 | Search | `SETTINGS_INDEX` gains one entry per working integration (`id: integration-<id>`, section `integrations`, keywords from `integrations.service.<id>.keywords`: e.g. «api key, clave de api, token, steam» / «usuario, contraseña, cuenta, password, login»), the moved Switch catalogue settings (new ids `setting-integrations-switch-catalog*`), and one entry for «Próximamente» whose keywords list the planned names (so «igdb» finds the list and its roadmap link). | Search by service and by field name, as asked. Planned names are findable without pretending they work. |
| D16 | Keychain errors per OS | `CredentialError` gains `Denied`. `classify` decides by the platform's **numeric code**, never by message text (macOS messages come from `SecCopyErrorMessageString` and follow the system language), and never by today's variant alone (keyring maps very different causes to the same variant). The boxed platform error of `PlatformFailure` / `NoStorageAccess` is downcast to the backend's own type — `security_framework::base::Error` (`.code()`) on macOS, `windows_native_keyring_store::PlatformError(u32)` on Windows, `secret_service::Error` on Linux — with target-specific dependencies pinned to the versions `keyring` 4.2 already pulls (`security-framework` 3.7, `windows-native-keyring-store` 1.1, `secret-service` 5.2); `BadEncoding` / `BadDataFormat` keep today's «not text» message and are never formatted. Table: **macOS** −25308 `errSecInteractionNotAllowed` (keychain locked and no UI, e.g. a CLI over SSH) → `Locked`; −128 `errSecUserCanceled`, −25293 `errSecAuthFailed`, −61 / −25292 / −25244 (read-only, owner edit) → `Denied`; −25291 `errSecNotAvailable`, −25294 `errSecNoSuchKeychain`, −25295 `errSecInvalidKeychain` → `Unavailable` (today −25294/−25295 arrive as `NoStorageAccess` and read as «locked», which is wrong). **Windows** 1312 `ERROR_NO_SUCH_LOGON_SESSION` (services, scheduled tasks and SSH sessions without a loaded profile) → `Unavailable`; 5 `ERROR_ACCESS_DENIED` → `Denied`; Credential Manager has no «locked» state, so there is no `keychain-locked.windows` copy. **Linux** `Locked` → `Locked`; `Prompt` / `PromptDisconnected` (the unlock prompt was dismissed) → `Denied` (today `Prompt` reads as «locked»); `Unavailable`, `Zbus`/`ZbusFdo` connection failures (no session bus: SSH, headless, a sandbox without the portal) and `NoResult` (no default collection) → `Unavailable`. On Linux `Entry::store_status()` is `Ok` even with no secret service running, so `KeychainStore::available()` alone cannot report «unavailable»: the first failing call must. Anything else stays `Failed(detail)` (detail already secret-free). `IpcError` gets `keychain-denied`. The OS comes from a new `AppInfo.platform` (`std::env::consts::OS`: `macos`, `windows`, `linux`) — `AppInfo` today has only `name`, `version`, `core_version` — and the UI picks `errors.keychain-<kind>.<os>`, falling back to the existing `errors.keychain-<kind>`. Copy in §7.3. | Each message says what to do on that machine, and the kind is right on every OS. |
| D17 | Export / import, logs, crash reports | There is no settings export/import today, and keychain secrets are not in `settings.json`. A Rust test serialises a fully populated `Settings` and fails if any `CredentialKey` account name appears or a test secret placed in the store leaks into it; the same test will guard a future export because it serialises the same type. One credential **does** live in `settings.json` today: `network.proxy_url` accepts `http(s)://user:pass@host` (validation only checks the scheme and whitespace). Not moved by this item (owner decision, §12); the test exempts that one field by name, and `integrations list`/logs never print it. Logs: `credential set/cleared: <account>` stays; new lines only say `set` / `unset` / the `KeyCheck`. There is no crash report to scrub: `ErrorBoundary` only does `console.error(error, componentStack)` (component names, no props) and the Rust panic hook (`archive::guard`) forwards to the default one; neither ever sees a value because values never enter React state beyond the typed, uncontrolled-after-save input (D8). ScreenScraper puts the user's credentials in the request query; `ScreenScraperSource::send` already strips the query from every `NetError` it returns (`public` URL, `scrub`), and a test pins that for `Status`, `Offline` and `Unregistered`. | Nothing to build for export; a guard so nobody adds secrets later; the proxy gap is named, not hidden. |
| D18 | Can the CLI use the keychain? | Yes. `KeychainStore` moves from `src-tauri/src/keychain.rs` to `coverly_core::credentials::keychain` behind a `keychain` cargo feature (on in `src-tauri` and `coverly-cli`), same `SERVICE` (`com.maecly.coverlyroms`) and account names, so app and CLI share entries. With keyring's default `v1` feature this pulls the Linux backend (`zbus`, pure Rust: no `libdbus` to install) into the CLI. `COVERLY_NO_KEYCHAIN=1` turns it off. **Every** existing `assert_cmd` test of the CLI (fill, organize, catalog…) sets it too, not only the new ones: otherwise a developer's real keychain is read during `cargo test` (a macOS prompt per test, and a real key sent to the test's mock server). Keychain calls in the CLI run on a helper thread with a 5 s cap; a timeout counts as unavailable with one stderr note (a Secret Service that never answers, or a macOS prompt nobody sees in a cron job, must not hang a script). macOS: the app is ad-hoc signed (`signingIdentity: "-"` in `tauri.conf.json`), so an item's «Always allow» is tied to one build's code hash; the separate `coverly` binary, and every update of either, gets the «wants to use your confidential information» prompt again. | `integrations set` is pointless without a store; one implementation keeps names identical. The prompt behaviour is an owner decision (§12). |
| D19 | CLI key precedence | For `fill`, `organize` (online identification, when ScreenScraper is enabled) and `integrations test`: environment variable first (`COVERLY_SGDB_KEY`, `COVERLY_SS_USER`, `COVERLY_SS_PASSWORD`, unchanged), then the keychain, read lazily — only for a source that is enabled and whose variable is empty. Keychain values are sent **only to the default endpoints**: when a run overrides them (hidden `--steamgriddb-url` / `--screenscraper-url`, used by tests and mocks), only environment values are used, so a saved key never reaches a host the user did not choose for it. A keychain that fails, times out or is off counts as empty with one stderr note. | Scripts and CI keep working as before; desktop users get their saved keys. Owner decision listed in §12. |
| D20 | CLI shape | `coverly integrations list [--json]`; `test <id> [--json]`; `set <id> [--field <user\|password\|api-key>] [--from-env <VAR>]`, reading the value from stdin (no echo when stdin is a TTY, through the `libc` termios / `windows-sys` `SetConsoleMode` calls the workspace already depends on, no new crate; one line, trimmed) or from the named variable; `forget <id> [--yes]` (asks «y/N» on a TTY without `--yes`; refuses on a non-TTY without `--yes`). No command takes a secret as an argument; a stray positional after `<id>` is a usage error. Outputs say `set` / `not set`, never values; `list` never prints `network.proxy_url`. Exit codes follow `main.rs`'s existing table: 0 success; 1 test said `invalid`, `missing` or `unavailable`, or a keychain error; 2 bad usage, including an unknown or planned id (as an unknown system id is today); 5 test said `offline` or `rate-limited` (the existing «paused: offline or rate limited» code). The `main.rs` doc comment lists the new meanings. | Matches the brief; no new exit code, and «paused» keeps one meaning across commands. |
| D21 | Migration | None needed: same keychain service and account names (D5). On first open the tab shows the stored keys as «Guardada». A test stores keys in a `MemoryStore` under the three names and checks the integrations view reports them. Presence comes from the same cached read the fill uses (`AppState::source_keys` / a presence cache invalidated by `set_credential` / `clear_credential`), not from three fresh `get`s per tab open: on macOS each `get` of an item can raise a keychain prompt after an update (D18), and the old Fuentes block already read all three on every open. | The brief's «aparecen solas», without a prompt storm. |
| D22 | ScreenScraper without developer credentials | Card state `unavailable` with «Esta versión aún no incluye acceso a ScreenScraper. Puedes guardar tu cuenta para cuando llegue.» Fields stay editable (saved for a build that has it); «Probar conexión» is disabled with that reason. Today this is **every** official build: the developer credentials are still pending (`docs/TRACKER.md` M6-02), so the first draft's «Las versiones oficiales sí» was false. | The user can prepare the account; nothing pretends to work. |
| D23 | Switch catalogue card | No credentials and no «Probar conexión»: its status line is the existing `SwitchCatalogStatusLine`, its actions «Descargar» / «Buscar actualización» / «Borrar catálogo», the icons switch and, folded, the nlib fallback switch and URL. State mapping: `off` → off; `ready` → connected; `failed-*` / `rejected` / `remove-failed` → error with the existing reason; `pending` / `downloading` → saved with progress. | A download is its own test; a second button would do the same thing. |
| D24 | Actualizaciones card | A `link` entry: one line («Comprueba si hay una versión nueva de Coverly ROMs en GitHub»), the current channel as text, «Cambiar en General» (focuses `setting-general-update-channel`; «Buscar al abrir» is `setting-general-check-updates-on-launch`, linked the same way) and «Ver versiones» through the existing `open_link('releases')`. No credit link: the updater talks to Coverly's own releases, which `net/hosts.rs` registers with `credit: None`. No toggle here. | The owner: «Enlazado desde aquí, sin moverlo». |
| D25 | Logos | None. Each card uses an app icon by kind (lucide `KeyRound` for api-key, `UserRound` for account, `Database` for dataset, `RefreshCw` for updates) and the service name in text. | SteamGridDB and ScreenScraper publish no logo licence for third-party apps; names and our own icons are always allowed. |
| D26 | Which cards start open | Collapsed by default except: a card in `error` or `quota`; the deep-link target; every card when none is configured (empty state). Open state lives in `navStore.openFolds` (session only, the existing mechanism). | Problems are never hidden; a calm tab otherwise. |
| D27 | Notices on other screens | One component, `IntegrationNotice`, takes `{ integration, problem }` with `problem` ∈ `missing` \| `rejected` \| `quota` \| `off` and renders «SteamGridDB sin clave», «ScreenScraper rechazó la cuenta», «Cuota de ScreenScraper agotada por hoy», «Catálogo de Switch desactivado» with «Configurar» linking the card. Back end: gather records structured `SourceTrouble { source, kind }` beside its existing warning strings (strings unchanged); fill and organise plans carry `integration_notices` built from them via the registry (`source` → integration id). | Text stays one place; screens never parse warning strings. |
| D28 | Where notices appear | **Fill wizard**: today it already shows `fill.popularityNoKey` («SteamGridDB no está listo, así que no se usan votos: añade una clave en Ajustes → Fuentes.») when the popularity strategy is picked and SteamGridDB is not ready; that alert keeps its place and condition, its text drops «en Ajustes → Fuentes» and gains the «Configurar» `IntegrationLink`. Fill summary: from `integration_notices` (`MissingKey` happens only when a key was forgotten after the source was enabled, since the switch cannot be turned on without one). **Organise**: there is no «online identification» option in the UI — online lookup is implicit (`online_wanted`: ScreenScraper enabled + dev credentials + account + not offline). So the notice shows when ScreenScraper is **enabled** but `not-configured` or `quota` (an `unavailable` build cannot enable it). The Switch catalogue card is linked when the pile has Switch items and the catalogue is off («Los nombres de Switch salen del archivo; activa el catálogo de Switch para usar los oficiales»), the one place the catalogue changes what Organizar shows. The plan warning `catalogue-missing` is **not** linked to it: that warning is about the DAT catalogues of a system (No-Intro / libretro-database, `env.catalogue.unconsulted()`), not titledb. **Duplicados**: no notice — nothing in the duplicate search reads an account, a quota or the Switch catalogue (its «catálogo» level 2 is the DATs), so any notice there would be false; listed in §12. **CLI**: `fill`'s `after_help` («the CLI has no keychain») and `catalog`'s `ENABLE_HINT` («…in the app's Sources settings») are rewritten to point at `coverly integrations set <id>` / «Integraciones». Game sheet: out of scope (no game sheet exists yet; the component is ready for it). | Where the missing service changes the result the user is looking at, and nowhere it does not. |
| D29 | Status after jobs | When a fill or organise job ends, its `SourceTrouble`s update the in-memory integration health (D6): `MissingKey` → no change (state already says it), `Auth` → `error/invalid`, `Exhausted` → `quota`, and a source that answered at least one request with a 2xx → `connected` (gather records this per source; a run where it was never asked changes nothing). CLI runs never touch the app's health (different process). | The card tells the truth about the last real use, not only the last button press. |

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
| Catálogo de Switch | — (fallback URL, folded) | — (D23) | — | `catalogs.switch.enabled` | Nombres e iconos de Switch en Organizar | `titledb`, `nlib`, `nintendo-eshop-icons` | — |
| Actualizaciones | — | — | — | — | Avisos de versión nueva | none (D24; «Ver versiones» instead) | — |

### 5.3 Próximamente

IGDB (client ID y secret de Twitch), RetroAchievements (usuario y clave de
la API web), TheGamesDB (clave de API), RAWG (clave de API), Microsoft
Store, GameTDB, Wikidata (sin cuenta). Each row: name, what it will need,
what it is for, in one line; the list ends with «Ver el plan» opening the
roadmap through a new `LinkId::Roadmap` in `prefs.rs` (the one file besides
`net/hosts.rs` allowed to hold project URLs; default target: `TODO.md` on
`main`, §12). `open_link('repo')` alone would land on the README, not the
plan. No inputs, no toggles, no keychain keys. Planned services have no
credit link yet (D3).

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
- **Type**: group titles use the existing `sectionTitle` (Atkinson
  Hyperlegible Next, `text-dense` 14 px, `font-semibold` — not Archivo,
  which the app keeps for `heading`/`display`); service names the same
  size at `font-medium`; Atkinson Hyperlegible Next for body and hints;
  Atkinson Hyperlegible Mono only for real data — quota numbers, the
  fallback URL, typed keys.
- **Layout**: one left-aligned column inside the existing `settings-group`
  (`max-width: 40rem`, `gap: 1rem`). Each service is a row, not a tile:

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
  existing `CollapsibleContent` height animation (`animate-collapsible-*`,
  `motion-reduce:animate-none`); the status mark swaps without animation;
  the deep-link arrival scrolls smoothly only without reduced motion
  (`arrive()` already checks); the test result is announced
  (`role="status"`), not animated; «Probando…» uses the existing `Spinner`
  (already still under `prefers-reduced-motion`).

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
- Tags are the existing `Badge` (`outline` + `border-dashed` for the
  dashed ones, `tag` for Manila, `primary`, `warning`, `danger`); no new
  variant and no new token. The dashed border is 1 px `border-strong`,
  which meets 3:1 against `surface` in both themes; the word inside
  carries the meaning anyway.

### 6.3 Status tags

| State | Tag (es / en) | Style | Extra line |
|---|---|---|---|
| off | Desactivado / Off | dashed, text-secondary | Per service, from `powers`: «Actívalo para usarlo al rellenar portadas.» (SteamGridDB, ScreenScraper) or «Actívalo para usar sus nombres al organizar.» (Switch catalogue) |
| unavailable | No disponible / Not available | dashed, text-secondary | D22 copy |
| not-configured | Sin configurar / Not set up | dashed, warning text | «Falta la clave de API.» / «Falta la cuenta.» |
| saved | Guardada / Saved (catalogue: «Pendiente» / «Descargando», D23) | solid manila | «Sin comprobar en esta sesión.» |
| connected | Conectado / Connected (catalogue: «Listo» / Ready) | solid primary | «Comprobado {relative}.» (`formatRelative` already yields «hace 2 minutos») |
| error | Error / Error | solid danger-bg | reason + fix (§7.2) |
| quota | Cuota agotada / Quota used up | solid warning-bg | «Has usado {used} de {max} peticiones hoy. Vuelve mañana; mientras, el relleno sigue con las demás fuentes.» |

### 6.4 Keyboard and screen readers

Each card header is a `button` (`aria-expanded`, `aria-controls`) inside
an `h3` (so heading navigation lists the services under the group
`h2`s), whose accessible name is «SteamGridDB, Sin configurar»; the
one-line use is its `aria-describedby`. Tab order inside an open card:
toggle, fields with their «Mostrar», actions, links. Closed card bodies
are `hidden`, so their controls are out of the tab order even though they
stay mounted (D14). The forget confirm traps nothing: it replaces the
action row, focus moves to «Cancelar», Escape restores the row and focus
to «Olvidar credenciales»; the confirm row is a `role="group"` labelled
by its question. Test results and saves are announced through one polite
live region per card that is mounted empty with the card (a region
inserted together with its text is not announced by VoiceOver/NVDA).
The «Mostrar» button keeps today's `aria-pressed` + `aria-controls`.
Status tags never rely on colour: word + icon (D25, §6.2).

### 6.5 Narrow windows

The window's minimum is 900 px (`tauri.conf.json`), and the settings
nav takes part of it, so the column can be ~560 px wide, and narrower at
200 % zoom. Layout uses wrapping flex (`flex-wrap`), not a viewport media
query: the field row wraps the input under its label when it runs out of
room, the status tag wraps under the name, and the quota line wraps; no
horizontal scroll at 900 px and 200 % zoom.

### 6.6 Empty state

When no working credential is stored and the catalogue is off, the intro is
followed by: «Para empezar, consigue una clave gratuita de SteamGridDB:
añade portadas a juegos que libretro no tiene.» with a button «Configurar
SteamGridDB» that opens and focuses that card. The ScreenScraper sentence
(«ScreenScraper, con cuenta, identifica ROMs por su hash.») is added only
in a build that has developer credentials; in every official build today
it would recommend something that cannot work (D22). Order chosen because
SteamGridDB is self-service in a minute.

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
| `integrations.field.api-key.saved` | Guardada en el llavero | Saved in the keychain |
| `integrations.field.user.saved` | Guardado en el llavero | Saved in the keychain |
| `integrations.field.password.saved` | Guardada en el llavero | Saved in the keychain |
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
| `integrations.releases` | Ver versiones | See releases |
| `integrations.empty` | Para empezar, consigue una clave gratuita de SteamGridDB: añade portadas a juegos que libretro no tiene. | To start, get a free SteamGridDB key: it adds covers for games libretro does not have. |
| `integrations.emptyScreenscraper` | ScreenScraper, con cuenta, identifica ROMs por su hash. | ScreenScraper, with an account, identifies ROMs by their hash. |
| `integrations.ssUnavailable` | Esta versión aún no incluye acceso a ScreenScraper. Puedes guardar tu cuenta para cuando llegue. | This version does not include ScreenScraper access yet. You can save your account for when it does. |

### 7.2 Test outcomes and errors

| `KeyCheck` | es | en |
|---|---|---|
| ok | Funciona. | It works. |
| invalid | Credenciales incorrectas. Revisa la clave o vuelve a generarla en la web del servicio. | Wrong credentials. Check the key or create a new one on the service's site. |
| missing | Falta la clave. Guárdala arriba y vuelve a probar. | No key yet. Save it above and test again. |
| offline | Sin red. Comprueba la conexión o el proxy en Red. | No network. Check the connection or the proxy in Network. |
| rate-limited | Cuota agotada, demasiadas peticiones o API cerrada por ahora. Prueba más tarde. | Quota used up, too many requests or the API is closed for now. Try again later. |
| unavailable | El servicio no responde. Puede estar caído; prueba más tarde. | The service is not answering. It may be down; try again later. |

### 7.3 Keychain messages per OS

| Key | es | en |
|---|---|---|
| `errors.keychain-locked.macos` | El llavero de macOS está bloqueado. Desbloquéalo en Acceso a Llaveros y vuelve a intentarlo; desde una sesión SSH, ejecuta antes `security unlock-keychain`. | The macOS keychain is locked. Unlock it in Keychain Access and try again; over SSH, run `security unlock-keychain` first. |
| `errors.keychain-denied.macos` | macOS no dio permiso para usar el llavero. Vuelve a intentarlo y elige «Permitir» en el aviso. | macOS did not allow access to the keychain. Try again and choose "Allow" in the prompt. |
| `errors.keychain-unavailable.macos` | No se encuentra el llavero de inicio de sesión. Revísalo en Acceso a Llaveros. | The login keychain cannot be found. Check it in Keychain Access. |
| `errors.keychain-denied.windows` | Windows denegó el acceso al Administrador de credenciales. Comprueba que tu cuenta no tiene restricciones de directiva. | Windows denied access to Credential Manager. Check that your account has no policy restrictions. |
| `errors.keychain-unavailable.windows` | El Administrador de credenciales no está disponible en esta sesión. Abre Coverly ROMs desde tu sesión de Windows (no como servicio ni tarea programada) o cierra sesión y vuelve a entrar. | Credential Manager is not available in this session. Open Coverly ROMs from your own Windows session (not as a service or scheduled task), or sign out and back in. |
| `errors.keychain-locked.linux` | El depósito de claves está bloqueado. Desbloquéalo en Contraseñas y claves o en KWalletManager y vuelve a intentarlo. | The keyring is locked. Unlock it in Passwords and Keys or KWalletManager and try again. |
| `errors.keychain-denied.linux` | El servicio de secretos denegó el acceso. Acepta el aviso de desbloqueo; en Flatpak, permite `org.freedesktop.secrets`. | The secret service denied access. Accept the unlock prompt; on Flatpak, allow `org.freedesktop.secrets`. |
| `errors.keychain-unavailable.linux` | No hay un servicio de secretos en marcha o no tiene un depósito por defecto. Instala o inicia GNOME Keyring o KWallet, crea un depósito «Inicio de sesión» y vuelve a abrir Coverly ROMs. | No secret service is running, or it has no default keyring. Install or start GNOME Keyring or KWallet, create a «Login» keyring and reopen Coverly ROMs. |

The existing `errors.keychain-*` keys stay as the fallback for an unknown
platform and for kinds an OS does not have (there is no
`keychain-locked.windows`, D16). The Linux «denied» line names Flatpak
only as a hint: Coverly ships AppImage and deb (`tauri.conf.json`), not a
Flatpak, so the copy must not read as if the user installed one. Spanish
names of OS tools are checked on a Spanish system when implementing
(«Acceso a Llaveros», «Administrador de credenciales», «Contraseñas y
claves»).

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
| screenscraper.privacy | Se envían el nombre del archivo, su tamaño, sus hashes CRC, MD5 y SHA-1 y el sistema (o el título, si no hay hash), con tu usuario y contraseña cifrados por HTTPS. Nunca la ruta completa ni el contenido de los archivos. | The file name, its size, its CRC, MD5 and SHA-1 hashes and the system (or the title when there is no hash) are sent, with your user name and password, over HTTPS. Never the full path or the file contents. |
| screenscraper.keywords | usuario, contraseña, cuenta, password, login, cuota, hilos | user, password, account, login, quota, threads |
| switch-catalog.use | Nombres, editora y fecha de los juegos de Switch por title ID al organizar. | Switch game names, publisher and date by title ID when organising. |
| switch-catalog.unlocks | Nombres de Switch en Organizar e iconos de la eShop. | Switch names in Organise and eShop icons. |
| switch-catalog.privacy | Se descarga un archivo público; no se envía nada de tu biblioteca. Los iconos, si los activas, se piden a la eShop de Nintendo, juego a juego. El respaldo en línea, si lo activas, recibe el title ID que falta. | A public file is downloaded; nothing from your library is sent. Icons, if you turn them on, are requested from the Nintendo eShop, one game at a time. The online fallback, if you turn it on, receives the missing title ID. |
| switch-catalog.keywords | titledb, nlib, title id, eshop, iconos | titledb, nlib, title id, eshop, icons |
| updates.use | Comprueba si hay una versión nueva de Coverly ROMs en GitHub. | Checks GitHub for a new version of Coverly ROMs. |
| updates.privacy | Se descarga un archivo público (`latest.json`) de las versiones de Coverly ROMs en GitHub; no se envía nada de tu biblioteca. GitHub ve tu dirección IP, como en cualquier descarga. | A public file (`latest.json`) is downloaded from Coverly ROMs' GitHub releases; nothing from your library is sent. GitHub sees your IP address, like any download. |

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
crates/coverly-core/src/net/hosts.rs    Endpoint.credentials: bool (D4)
crates/coverly-core/src/sources/
  screenscraper.rs                      check_account → AccountCheck
                                         { verdict, quota }; last quota
  mod.rs                                SourceTrouble
crates/coverly-core/src/gather/mod.rs   collect SourceTrouble beside warnings
crates/coverly-core/Cargo.toml          feature keychain = ["dep:keyring"] +
                                         target deps for code classify (D16)
crates/coverly-cli/src/integrations.rs  (new) list/test/set/forget
crates/coverly-cli/src/{main,fill,organize}.rs  subcommand; env→keychain
                                         keys (default endpoints only,
                                         D19); hints «coverly integrations
                                         set <id>»; exit-code doc
crates/coverly-cli/src/catalog.rs       ENABLE_HINT → Integraciones
crates/coverly-cli/tests/*              COVERLY_NO_KEYCHAIN=1 everywhere
src-tauri/src/keychain.rs               re-export from core (or removed)
src-tauri/src/integrations.rs (new)     integrations_view, health map
src-tauri/src/ipc.rs, lib.rs            integrations_view, test_integration,
                                         forget_integration,
                                         open_integration_help; AppInfo
                                         .platform
src-tauri/src/sources.rs                sources_info needs ← registry
src-tauri/src/prefs.rs                  LinkId::Roadmap (§5.3)
src-tauri/src/error.rs                  keychain-denied
src/lib/integrations.ts       (new)     typed loader of the JSON
tsconfig.json                           include "integrations" (D2)
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
  registry.ts                           integrations entries; Switch ids
                                         setting-sources-switch-catalog* →
                                         setting-integrations-switch-
                                         catalog*; fold sources-advanced →
                                         integrations-switch-advanced
  IntegrationLink.tsx, IntegrationNotice.tsx (new)
src/stores/navStore.ts                  'integrations' in SETTINGS_SECTIONS
                                         and SETTINGS_GROUPS
src/screens/SettingsScreen.tsx          SECTION_COMPONENTS entry
src/components/fill/FillWizard.tsx      popularityNoKey alert + link (D28)
src/screens/OrganizeScreen.tsx          notices (D28); nothing in dedupe
locales/{es,en}/ui.json                 §7 keys; moved/removed keys (D7);
                                         fill.popularityNoKey reworded
docs/04-architecture/{settings,desktop-app,art-sources,fill-pipeline}.md
                                         («Settings → Sources», «the CLI has
                                         no keychain» become wrong),
README.md, README.es.md (COVERLY_SGDB_KEY «la CLI no…»), CHANGELOG.md
```

### 8.2 IPC

- `integrations_view() -> IntegrationView[]`: per working entry `{ id,
  enabled, built, fields: { key, set }[], state, reason?, checked_at?,
  quota? }`. Reads keychain presence from the cached read (D21; blocking
  pool on a miss), settings and the in-memory health. Never a value.
- `test_integration(id) -> { verdict: KeyCheck, quota?: SsQuota }`:
  wraps `test_source_key`, updates health.
- Existing `set_credential`, `clear_credential`, `credential_status`,
  `sources_info` and `test_source_key` stay (same names and payloads, so a
  half-updated mock or an older build's webview still works); the card
  saves per field with `set_credential`. `forget_integration(id)` clears
  every key of the entry (each key cleared, all attempted even if one
  fails; on a partial failure the view reports what is still set and the
  error). An unknown or planned id is `BadRequest`.
- `open_integration_help(id)`: opens the registry `help_url` for `id`
  with the opener plugin's Rust API, like `open_credit`; the webview names
  an id, never a URL.

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
- SteamGridDB without a key: its switch is disabled, as in Fuentes today
  (D12). If the key is forgotten while the source is on, the source stays
  on, the card shows `not-configured`, and the fill skips it with its
  existing `MissingKey` warning plus the notice (D28).
- Turning off the last enabled source from a card: refused by validation
  (`sources.order` `Required`); the switch springs back and the card shows
  the error.
- Settings failed to load: the tab shows the same load error as the other
  non-standalone tabs (D1); the keychain is not touched.
- Older build after a rollback, or a newer build with the same keychain:
  same service and account names (D5); no setting was added or renamed, so
  `settings.json` round-trips unchanged.
- Quota numbers absent from the answer: no quota line; never zeros.
- Quota seen yesterday: shown with its age («visto ayer»); the `quota`
  state expires 24 h after it was seen (ScreenScraper resets on its own
  clock; we do not guess its timezone) and the card falls back to `saved`.
- Deep link to a planned id or unknown id: opens the tab without focus.
- Switch catalogue download running while the tab moves: the module-level
  `inflight` tracker moves with `SwitchCatalogGroup` unchanged.
- Search ids renamed (D15): no persisted references exist (`focusSetting`
  is in-memory); the registry completeness test catches stale ids.
- CLI `set` with stdin closed or empty: exit 1, «no value read from stdin».
  With `--from-env VAR` unset: exit 1 naming the variable, not a value.
- CLI on a non-TTY `forget` without `--yes`: refuses, exit 2 (usage).
- CLI on a headless Linux box or over SSH with no session bus: the first
  keychain call fails as `Unavailable` (D16) within the 5 s cap (D18);
  `fill` and `organize` go on with environment keys only.
- CLI on macOS over SSH: a locked login keychain gives
  `errSecInteractionNotAllowed` → `Locked` with the
  `security unlock-keychain` hint; no GUI prompt is waited for.
- CLI run inside tests: `COVERLY_NO_KEYCHAIN=1` in every CLI test (new and
  existing, D18) so neither CI nor a developer machine touches a real
  keychain. The ignored `real_keychain_round_trip` test keeps its own
  service name (`com.maecly.coverlyroms.test`) after the move.
- Windows paths and mtimes: not involved; CLI tests compare output text,
  no fixtures with paths. Line endings: `set` trims the trailing `\r\n`
  a Windows console or a CRLF pipe adds before storing.

## 10. Test plan

### Rust (`cargo test --workspace`)

- `integrations::problems` is empty for the shipped file; copies missing a
  credit, with a `CredentialKey` owned twice or not at all, a planned entry
  with fields, a help URL on another host or the denylist, each fail
  naming the entry.
- Coverage: every `net::hosts::ENDPOINTS` entry with `credentials: true`
  has its credit in a working entry; a copy of the endpoints with an extra
  credentialed provider and no entry fails naming it. `sources_info`'s
  `needs` equals the registry's `fields` for every `KNOWN_SOURCES` id
  (src-tauri test).
- `state()` table test over every precedence pair of D6.
- `CredentialKey` account names and `SERVICE` pinned (D5, D21); a
  `MemoryStore` with the three keys yields `saved` for both services.
- `classify` table test per OS (`#[cfg(target_os)]`, built from the
  backend error types with the codes of D16): macOS −25308 → `Locked`,
  −128 / −25293 → `Denied`, −25294 → `Unavailable`; Windows 1312 →
  `Unavailable`, 5 → `Denied`; Linux `Locked` → `Locked`, `Prompt` →
  `Denied`, `Unavailable` → `Unavailable`. No secret bytes in any message
  (existing test extended). `scripts/dev/clippy-windows.sh` covers the
  Windows-only code; the Linux arm is compiled by CI's Linux job.
- `check_account` parses `requeststoday`, `maxrequestsperday`,
  `maxthreads` into `SsQuota` from a fixture answer; absent numbers or a
  non-JSON 2xx → `Ok` with no quota.
- ScreenScraper errors never carry `sspassword` / `ssid` (D17).
- Gather records `SourceTrouble` for `MissingKey`, `Auth`, `Exhausted`
  while its warning strings stay byte-identical.
- Settings serialisation contains no credential account name or secret
  (D17), `network.proxy_url` exempted by name.
- With an overridden endpoint (`--steamgriddb-url`), a keychain key in a
  `MemoryStore` is not used; an env key is (D19).
- CLI unit tests with a `MemoryStore`: `set` from a reader and from an env
  lookup, `forget`, `list` text and JSON never contain the value; `test`
  maps verdicts to exit codes (1 / 5, D20). `assert_cmd` tests with
  `COVERLY_NO_KEYCHAIN=1`: `integrations list` succeeds and prints
  `keychain: off`; `set` exits 1; `integrations set steamgriddb sk-123`
  and `integrations test igdb` are usage errors (exit 2).
- `no_url_literal_outside_the_registry` still passes.

### Vitest

- `IntegrationsSection`: renders one card per working entry and the
  Próximamente list with no inputs; each state tag word; stored values
  never appear in the DOM after save (assert the input is empty and no
  text node holds the value); Show toggles only typed text; forget
  confirm in-page (no `window.confirm` spy calls), Escape cancels, focus
  returns; test outcomes copy for all six `KeyCheck`s; quota line
  formatting with exact counts in es and en (`Intl.NumberFormat`); closed
  cards keep their controls mounted but `hidden`; the per-card live
  region exists before its first message.
- Toggle writes the same setting path Fuentes writes.
- `SourcesSection`: no credential inputs; «Configurar en Integraciones» on
  SteamGridDB and ScreenScraper only; it opens the tab with the card
  focused.
- Registry / search: «api key», «contraseña», «token», «igdb»,
  «screenscraper» land on the right entries in es and en; completeness
  test covers the new ids.
- Keychain alert picks the OS-specific key and falls back.
- `IntegrationNotice` per problem; the Fill wizard's existing
  `popularityNoKey` alert carries the «Configurar» link; Organise shows
  the ScreenScraper notice only when it is enabled, and the Switch
  catalogue notice only with Switch items and the catalogue off; a
  `catalogue-missing` warning is not linked to the Switch card.
- `SourcesSection` keeps the «usable» rule: SteamGridDB's switch is
  disabled without a key.
- i18n parity test passes; mock backend contract test covers the new calls.

### E2E (Chrome DevTools MCP, isolated context `integrations`, port 1469)

Against the Vite dev server with the mock backend. The configured port
is 1431 (`vite.config.ts`, `strictPort`), so it is started as
`pnpm dev --port 1469` and stopped by port only
(`lsof -ti tcp:1469 -sTCP:LISTEN | xargs -r kill`). The mock also needs
`?mockIntegrations=nodev` (ScreenScraper `unavailable`, the state of every
official build) and a `?mockKeychain=locked|denied|unavailable` switch
with `?mockPlatform=macos|windows|linux` to see each OS message.

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
   at 900 px wide (the window minimum) and at 200 % zoom; once with
   reduced motion emulated.
10. `?mockIntegrations=nodev`: ScreenScraper «No disponible», test
    disabled with its reason, no ScreenScraper sentence in the empty state.
11. `?mockKeychain=locked&mockPlatform=windows`: generic locked copy
    (no Windows «locked» key); `denied&mockPlatform=linux`: Linux copy.

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
   environment variables, lazily, with a 5 s cap, and only towards the
   default endpoints. Cost on macOS: because builds are ad-hoc signed
   (`signingIdentity: "-"`), the first read of each item by `coverly`, and
   again after every update of the app or the CLI, shows a system prompt;
   a cron job sees `Locked`/`Denied` instead. Alternatives: CLI keeps
   env-only and `integrations set/forget` only manage the app's items; or
   sign with a Developer ID so «Always allow» survives updates (also
   stops the app's own re-prompts after each update).
2. **Official «how to get the key» pages** (D10) are the current SteamGridDB
   API preferences and ScreenScraper sign-up pages; if either moves, the
   card falls back to the credit's home page.
3. **Tab position** (D1): after Fuentes in Portadas. Alternative: under
   Aplicación before Red.
4. **Quota memory** (D11): memory only, cleared on restart. Alternative:
   persist the last numbers in the app data dir.
5. **Proxy credentials** (D17): `network.proxy_url` may hold
   `user:pass@` in plain text in `settings.json`. Default: unchanged in
   this item (not shown by the CLI, exempted by name in the secrets test).
   Alternative: move proxy user and password to the keychain as a
   «Proxy» integration (new `CredentialKey`s, outside this item's YAGNI
   rule).
6. **Duplicados notices** (D28): none, because no integration feeds the
   duplicate search today. Alternative: a neutral «Los nombres de Switch
   salen del archivo» line there too, once dedupe reads the catalogue.
7. **«Ver el plan» target** (§5.3): default `TODO.md` on `main` through a
   new `LinkId::Roadmap`. Alternative: the landing's roadmap page.
8. **ScreenScraper card in official builds** (D22): shown as «No
   disponible» with editable fields until the developer credentials
   arrive. Alternative: hide the card (and its Fuentes row) in builds
   without them.

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

## 14. Adversarial review (2026-11-02)

Read against `TODO.md` §8 and the code on `feat/integrations-tab`. Each
line: what the draft assumed, what the code says, where it is fixed.

- **Wrong assumptions about existing code**
  - `check_account` was said to return quota numbers «it already parses»;
    it only checks the status, and `note_user` runs on the JSON path of a
    run only → D11.
  - `appInfo.platform` does not exist (`AppInfo` = name, version,
    core_version) → D16 adds it.
  - The Fill wizard was said to have a silent «no vote»; it already shows
    `fill.popularityNoKey`, pointing at «Ajustes → Fuentes» → D28.
  - Organise has no «online identification» option, and its
    `catalogue-missing` warning is about DATs, not titledb → D28.
  - Duplicados reads no account and no Switch catalogue; the drafted
    notice there would have been false → D28, §12.6.
  - There is no crash report; `ErrorBoundary` only logs to the console →
    D17.
  - `sources_info` lives in `src-tauri` and gives ScreenScraper
    `needs: []`, so the drafted core coverage test could not see it → D3,
    D4 (`Endpoint.credentials`).
  - Planned services and the updater have no `credits.json` entry; a test
    requiring one for every entry would fail at once → D3, D4, D24.
  - Fuentes forbids enabling a source without its key; the draft allowed
    it from the card → D7, D12, §9.
  - Collapsed Radix content unmounts by default, which breaks search
    landing and `registry.test.tsx` → D14.
  - `sectionTitle` is Atkinson, not Archivo; `settings-group` is 40rem →
    §6.1. The dev port is 1431, not 1469 → §10.
  - Turning the catalogue off also resets icons and fallback
    (`normalise`) → D12.
- **Owner asks missed or misread**: ScreenScraper is in no official build
  yet (TRACKER M6-02), so «Las versiones oficiales sí» and recommending it
  in the empty state were false → D22, §6.6. «Ver el plan» via
  `open_link('repo')` lands on the README → §5.3. Owner decisions added:
  §12.5–8.
- **Cross-platform**: keyring's variant mapping is wrong for several
  causes (macOS «no such keychain» reads as locked, Linux «prompt
  dismissed» reads as locked, Linux with no secret service passes
  `store_status`), and message text is localized on macOS → classify by
  code (D16). Windows has no locked state; its real failure is a logon
  session without a profile → §7.3. macOS ad-hoc signing re-prompts after
  updates and for the separate CLI binary → D18, D21, §12.1. CLI over SSH
  or headless Linux must not hang → 5 s cap, D18. CRLF from Windows
  consoles → §9.
- **Security**: keychain keys must not follow an overridden endpoint
  (`--steamgriddb-url`) → D19. Existing CLI tests would read a
  developer's real keychain → D18. The proxy URL can hold credentials in
  `settings.json` → D17, §12.5. ScreenScraper credentials travel in the
  query; the existing scrub is now pinned by a test → D17.
- **Accessibility**: heading structure, `hidden` closed bodies, live
  regions mounted before use, gendered «Guardada/Guardado», per-service
  «off» line → §6.3, §6.4, D8.
- **i18n**: numbers through `Intl.NumberFormat` (es does not group four
  digits; «1 234» was wrong), relative time through `formatRelative`,
  stale «abajo» / «Ajustes → Fuentes» strings and CLI hints → D7, D11,
  D28.
- **Performance**: presence from a cached read instead of three keychain
  reads per tab open → D21. No network on tab open (unchanged).
- **Installed users**: no setting added or renamed; same keychain names;
  old IPC commands kept for a half-updated webview or mock → §8.2, §9.
- **Exit codes**: unknown id is usage (2) and offline/rate-limited is the
  existing «paused» (5), not a new meaning of 1 → D20.
