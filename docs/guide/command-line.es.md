# Línea de comandos

[Coverly ROMs](../../README.es.md) › Línea de comandos · **Español** · [English](command-line.md)

`coverly` es la misma lógica que la app, para la terminal. Como la app, no
escribe nada salvo que pases `--apply`.

| Orden          | Qué hace                                                                                                       |
| -------------- | -------------------------------------------------------------------------------------------------------------- |
| `audit`        | Informa de qué ROMs no tienen portada, captura o título (solo lectura)                                         |
| `catalog`      | Gestiona el catálogo opcional de nombres de Switch (`switch status`, `update`, `remove`)                       |
| `fill`         | Rellena las imágenes que faltan desde las fuentes activas (simulación salvo con `--apply`)                     |
| `integrations` | Lista, prueba, guarda u olvida las cuentas y claves de servicios en línea (`list`, `test`, `set`, `forget`)    |
| `organize`     | Reparte un montón de ROMs sueltas en una carpeta por sistema (simulación salvo con `--apply`)                  |
| `dedupe`       | Encuentra el mismo juego guardado dos veces en un sistema y aparta las copias (simulación salvo con `--apply`) |
| `profiles`     | Lista los perfiles de formato, de serie y propios                                                              |
| `systems`      | Lista los sistemas conocidos y sus nombres de carpeta                                                          |
| `undo`         | Deshace el último trabajo (relleno, organización o duplicados), o el que indiques con `--job`                  |
| `update`       | Comprueba si hay un `coverly` más nuevo, o lo instala (verificado con minisign)                                |

La ayuda de `coverly` está en inglés; la tabla traduce su descripción.

```bash
coverly audit /Volumes/SD --missing-only --format csv --output informe.csv
coverly fill /Volumes/SD                      # simulación: el plan, sin escribir nada
coverly fill /Volumes/SD --apply --auto-only --system gba,snes --media cover
coverly fill /Volumes/SD --sources libretro,steamgriddb --strategy popularity
coverly organize ~/Downloads/roms             # simulación: propuesta por sistema
coverly organize ~/Downloads/roms --target /Volumes/SD --copy --apply
coverly dedupe /Volumes/SD                    # simulación: qué copia se queda y adónde van las demás
coverly dedupe /Volumes/SD --apply            # las aparta en /Volumes/SD/_Duplicados
coverly dedupe /Volumes/SD --apply --trash    # o las manda a la Papelera
coverly catalog switch update                 # descarga el catálogo opcional de nombres de Switch
coverly integrations list                     # qué servicios están activos y qué claves hay guardadas
coverly integrations set steamgriddb          # guarda una clave, escrita en un aviso oculto
coverly integrations test steamgriddb         # una petición con la clave guardada
coverly undo --list
coverly undo                                  # deshace el último trabajo
```

### Opciones útiles

**`fill`:** `--profile ID|auto`, `--regions rom,world,usa,europe,japan`,
`--match hash-then-name|hash-only|name-only`, `--scope missing|all` (`all`
necesita `--overwrite backup`), `--concurrency 1-6`, `--refresh` (ignora los
candidatos e índices en caché), `--report table|json`, `--output ARCHIVO`.

**`organize`:** `--layout PERFIL`, `--target CARPETA`, `--flat`,
`--copy`/`--move`, `--names keep|catalogue`, `--assign ARCHIVO=SISTEMA`
(repetible), `--resume TRABAJO`, `--include-review`, `--offline`,
`--no-external-tools` (no usa unrar, 7-Zip ni bsdtar),
`--duplicates set-aside|report|off` (copias que ya están en la biblioteca),
`--yes-large` (confirma un trabajo que escribe 10 GB o más), `--skip-large`
(deja esos elementos donde están), `--game-folders`/`--no-game-folders`
(en esta ejecución, cada juego moderno en su carpeta con `Updates/` y `DLC/`,
o todo suelto; por defecto, lo que diga el perfil),
`--emulator SISTEMA=EMULADOR|none` (repetible; ordena un sistema como lo
espera ese emulador, por ejemplo `ps4=shadps4`), `--emulator-data
EMULADOR=CARPETA` (repetible, ruta absoluta; dónde guarda ese emulador sus
trucos, parches y mods).

**`catalog switch`:** `status`, `update`, `remove`, `--data-dir CARPETA`.

**`dedupe`:** `--system gb,psx`, `--level exact,format,version`,
`--one-game-one-rom`, `--keep RUTA` (esta copia se queda; repetible),
`--keep-rules has_save,verified,…`, `--reset-choices`, `--quarantine CARPETA`,
`--trash`, `--resume TRABAJO`, `--offline`, `--no-external-tools`,
`--report table|json`, `--output ARCHIVO`.

**`undo`:** `--list`, `--job ID`, `--include-later` (con `--job`: deshace
también los trabajos posteriores que tocaron la misma carpeta, del más nuevo al
más antiguo).

**`integrations`:** `list [--json]`, `test ID [--json]`, `set ID [--field
user|password|api-key] [--from-env VARIABLE]`, `forget ID [--yes]`. `set` nunca
recibe el secreto como argumento: lo lee de stdin (sin eco si lo escribes) o
de la variable que indique `--from-env`. Ninguna salida muestra un valor, solo
`set` o `not set`. Códigos de salida: 0 bien, 1 la prueba falló (clave
incorrecta, falta, servicio caído) o el llavero lo rechazó, 2 uso incorrecto,
5 sin red o límite de peticiones.

**Variables de entorno:** `COVERLY_SGDB_KEY` (clave de SteamGridDB),
`COVERLY_SS_USER` y `COVERLY_SS_PASSWORD` (cuenta de ScreenScraper, para
`organize`). Si la variable está definida, manda; si no, la CLI usa la clave
que la app guardó en el llavero del sistema, y solo hacia la dirección del
propio servicio (nunca con `--steamgriddb-url` ni otro destino propio).
`COVERLY_NO_KEYCHAIN=1` desactiva el llavero.

`coverly <orden> --help` lista todas las opciones.
