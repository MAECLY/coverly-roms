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
	  awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "  Quick start:  make install && make dev"
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

.PHONY: lint
lint: ## Lint TS/TSX (ESLint), types (tsc) and Rust (clippy, deny warnings).
	pnpm lint
	pnpm typecheck
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format frontend (Prettier) and Rust (rustfmt) sources.
	pnpm format
	cargo fmt --all

.PHONY: check
check: ## Run every quality gate without writing (CI equivalent).
	pnpm lint
	pnpm typecheck
	pnpm format:check
	pnpm test
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

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
