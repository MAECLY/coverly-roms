# Programa de marca — Coverly ROMs

Rama: `feat/brand-identity` (y ramas hijas por fase). Memoria del bucle entre compactaciones.

| Fase | Entregable | Estado |
|---|---|---|
| W1 Investigación | `docs/08-brand/research.md` (referentes, tokens, iconos) + `docs/08-brand/component-audit.md` | en curso |
| W1 Marca | `docs/08-brand/brand.md` (concepto, paleta OKLCH/hex claro-oscuro, tipografía, voz) + `design/tokens.json` (DTCG) | en curso |
| W1 Iconos | HTML con 10 alternativas → Artifact para Miguel | en curso |
| W2 Primitivas UI | `src/components/ui/*` estilo shadcn (Radix) con nuestros tokens; migrar todos los controles crudos | pendiente |
| W3 Aplicar marca | tokens en `globals.css` + `site/assets/site.css`, tipografía, capturas | pendiente |
| W4 Icono | aplicar el icono elegido: `src-tauri/icons`, favicon, OG, manifest, cabecera app/landing | **bloqueado: elección de Miguel** |

Reglas: commits conventional sin coautor y con push; PR en inglés sin firma; CI verde en 3 SO antes de mergear; E2E con Chrome DevTools MCP (`make web` en :1431, `make site` en :8080).
