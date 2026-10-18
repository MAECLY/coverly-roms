# Coverly ROMs — developer tasks.
#
# A thin wrapper around `pnpm` and `cargo`, and the canonical entry point for
# local development. Run `make` (or `make help`) to list every target.
#
# Windows note:
#   GNU make is not bundled with Windows. Install once with one of:
#     winget install GnuWin32.Make
#     scoop install make
#     choco install make
#   Or run the underlying commands (listed under each target) directly.

SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help
.ONESHELL:
.SHELLFLAGS := -eu -o pipefail -c
MAKEFLAGS += --no-print-directory --warn-undefined-variables

# Library to audit with `make audit ROOT=/path/to/roms`.
ROOT ?=
# Extra arguments for `make cli ARGS="..."`.
ARGS ?=
# New version for `make bump VERSION=X.Y.Z`.
VERSION ?=
# Port the Vite dev server listens on (see vite.config.ts).
DEV_PORT := 1431

# `make site`: where the filled landing page goes, its port, and an optional
# GitHub release JSON to preview (e.g. scripts/site/fixtures/release.json).
SITE_OUT := target/site
SITE_PORT ?= 8080
RELEASE ?=
# `make site-covers CONTACT=/tmp/sheet.html`: also write a contact sheet.
CONTACT ?=

# Throwaway example library built by `make example` (git-ignored).
EXAMPLE_DIR := target/example
EXAMPLE_LIB := $(EXAMPLE_DIR)/library
EXAMPLE_FLAGS := --profile anbernic-stock --cache-dir $(EXAMPLE_DIR)/cache --data-dir $(EXAMPLE_DIR)/data
CLI := cargo run -q -p coverly-cli --

# ───────────────────────────────────────────────────────────────────────
# Help
# ───────────────────────────────────────────────────────────────────────

.PHONY: help
help: ## Show this help.
	@echo ""
	@echo "  Coverly ROMs — make targets"
	@echo "  ───────────────────────────"
	@grep -E '^[a-zA-Z][a-zA-Z0-9_-]*:.*## ' $(MAKEFILE_LIST) | \
	  sort | \
	  awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "  Quick start:  make install && make dev"
	@echo "  Browser only: make web   (mock backend, no Rust build)"
	@echo "  Try the CLI:  make example && make example-fill"
	@echo ""

# ───────────────────────────────────────────────────────────────────────
# Setup
# ───────────────────────────────────────────────────────────────────────

.PHONY: install
install: ## Install all frontend + Rust dependencies.
	pnpm install
	cargo fetch

.PHONY: doctor
doctor: ## Check that required toolchain pieces are installed.
	@echo "node    : $$(node --version 2>/dev/null || echo 'MISSING')"
	@echo "pnpm    : $$(pnpm --version 2>/dev/null || echo 'MISSING')"
	@echo "rustc   : $$(rustc --version 2>/dev/null || echo 'MISSING')"
	@echo "cargo   : $$(cargo --version 2>/dev/null || echo 'MISSING')"
	@echo "tauri   : $$(pnpm tauri --version 2>/dev/null | tail -n1 || echo 'MISSING')"

# ───────────────────────────────────────────────────────────────────────
# Dev loop and builds
# ───────────────────────────────────────────────────────────────────────

.PHONY: dev
dev: ## Run the full Tauri dev loop (Rust backend + Vite + WebView).
	pnpm tauri dev

.PHONY: web
web: ## Run only the web UI in a browser with the mock backend (http://localhost:1431).
	@echo "Mock scenarios: add ?mockUpdate=available|offline|no-key|up-to-date|bad-signature to the URL"
	@echo "Open folder… picks a loose ROM pile with ?mockPick=downloads|mixed"
	pnpm dev

.PHONY: stop
stop: ## Stop a dev server left listening on the Vite port.
	@pids=$$(lsof -ti tcp:$(DEV_PORT) -sTCP:LISTEN || true); \
	if [ -n "$$pids" ]; then kill $$pids && echo "stopped $$pids"; else echo "nothing listening on $(DEV_PORT)"; fi

.PHONY: build-web
build-web: ## Build only the web UI into dist/.
	pnpm build

.PHONY: preview
preview: build-web ## Serve the built web UI (mock backend).
	pnpm preview

.PHONY: build
build: ## Build a release Tauri bundle (unsigned, no updater artifacts).
	pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'

# ───────────────────────────────────────────────────────────────────────
# Quality gates
# ───────────────────────────────────────────────────────────────────────

.PHONY: test-web
test-web: ## Run frontend unit tests (vitest).
	pnpm test

.PHONY: test-rust
test-rust: ## Run Rust unit + integration tests for the whole workspace.
	cargo test --workspace

.PHONY: test
test: test-web test-rust ## Run frontend AND Rust tests.

.PHONY: test-watch
test-watch: ## Re-run frontend tests on every change.
	pnpm test:watch

.PHONY: coverage
coverage: ## Frontend test coverage report (coverage/).
	pnpm test:coverage

.PHONY: lint
lint: ## Lint TS/TSX (ESLint), types (tsc) and Rust (clippy, deny warnings).
	pnpm lint
	pnpm typecheck
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format frontend (Prettier) and Rust (rustfmt) sources.
	pnpm format
	cargo fmt --all

.PHONY: tokens
tokens: ## Regenerate src/styles/tokens.css and site/assets/tokens.css from design/tokens.json.
	pnpm tokens

.PHONY: tokens-check
tokens-check: ## Fail if the generated token CSS is stale or a contrast pair misses its target.
	pnpm tokens:check

.PHONY: check
check: ## Run every quality gate without writing (CI equivalent).
	pnpm tokens:check
	pnpm lint
	pnpm typecheck
	pnpm format:check
	pnpm test
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

.PHONY: actionlint
actionlint: ## Lint the GitHub Actions workflows (needs actionlint installed).
	actionlint

.PHONY: version-check
version-check: ## Check that Cargo.toml, package.json and Cargo.lock agree on the version.
	@scripts/bump-version.sh --check

# ───────────────────────────────────────────────────────────────────────
# CLI
# ───────────────────────────────────────────────────────────────────────

.PHONY: audit
audit: ## Audit a ROM library (read-only): make audit ROOT=/path/to/roms
	@if [ -z "$(ROOT)" ]; then echo "usage: make audit ROOT=/path/to/roms" >&2; exit 2; fi
	cargo run -p coverly-cli -- audit "$(ROOT)"

.PHONY: cli
cli: ## Run the coverly CLI: make cli ARGS="systems"
	$(CLI) $(ARGS)

.PHONY: fill
fill: ## Plan a fill without writing (dry run): make fill ROOT=/path/to/roms
	@if [ -z "$(ROOT)" ]; then echo "usage: make fill ROOT=/path/to/roms" >&2; exit 2; fi
	$(CLI) fill "$(ROOT)" --dry-run

# ───────────────────────────────────────────────────────────────────────
# Landing page (site/, published by .github/workflows/pages.yml)
# ───────────────────────────────────────────────────────────────────────

.PHONY: site
site: ## Serve the landing page on http://localhost:8080 (RELEASE=release.json to preview a release).
	node scripts/site/gen_covers.mjs --check
	rm -rf "$(SITE_OUT)"
	@if [ -n "$(RELEASE)" ]; then \
	  python3 -I scripts/site/apply_release.py --release-json "$(RELEASE)" --repo MAECLY/coverly-roms --site site --out "$(SITE_OUT)"; \
	else \
	  python3 -I scripts/site/apply_release.py --no-release --repo MAECLY/coverly-roms --site site --out "$(SITE_OUT)"; \
	fi
	@echo "Serving $(SITE_OUT) on http://localhost:$(SITE_PORT) (Ctrl+C to stop)"
	python3 -m http.server $(SITE_PORT) --bind 127.0.0.1 -d "$(SITE_OUT)"

.PHONY: site-covers
site-covers: ## Regenerate the landing cover sprite and inks (CONTACT=sheet.html also writes a review sheet).
	node scripts/site/gen_covers.mjs $(if $(CONTACT),--contact "$(CONTACT)")

.PHONY: site-og
site-og: ## Render the Open Graph cards site/assets/og.png and og-en.png (needs Chrome or Chromium).
	node scripts/site/og.mjs

.PHONY: site-check
site-check: ## Check the landing page: tokens, cover sprite, links, ES/EN parity, head tags, budgets, contrast, motion, art, live release.
	node scripts/brand/build_tokens.mjs --check
	node scripts/site/gen_covers.mjs --check
	python3 -I scripts/site/check_site.py

# ───────────────────────────────────────────────────────────────────────
# Example library (throwaway, under target/example)
# ───────────────────────────────────────────────────────────────────────

.PHONY: example
example: ## Create a small example library and audit it.
	scripts/dev/example-library.sh "$(EXAMPLE_LIB)"
	$(CLI) audit "$(EXAMPLE_LIB)" --profile anbernic-stock

.PHONY: example-fill
example-fill: ## Plan a fill of the example library (dry run, needs internet).
	@[ -d "$(EXAMPLE_LIB)" ] || $(MAKE) example
	$(CLI) fill "$(EXAMPLE_LIB)" $(EXAMPLE_FLAGS) --dry-run

.PHONY: example-apply
example-apply: ## Fill the example library for real, then audit it again.
	@[ -d "$(EXAMPLE_LIB)" ] || $(MAKE) example
	$(CLI) fill "$(EXAMPLE_LIB)" $(EXAMPLE_FLAGS) --apply
	$(CLI) audit "$(EXAMPLE_LIB)" --profile anbernic-stock

.PHONY: example-undo
example-undo: ## Undo the last fill of the example library.
	$(CLI) undo --data-dir $(EXAMPLE_DIR)/data

.PHONY: example-clean
example-clean: ## Delete the example library, its cache and its journals.
	rm -rf "$(EXAMPLE_DIR)"

# ───────────────────────────────────────────────────────────────────────
# Maintenance
# ───────────────────────────────────────────────────────────────────────

.PHONY: icons
icons: ## Regenerate every app and landing icon from design/icon/*.svg.
	pnpm icons

.PHONY: bump
bump: ## Set the version everywhere: make bump VERSION=X.Y.Z
	@if [ -z "$(VERSION)" ]; then echo "usage: make bump VERSION=X.Y.Z" >&2; exit 2; fi
	scripts/bump-version.sh "$(VERSION)"

.PHONY: clean
clean: ## Remove build output (dist/, target/, coverage/).
	rm -rf dist coverage
	cargo clean
