.PHONY: help build build-rust build-core build-cli build-rust-frontend build-examples build-ts build-go \
       test test-quick test-standard test-full test-e2e \
       check-tooling check-rust check-ts check-go check-docs check-meta check-all check-fast pre-completion \
       clean lint walkthrough

# -- Build -------------------------------------------------------------------

build: build-rust build-ts build-go ## Build everything (Rust + TS + Go)

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

build-rust: build-core build-cli build-rust-frontend build-examples ## Build all Rust crates

build-core: ## Build shatter-core library
	cargo build -p shatter-core

build-cli: ## Build shatter CLI binary
	cargo build -p shatter-cli

build-rust-frontend: ## Build Rust frontend + runtime
	cargo build --manifest-path shatter-rust/Cargo.toml
	cargo build --manifest-path shatter-rust-runtime/Cargo.toml

build-examples: ## Build example targets (Rust + WASM generators)
	cargo build --manifest-path examples/rust/Cargo.toml
	cargo build --manifest-path examples/generators/wasm-rust/Cargo.toml
	cargo build --manifest-path examples/generators/wasm-adversarial/Cargo.toml

build-ts: ## Build TypeScript frontend
	cd shatter-ts && npm install --silent && npm run build

build-go: ## Build Go frontend
	cd shatter-go && go build ./...

# -- Test tiers (see CLAUDE.md) -----------------------------------------------

test: test-full ## Run full test suite (default)

test-quick: build ## Quick: cargo test (or nextest if available)
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		echo "[info] Using cargo-nextest for parallel test execution"; \
		cargo nextest run; \
	else \
		echo "[info] Using cargo test (install cargo-nextest for faster runs)"; \
		cargo test; \
	fi

test-standard: build ## Standard: cargo test + clippy
	cargo test
	cargo clippy -- -D warnings

test-full: build ## Full: all languages + clippy
	cargo test
	cargo clippy -- -D warnings
	cd shatter-ts && npm test
	cd shatter-go && go test ./...

test-e2e: build ## E2E: concolic pipeline tests only
	cargo test --test e2e_concolic

# -- Quality scripts ----------------------------------------------------------

check-tooling: ## Report available required and optional analysis tools
	./scripts/quality/check-tooling.sh

check-rust: ## Run Rust quality gates
	./scripts/quality/check-rust.sh

check-ts: ## Run TypeScript quality gates
	./scripts/quality/check-ts.sh

check-go: ## Run Go quality gates
	./scripts/quality/check-go.sh

check-docs: ## Run documentation quality gates
	./scripts/quality/check-docs.sh

check-meta: ## Run repository meta checks (workflow lint, Semgrep)
	./scripts/quality/check-meta.sh

check-all: ## Run the full aggregate quality script
	./scripts/quality/check-all.sh

check-fast: ## Run the fast quality gate (clippy + tests, skip docs/schemas/meta)
	./scripts/quality/check-all.sh --fast

pre-completion: ## Run the pre-completion quality script
	./scripts/quality/pre-completion.sh

# -- Other --------------------------------------------------------------------

lint: ## Run linters (clippy + golangci-lint)
	cargo clippy -- -D warnings
	cd shatter-go && golangci-lint run ./...

walkthrough: build ## Run the demo walkthrough
	bash demo/walkthrough.sh --auto --delay 0

clean: ## Remove all build artifacts
	cargo clean
	rm -rf shatter-ts/dist shatter-ts/node_modules
	cd shatter-go && go clean ./...
