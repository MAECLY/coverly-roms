# TODO — Coverly ROMs

## Landing

Sitio estático en `site/` (es en `/`, en en `/en/`), publicado en https://coverly-roms.maecly.com con GitHub Pages (`.github/workflows/pages.yml`). Documentación: `docs/07-build-and-release/landing.md`. El rediseño de marca sigue en curso en `feat/brand-identity` (`docs/08-brand/PROGRAM.md`), así que los números de línea de `site/index.html` pueden moverse unas 2 líneas.

### Prioridad sugerida

1. Arreglar las fuentes que apuntan a un archivo borrado antes de mergear el PR de marca (`site/index.html:22`, `site/assets/site.css:10`).
2. Recortar las meta descriptions a unos 150 caracteres (`site/index.html:8`, `site/en/index.html:8`).
3. Añadir enlaces Markdown a `llms.txt`: hoy suspende el check "agentic browsing" de Lighthouse (puntuación 67).
4. Poner `lastmod` en el sitemap aunque no haya release (`site/sitemap.xml:5,12`).
5. Añadir `.well-known/security.txt` (ahora da 404 en el sitio en vivo).
6. Verificar el dominio en Search Console y Bing, y enviar el sitemap.
7. Añadir a RELEASING.md §5 el paso de comprobar que el landing se ha vuelto a desplegar.
8. Completar los enlaces del pie (Novedades, licencias, contribuir y seguridad) y el feed Atom en el head.

### Metadatos y SEO

- [ ] Recortar la meta description a unos 150 caracteres, con "gratis / sin cuenta" al principio. Ahora mide 201 en ES y 188 en EN y Google la corta (responsable: Claude, S; mejor junto al copy del PR de marca). Evidencia: `site/index.html:8`, `site/en/index.html:8`.
- [ ] Manifest: añadir `"id":"/"` (el de ES y el de EN tienen `start_url` distinta y parecen dos apps distintas), `"categories":["utilities"]` y `screenshots` anchos y estrechos desde `assets/shots/{es,en}/` (responsable: Claude, S, bloqueado por: PR de marca, por los colores y las capturas). Evidencia: `site/site.webmanifest:7-16`, `site/en/site.webmanifest:7-16`.
- [ ] JSON-LD `Organization`: añadir `logo` (PNG cuadrado de 112 px o más), `sameAs` (`https://github.com/MAECLY`) y `email` (responsable: Claude; Miguel confirma qué perfiles listar; S, bloqueado por: elección del icono, solo para `logo`). Evidencia: `site/index.html:48`, `site/en/index.html:48`.
- [ ] JSON-LD `SoftwareApplication`: añadir `sameAs` del repo, `offers.availability`, `screenshot` como array (claro y oscuro) e `image` (la imagen OG). Llevar `softwareVersion`, `downloadUrl` y `datePublished` al mismo nodo del `@graph` en vez de un segundo `<script>` con el mismo `@id` (responsable: Claude, S, bloqueado por: release v0.1.0, solo para los campos de versión y descarga). Evidencia: `site/index.html:49`, `:54-58`.
- [ ] Sitemap: escribir siempre `lastmod` con la fecha del último commit de `site/` (`git log -1 --format=%cI -- site`), aunque no haya release. Hoy solo existe dentro del bloque `release:` y el sitemap en vivo no lo tiene (responsable: Claude, S). Evidencia: `site/sitemap.xml:5,12`, `scripts/site/apply_release.py`.
- [ ] `llms.txt`: añadir secciones `## Docs` y `## Optional` con enlaces `[título](url)`: inicio ES y EN, repo, releases, privacidad y licencia. Reflejarlos en `agents.txt` (responsable: Claude, S). Evidencia: `site/llms.txt` (Lighthouse `llms-txt`: "File does not appear to contain any links").
- [ ] Añadir `<link rel="alternate" type="application/atom+xml">` a `https://github.com/MAECLY/coverly-roms/releases.atom` en las dos páginas (responsable: Claude, S). Evidencia: un grep en `site/index.html` y `site/en/index.html` da 0 resultados, y el feed de GitHub devuelve 200.
- [ ] Añadir `site/humans.txt` (autor, MAECLY, fuentes OFL y stack) y `<link rel="author">` (responsable: Claude, S). Evidencia: `/humans.txt` en vivo da 404.
- [ ] 404: añadir `apple-touch-icon` y `rel="manifest"` para que el head coincida con el de las páginas principales (responsable: Claude, S, bloqueado por: PR de marca, porque la página se está reestilizando). Evidencia: `site/404.html:7-13`.
- [ ] Ampliar `gate_head` en `check_site.py` para que compruebe:
  - description de 160 caracteres o menos;
  - imagen OG y de Twitter presente y de 1200×630;
  - manifest con icono maskable e `id`;
  - `lastmod` en el sitemap;
  - `security.txt` presente y con `Expires` futuro;
  - que cada preload y cada `@font-face` apunten a un archivo que existe.

  (responsable: Claude, M). Evidencia: `scripts/site/check_site.py:307-349`.

### Iconos y marca

- [ ] Favicon e iconos definitivos generados desde el SVG maestro (responsable: Miguel elige, Claude genera; M, bloqueado por: elección del icono, candidatos de la ronda 2). Hay que generar:
  - `favicon.svg` con una regla `prefers-color-scheme` dentro;
  - `favicon.ico` con 16, 32 y 48;
  - `apple-touch-icon.png` de 180;
  - `icon-192.png` e `icon-512.png`.

  Evidencia: `site/assets/favicon.svg:1` ("Temporary icon…"), `landing.md:172`.

- [ ] Icono maskable (`icon-maskable-512.png`, con zona segura del 80 %) con `"purpose":"maskable"` en los dos manifests (responsable: Claude, S, bloqueado por: elección del icono). Evidencia: `site/site.webmanifest:12-16`, `docs/08-brand/brand.md:599`.
- [ ] Llevar `theme-color`, `background_color`/`theme_color` del manifest y `BG` de `theme.js` a los tokens de marca (`#F2F3EF` / `#121514`) desde una sola fuente (responsable: Claude, S, bloqueado por: PR de marca; comprobar antes si W3 ya lo cubre). Evidencia: `site/index.html:12-13`, `site/404.html:10-11`, `site/assets/theme.js:7`, `site/assets/tokens.css`.
- [ ] Poner la marca (SVG inline) junto al wordmark en la cabecera y el pie de ES, EN y 404 (responsable: Claude, S, bloqueado por: elección del icono). Evidencia: `site/index.html:65`, `:394`, `PROGRAM.md:12`.
- [ ] Rehacer las capturas WebP (es/en, claro/oscuro, 1x/2x) con la UI nueva, y revisar las coordenadas de los clips y `--fx-dy`/`--fx-cy` (responsable: Claude, M, bloqueado por: PR de marca, W2 y W3). Evidencia: `site/assets/shots/` (anteriores a la marca), `PROGRAM.md:11`, `landing.md:133-136`, `:157-165`.
- [ ] Volver a generar `og.png` y `og-en.png` con `scripts/site/og.html?lang=es|en`, valorar pasarlas a un JPG más ligero (el og.png en vivo pesa 202 KB) y revisar la vista previa con un debugger de compartir (responsable: Claude, S, bloqueado por: PR de marca, capturas nuevas y elección del icono). Evidencia: `site/assets/og.png`, `landing.md:167-170`.
- [ ] Quitar los textos provisionales "Temporary icon" y "there is no logo yet" cuando llegue la marca (responsable: Claude, S, bloqueado por: elección del icono). Evidencia: `site/agents.txt:22`, `site/llms.txt:8`, `TRADEMARKS.md:7`, `landing.md:172-173`.

### Contenido

- [ ] Pie: añadir "Novedades", con enlace a `CHANGELOG.md` y al feed de releases. Más adelante, una página `/novedades/` generada desde `CHANGELOG.md` (responsable: Claude; S para los enlaces, M para la página, bloqueado por: release v0.1.0, solo para la página). Evidencia: `site/index.html:403-410`.
- [ ] Pie: enlazar `TRADEMARKS.md`, `THIRD-PARTY-NOTICES.md` y las licencias OFL de las fuentes, en ES y EN (responsable: Claude, S). Evidencia: `site/index.html:403-413`, donde solo aparece "Licencia AGPL-3.0".
- [ ] Pie: enlazar "Contribuir" (`CONTRIBUTING.md`), "Informar de un fallo" (issues) y "Seguridad" (`SECURITY.md`). Miguel decide si habrá donaciones (`.github/FUNDING.yml` y solo un enlace, porque la CSP no permite embeds) (responsable: Claude; Miguel decide sobre las donaciones; S, bloqueado por: decisión de Miguel, solo para las donaciones). Evidencia: `site/index.html:402-411`.
- [ ] Página `/detalles/` y `/en/details/` con requisitos (disco, red, WebKitGTK/glibc en Linux, lectores SD), instalación por sistema, solución de problemas y créditos. Enlazarla desde la cabecera y el pie, y añadirla al sitemap y a la paridad de `check_site.py` (responsable: Claude escribe, Miguel revisa; M). Evidencia: `site/index.html:303-306` (solo hay líneas `.req`).
- [ ] Declaración de privacidad en `/detalles/#privacidad`: qué hosts contacta la app (libretro-thumbnails, SteamGridDB y GitHub Releases), dónde quedan las claves (llavero), las copias y los logs, y que el sitio solo guarda `coverly:mode` (responsable: Claude escribe, Miguel aprueba; S). Evidencia: hoy está repartida entre `site/index.html:374` y `:414`.
- [ ] Declaración de accesibilidad en `/detalles/#accesibilidad`: objetivo WCAG 2.2 AA, qué se comprueba, límites conocidos y contacto (responsable: Claude, S). Evidencia: `check_site.py` (`gate_contrast` y `gate_motion`), `landing.md:47-52`.
- [ ] Lista de sistemas compatibles con sus alias por firmware, generada desde `systems.toml` con un script en `scripts/site/`, y un fallo en `check_site.py` si se desincroniza (responsable: Claude, M). Evidencia: `site/index.html:226` ("más de 50 sistemas", sin lista), `crates/coverly-core/src/systems/systems.toml`.
- [ ] Página `/comparar/` con un apartado por herramienta (Skraper, Skyscraper, ARRM y Mini Scraper), fuentes y fecha. Dejar la tabla resumen en el landing con un enlace a la página (responsable: Claude, M). Evidencia: `site/index.html:266-292`.
- [ ] Aviso que se pueda cerrar ("Read this page in English") cuando `navigator.language` no coincida con el idioma de la página, guardando la elección en `coverly:lang` y sin redirigir (responsable: Claude, S). Evidencia: `site/assets/theme.js:6` (solo guarda `coverly:mode`), `site/index.html:81,84`.
- [ ] Clips grabados de la app real (Aplicar y Deshacer) al estilo Versorium: `?mockPause=off` en el backend simulado, Playwright, `record.mjs` y `encode-clips`, con presupuestos de unos 110 kB en escritorio y unos 75 kB en móvil (responsable: Claude, L, bloqueado por: PR de marca). Evidencia: `landing.md:140-155`.
- [ ] Kit de prensa `/prensa/` (o un ZIP) con wordmark, icono, capturas, descripción corta y larga en ES y EN, y las reglas de uso de `TRADEMARKS.md` (responsable: Claude prepara, Miguel aprueba; M, bloqueado por: elección del icono, capturas nuevas y OG). Evidencia: no existe nada de prensa en `site/`.
- [ ] Cambiar "Aún no se ha probado en consolas reales" en ES y EN, en la sección "Lo que aún no hace", el FAQ y el JSON-LD, y añadir una lista "probado en" (responsable: Miguel prueba, Claude cambia el copy; M, bloqueado por: prueba en consola real M2-12). Evidencia: `site/index.html:355`, `:372`, `:50`, `site/agents.txt:22`.

### Descargas y releases

- [ ] Publicar v0.1.0 para rellenar los marcadores `{{release.*}}` y comprobar que `apply_release.py` rellena todo lo siguiente, con una pasada E2E del landing en vivo (responsable: Miguel publica, Claude verifica; M, bloqueado por: claves minisign y elección del icono):
  - la pill;
  - el hueco de descarga;
  - el JSON-LD;
  - el sitemap;
  - `llms.txt`.

  Evidencia: `site/index.html:93`, `:324-337`, `landing.md:190-192`, `RELEASING.md`.

- [ ] Añadir a `RELEASING.md` §5 la comprobación del landing tras publicar (responsable: Claude, S). Los pasos son:
  - el run "Pages" en `main` está en verde;
  - un `curl` muestra la versión y el enlace al `.dmg` da 200, sin `{{` sueltos;
  - si no se lanzó, Actions → Pages → Run workflow.

  Evidencia: `RELEASING.md:141-175`, `.github/workflows/pages.yml:15-16`.

- [ ] Bloque desplegable "Comprobar la descarga" con los comandos `shasum`, `sha256sum` y `Get-FileHash`, y `minisign -Vm <archivo> -P <clave>`, y la clave pública publicada en el sitio y en `llms.txt` (responsable: Claude, S, bloqueado por: release v0.1.0, solo para la URL de los `.sig`; el texto puede ir ya). Evidencia: `site/index.html:320`, `:346`, `README.md:52-55`, `crates/coverly-core/updater.pub`.
- [ ] Visitantes desde el móvil (`data-os="mobile"`): nota de que es una app de escritorio y enlace `mailto:` con la URL de `#descargar` para abrirla en el ordenador (responsable: Claude, S, bloqueado por: release v0.1.0). Evidencia: `site/assets/theme.js`, `site/assets/site.css:1078-1085`.

### Rendimiento y accesibilidad

- [ ] Fuentes: el preload y el `@font-face` apuntan a `archivo-display-800.woff2`, que la marca borra, y eso da un 404 y la fuente de reserva. Hay que hacer esto (responsable: Claude, M, bloqueado por: PR de marca):
  - recortar `archivo-variable.woff2` (70 KB, unas 7 veces el subset actual) a los pesos y caracteres que se usan;
  - precargar solo la cara display;
  - enlazar `tokens.css`;
  - añadir `@font-face` para Atkinson Next y Mono;
  - actualizar `landing.md:173-175`.

  Evidencia: `site/index.html:22`, `site/en/index.html:22`, `site/assets/site.css:9-10`.

- [ ] Hacer que el nombre accesible del enlace de marca contenga el texto visible. Para ello, poner un espacio real en `Coverly <b>ROMs</b>` y quitar el `aria-label` o hacer que empiece igual que el texto visible. Es la única auditoría a11y que falla en Lighthouse (`label-content-name-mismatch`) (responsable: Claude, S, bloqueado por: PR de marca). Evidencia: `site/index.html:65`, `site/en/index.html:65`.
- [ ] Capturas responsive: que `encode_shots.py` genere también 640w (y quizá 960w) y añadirlas a cada `srcset` y `<source>`. Hoy el móvil descarga 1280w, unos 47 kB de más por captura (responsable: Claude, S, bloqueado por: PR de marca, porque se rehacen las capturas). Evidencia: `site/index.html:148`, `scripts/site/encode_shots.py`.
- [ ] Modo de alto contraste de Windows: añadir `@media (forced-colors: active) { .tile { border: 1px solid CanvasText; } }` y revisar las pills de modo y los botones de repetir (responsable: Claude, S, bloqueado por: PR de marca). Evidencia: `site/assets/site.css` no tiene ningún bloque `forced-colors`.

### Infraestructura (dominio/HTTPS/cabeceras)

- [ ] Añadir `site/.well-known/security.txt` (RFC 9116) con:
  - `Contact:` (`mailto:hola@maecly.com` o GitHub Security Advisories);
  - `Expires:`;
  - `Preferred-Languages: es, en`;
  - `Canonical:`;
  - `Policy:` con enlace a `SECURITY.md`.

  Comprobar que Pages sirve la carpeta con punto (con `.nojekyll` si hace falta) (responsable: Claude; Miguel confirma el contacto; S). Evidencia: `/.well-known/security.txt` en vivo da 404.

- [ ] Caché de 10 minutos y sin HSTS, por límite de GitHub Pages. Opciones: aceptarlo y documentarlo en `landing.md`, o poner Cloudflare en modo proxy con caché larga en `/assets/*`, nombres versionados y HSTS. No hace falta para el lanzamiento (responsable: Miguel decide; M). Evidencia: `curl -I …/assets/site.css` devuelve `cache-control: max-age=600` y no hay `strict-transport-security`.
- [ ] Actualizar la sección "One-time setup" de `landing.md`: marcarla como hecha el 2026-10-17 y anotar que http→https ya devuelve 301 (responsable: Claude, S). Evidencia: `docs/07-build-and-release/landing.md:177-199`, `docs/TRACKER.md:375`.
- [ ] (Opcional) `www.coverly-roms.maecly.com` no resuelve. Si se quiere, añadir un CNAME `www` y dejar que Pages redirija (responsable: Miguel, S). Evidencia: `dig +short www.coverly-roms.maecly.com` no devuelve nada.

### Medición

- [ ] Verificar `maecly.com` como propiedad de dominio en Google Search Console mediante un registro DNS TXT en Cloudflare, enviar `https://coverly-roms.maecly.com/sitemap.xml` e importarlo en Bing Webmaster Tools (responsable: Miguel, S). Evidencia: no hay `google-site-verification` ni `msvalidate.01` en `site/`.
- [ ] Documentar ese paso en `landing.md` ("One-time setup") (responsable: Claude, S). Evidencia: `landing.md:177-199` no lo menciona.
- [ ] Después del PR de marca y de la release, repetir Lighthouse en ES y EN (móvil y escritorio, claro y oscuro) y pasar el Rich Results Test de Google. Hoy la base es A11y, Best Practices y SEO a 100, LCP de 1,46 s en móvil y CLS 0 (responsable: Claude, S, bloqueado por: PR de marca). Evidencia: auditoría de calidad del 2026-10-17.

## Fuera de la landing (recordatorio)

Son tareas de la release que dependen de Miguel. El detalle está en `RELEASING.md` y `docs/TRACKER.md`:

- [ ] Generar el par de claves minisign, cargar los 2 secretos y sustituir `updater.pub` (ahora `UNSET`) (responsable: Miguel; `RELEASING.md` §2–§4, M7-01/M7-05 en `docs/TRACKER.md`).
- [ ] Elegir el icono definitivo entre los candidatos de la ronda 2 (responsable: Miguel; M7-04 y P0-13 en `docs/TRACKER.md`, W4 en `docs/08-brand/PROGRAM.md:12`).
- [ ] Etiquetar y publicar `v0.1.0` (responsable: Miguel; `RELEASING.md` §5, `docs/TRACKER.md:12`).
- [ ] Credenciales de SteamGridDB y ScreenScraper para las pruebas reales (responsable: Miguel; `docs/TRACKER.md`).
- [ ] Pruebas manuales de la app Tauri en macOS, Windows y Linux, y en la consola real (M2-12) (responsable: Miguel; `docs/TRACKER.md`).
