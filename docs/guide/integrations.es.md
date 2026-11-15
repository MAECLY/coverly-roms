# Integraciones, privacidad y seguridad

[Coverly ROMs](../../README.es.md) › Integraciones, privacidad y seguridad · **Español** · [English](integrations.md)

## Integraciones

Coverly funciona sin ninguna cuenta. En Configuración → Integraciones añades
las opcionales y activas o desactivas los servicios en línea:

| Servicio                      | Qué añade                                                        | Qué necesita                                                                                                         |
| ----------------------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| SteamGridDB                   | Portadas de la comunidad al rellenar, y los votos de Popularidad | Tu propia clave de API, de la [página de API de SteamGridDB](https://www.steamgriddb.com/profile/preferences/api)    |
| ScreenScraper                 | Identificar archivos por hash al organizar                       | Una cuenta gratuita, de la [página de registro de ScreenScraper](https://www.screenscraper.fr/membreinscription.php) |
| Catálogo de nombres de Switch | Nombres, editora y fecha de los juegos de Switch por title ID    | Nada; está apagado hasta que lo activas                                                                              |
| Actualizaciones de Coverly    | Comprueba si hay una versión nueva de Coverly ROMs en GitHub     | Nada; se desactiva en Configuración → General                                                                        |

ScreenScraper necesita además unas credenciales de desarrollador incluidas en
la app, y las versiones publicadas aún no las llevan: su tarjeta sale como «No
disponible», y puedes guardar tu cuenta para cuando llegue.

**Cómo conseguir una clave.** «Cómo conseguir la clave», en cada tarjeta, abre
la página de cuenta del propio servicio, nunca una web de terceros.
SteamGridDB: inicia sesión con Steam, abre Preferencias › API, genera una clave
y pégala en la tarjeta. ScreenScraper: crea una cuenta gratuita y escribe su
usuario y contraseña. Después pulsa **Probar conexión**: la tarjeta dice si
funciona, si la clave es incorrecta, si no hay conexión o el servicio está
caído y, en ScreenScraper (en las versiones que lo incluyen), cuánto de la
cuota diaria se ha usado.

**Qué se envía.** Cada tarjeta lo dice. A SteamGridDB, el nombre del juego que
buscas y tu clave; a ScreenScraper, el nombre del archivo, su tamaño, sus
hashes y el sistema, con tu cuenta, por HTTPS (nunca la ruta completa ni el
contenido de los archivos); el catálogo de Switch y la comprobación de
actualizaciones descargan archivos públicos y no envían nada de tu biblioteca,
salvo los iconos de la eShop (se piden juego a juego) y el respaldo en línea
(el title ID que falta) si los activas.

**Dónde se guardan las claves.** En el llavero del sistema: Llavero en macOS,
Administrador de credenciales en Windows y el Secret Service (GNOME Keyring o
KWallet) en Linux. Una clave guardada no se vuelve a mostrar, ni en la app, ni
en los registros, ni al exportar la configuración, ni en la CLI; solo dicen si
hay una o no. «Olvidar credenciales» la borra del llavero, y desactivar un
servicio la conserva para más adelante.

**Desde la línea de comandos.** `coverly integrations` usa la misma lista y las
mismas entradas del llavero que la app:

```bash
coverly integrations list                          # cada servicio, activo o no, y qué claves hay guardadas
coverly integrations set steamgriddb               # pide la clave sin mostrar lo que escribes
coverly integrations set steamgriddb --from-env COVERLY_SGDB_KEY  # o la lee de una variable
coverly integrations set screenscraper --field user
coverly integrations set screenscraper --field password
coverly integrations test steamgriddb              # una petición con la clave guardada
coverly integrations forget steamgriddb            # pregunta antes; --yes sin terminal
```

`set` nunca recibe el secreto como argumento, así que no acaba en el historial
de tu terminal: escríbelo en el aviso oculto, pásalo por stdin o nombra una
variable con `--from-env`. Las opciones y los códigos de salida están en
[Línea de comandos](command-line.es.md).

## Privacidad y seguridad

- **Sin telemetría ni cuenta.** La app solo se conecta a las fuentes de
  imágenes que actives, a los servicios que configures en Configuración →
  Integraciones, al catálogo de nombres de Switch si lo activas y, si no lo
  desactivas, a la comprobación de actualizaciones. Cada tarjeta de
  Integraciones dice qué envía.
- **Sin claves de consola.** Coverly nunca las lee, pide, descarga ni incluye;
  los archivos de claves y licencias de la tarjeta no se tocan.
- **Las imágenes se recodifican** antes de escribirse, así que no llevan
  metadatos.
- **Tus claves se quedan en el llavero del sistema.** La de SteamGridDB solo
  se envía a SteamGridDB, en una cabecera `Authorization: Bearer`, y la cuenta
  de ScreenScraper solo a ScreenScraper. Nada vuelve a mostrar una clave
  guardada, y los registros solo dicen si hay una o no. La configuración, su
  exportación y su importación nunca llevan una clave ni una contraseña, y
  `coverly integrations` nunca imprime una. Abrir Configuración no envía nada;
  «Probar conexión» hace una petición cuando la pulsas.
- **La auditoría solo lee.** Rellenar, organizar y apartar duplicados no
  escriben nada hasta que aplicas, nunca fuera de la raíz de la biblioteca, y
  cada trabajo se puede deshacer. Los duplicados se apartan o van a la
  Papelera; nunca se borran.
- **Las copias y el registro de cada trabajo** van a la carpeta de datos de la
  app, no a la tarjeta SD.
- **Coverly ROMs no incluye ROMs, BIOS ni imágenes de juegos.**
