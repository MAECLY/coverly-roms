# Integrations registry

`integrations.json` lists every service Coverly talks to on the user's behalf; it feeds the «Integraciones» settings tab, the settings search, the notices and `coverly integrations`.

1. Add one entry to `integrations.json` (`id` in kebab-case; `help_url` only the service's official account or API page, on the host of its credit).
2. Add or reuse its credit in `credits/credits.json` and list the id in `credits`.
3. Add its copy under `integrations.service.<id>.*` in `locales/es/ui.json` and `locales/en/ui.json`.
4. Planned services take `needs` and `roadmap`, never `fields` or `test` (no keychain key until the service works).
5. Run `cargo test -p coverly-core integrations` and `pnpm test src/lib/integrations`.
