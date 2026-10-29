# wt-monitor: developer and operator tasks. Run `make help`.

ROOT     ?= $(HOME)/dev
PORT     ?= 8765
PREFIX   ?= $(HOME)/.local
BIN      := $(PREFIX)/bin
UNIT_DIR ?= $(HOME)/.config/systemd/user
SERVICE  := wt-monitor.service
# Claude Code config dirs for `make hooks` (default: ~/.claude and every ~/.claude-*)
CONFIG_DIRS ?=

.DEFAULT_GOAL := help
.PHONY: help build run open test fmt fmt-check lint js-check check \
        install uninstall restart status logs linger \
        hooks hooks-uninstall hook-test screenshots clean

help: ## Show this help
	@echo "Usage: make <target> [ROOT=$(ROOT)] [PORT=$(PORT)] [PREFIX=$(PREFIX)] [CONFIG_DIRS=...]"
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_-]+:.*## / { printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# ---- build & run ----------------------------------------------------------

build: ## Build the optimized binary
	cargo build --release

run: ## Run in the foreground on ROOT/PORT (Ctrl+C to stop)
	cargo run --release -- $(ROOT) $(PORT)

open: ## Open the dashboard in the default browser
	xdg-open http://127.0.0.1:$(PORT)

# ---- quality ----------------------------------------------------------------

test: ## Run unit tests
	cargo test

fmt: ## Format the code
	cargo fmt

fmt-check: ## Check formatting
	cargo fmt --check

lint: ## Run clippy (warnings are errors)
	cargo clippy --all-targets -- -D warnings

js-check: ## Syntax-check the page script (needs node)
	scripts/js-check.sh

check: fmt-check lint test js-check ## Everything CI runs

# ---- systemd user service ---------------------------------------------------

install: build ## Build, install to PREFIX/bin and enable+start the user service
	install -d $(BIN) $(UNIT_DIR)
	install -m755 target/release/wt-monitor $(BIN)/wt-monitor.new
	mv $(BIN)/wt-monitor.new $(BIN)/wt-monitor
	sed -e 's|@BIN@|$(BIN)|' -e 's|@ROOT@|$(ROOT)|' -e 's|@PORT@|$(PORT)|' contrib/$(SERVICE).in > $(UNIT_DIR)/$(SERVICE)
	systemctl --user daemon-reload
	systemctl --user enable $(SERVICE)
	systemctl --user restart $(SERVICE)
	@echo "wt-monitor running at http://127.0.0.1:$(PORT) (watching $(ROOT))"
	@echo "Agent notifications: make hooks   |   start before login: make linger"

uninstall: ## Stop and remove the service and binary (hooks: make hooks-uninstall)
	-systemctl --user disable --now $(SERVICE)
	rm -f $(UNIT_DIR)/$(SERVICE) $(BIN)/wt-monitor
	systemctl --user daemon-reload
	@echo "Removed. The Claude Code hook is separate: make hooks-uninstall"

restart: ## Restart the service
	systemctl --user restart $(SERVICE)

status: ## Show the service status
	systemctl --user status $(SERVICE) --no-pager

logs: ## Follow the service logs
	journalctl --user -u $(SERVICE) -n 50 -f

linger: ## Start the service at boot, before you log in
	loginctl enable-linger $(USER)

# ---- Claude Code agent hook -------------------------------------------------

hooks: ## Install the Stop hook into Claude Code settings (backs them up first)
	python3 install-hooks.py $(CONFIG_DIRS)

hooks-uninstall: ## Remove the Stop hook from Claude Code settings
	python3 install-hooks.py --uninstall $(CONFIG_DIRS)

hook-test: ## Send a fake "task finished" event to the running dashboard
	printf '{"cwd":"%s","session_id":"make0000","last_assistant_message":"Test event from make hook-test"}' "$(CURDIR)" | WT_PORT=$(PORT) python3 hook/wt-agent-hook

# ---- docs & housekeeping ----------------------------------------------------

screenshots: build ## Regenerate docs/img/*.png from the demo data (needs Chrome)
	scripts/screenshots.sh

clean: ## Remove build artifacts
	cargo clean
