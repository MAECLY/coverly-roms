# Contribuir y compilar desde el código

[Coverly ROMs](../../README.es.md) › Contribuir y compilar desde el código · **Español** · [English](development.md)

## Ayuda a mejorarlo

- **Pruébalo en una consola real** y
  [abre una incidencia](https://github.com/MAECLY/coverly-roms/issues) con el
  firmware, el formato que detectó y cómo fue el relleno.
- **Añade un perfil de formato o un nombre de carpeta** que use tu firmware.
- **Mejora una traducción** en [`locales/en/ui.json`](../../locales/en/ui.json) o
  [`locales/es/ui.json`](../../locales/es/ui.json).

Lee antes [CONTRIBUTING.md](../../CONTRIBUTING.md) (en inglés). En resumen: las
contribuciones se hacen bajo el
[Acuerdo de Licencia de Contribuidor](../../CLA.md); `make check` pasa todo lo que
pasa la CI; los commits siguen
[Conventional Commits](https://www.conventionalcommits.org/), y commitlint lo
comprueba; lo nuevo en `coverly-core` llega con su test primero; y el
repositorio nunca acepta ROMs ni imágenes de juegos. Los informes de seguridad
siguen [SECURITY.md](../../SECURITY.md) (un aviso privado en GitHub, o
`security@maecly.com`).

## Para desarrolladores

### Tecnología, estructura y compilar desde el código

**Tecnología.** Tauri 2 · Rust · React + TypeScript + Vite + Tailwind.

| Ruta                  | Qué contiene                                                               |
| --------------------- | -------------------------------------------------------------------------- |
| `crates/coverly-core` | Escaneo, identificación, fuentes, relleno, organizar, deshacer (sin Tauri) |
| `crates/coverly-cli`  | La herramienta de línea de comandos `coverly`                              |
| `src-tauri`           | La app de escritorio y sus órdenes                                         |
| `src`                 | La interfaz en React                                                       |

**Requisitos:** [Rust](https://rustup.rs/) 1.93 o superior,
[Node.js](https://nodejs.org/) 22.22.2 o superior con [pnpm](https://pnpm.io/), y
las [dependencias de sistema de Tauri 2](https://v2.tauri.app/start/prerequisites/)
para tu sistema operativo. `make doctor` muestra cuáles tienes instaladas.

```sh
make install                                   # instala las dependencias JS y Rust
make dev                                       # ejecuta la app de escritorio en desarrollo
make web                                       # solo la interfaz, en el navegador, con un backend simulado
make example                                   # crea una biblioteca de ejemplo y la audita
make example-fill                              # planifica rellenar sus portadas (simulación)
make audit ROOT=<ruta>                         # audita una biblioteca desde la terminal
make check                                     # lint, formato y todos los tests
```

Además: `make build` (un paquete de la app sin firmar), `make cli ARGS="systems"`
(ejecuta la CLI), `make fill ROOT=<ruta>` (planifica un relleno, simulación).

`make help` lista todos los objetivos. La biblioteca de ejemplo vive en
`target/example` (`make example-apply`, `make example-undo`, `make example-clean`).

La página web de [coverly-roms.maecly.com](https://coverly-roms.maecly.com)
vive en su propio repositorio privado, MAECLY/coverly-roms-landing,
que explica cómo verla en local, comprobarla y publicarla (en inglés).

**Documentación** (en inglés). [docs/README.md](../README.md) es el índice.
La arquitectura está en [docs/04-architecture/](../04-architecture/system-architecture.md),
las decisiones en [docs/adr/](../adr/README.md), el proceso de publicación en
[RELEASING.md](../../RELEASING.md) y el historial en [CHANGELOG.md](../../CHANGELOG.md).
