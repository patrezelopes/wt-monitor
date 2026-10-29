# wt-monitor: developer and operator tasks. Run `make help`.

# rustup installs cargo in ~/.cargo/bin, which non-login shells often don't have on PATH
export PATH := $(HOME)/.cargo/bin:$(PATH)

ROOT     ?= $(HOME)/dev
PORT     ?= 8765
PREFIX   ?= $(HOME)/.local
BIN      := $(PREFIX)/bin
UNIT_DIR ?= $(HOME)/.config/systemd/user
SERVICE  := wt-monitor.service
# Claude Code config dirs for `make hooks` (default: ~/.claude and every ~/.claude-*)
CONFIG_DIRS ?=
# Set TERMINALS=1 to let the dashboard open terminals (opt-in; see SECURITY.md)
# Set CONTROL=1 to let the dashboard change CPU/memory priority per worktree via cgroups (opt-in; see SECURITY.md)
FLAGS := $(if $(TERMINALS), --terminals)$(if $(CONTROL), --control)
# `make install` turns both opt-in features ON unless you pass TERMINALS= and/or CONTROL= (empty) to turn them off
INSTALL_TERMINALS = $(if $(filter undefined,$(origin TERMINALS)),1,$(TERMINALS))
INSTALL_CONTROL   = $(if $(filter undefined,$(origin CONTROL)),1,$(CONTROL))
INSTALL_FLAGS     = $(if $(INSTALL_TERMINALS), --terminals)$(if $(INSTALL_CONTROL), --control)

.DEFAULT_GOAL := help
.PHONY: help build run open open-terminals open-control test fmt fmt-check lint js-check check e2e \
        install uninstall restart status logs linger \
        hooks hooks-uninstall hook-test screenshots clean

help: ## Show this help
	@echo "Usage: make <target> [ROOT=$(ROOT)] [PORT=$(PORT)] [PREFIX=$(PREFIX)] [CONFIG_DIRS=...]"
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# ---- build & run ----------------------------------------------------------

build: ## Build the optimized binary
	cargo build --release

run: ## Run in the foreground on ROOT/PORT (TERMINALS=1 / CONTROL=1 enable the opt-in features)
	cargo run --release -- $(ROOT) $(PORT)$(FLAGS)

open: ## Open the dashboard in the default browser
	xdg-open http://127.0.0.1:$(PORT)

open-terminals: build ## Open the dashboard with the one-time terminal token in the link
	xdg-open "http://127.0.0.1:$(PORT)/#token=$$(target/release/wt-monitor --print-token)"

open-control: open-terminals ## Same link as open-terminals: the priority controls use the same token

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

check: fmt-check lint test js-check ## Static checks and unit tests

e2e: build ## End-to-end tests: terminals, priority control (real cgroups) and a real Chrome (needs node)
	node scripts/e2e-terminals.mjs
	node scripts/e2e-control.mjs
	node scripts/e2e-browser.mjs

# ---- systemd user service ---------------------------------------------------

install: build ## Build, install and enable+start the user service (terminals+control ON; TERMINALS= CONTROL= to turn off)
	install -d $(BIN) $(UNIT_DIR)
	install -m755 target/release/wt-monitor $(BIN)/wt-monitor.new
	mv $(BIN)/wt-monitor.new $(BIN)/wt-monitor
	sed -e 's|@BIN@|$(BIN)|' -e 's|@ROOT@|$(ROOT)|' -e 's|@PORT@|$(PORT)|' -e 's|@FLAGS@|$(INSTALL_FLAGS)|' contrib/$(SERVICE).in > $(UNIT_DIR)/$(SERVICE)
	systemctl --user daemon-reload
	systemctl --user enable $(SERVICE)
	systemctl --user restart $(SERVICE)
	@echo "wt-monitor running at http://127.0.0.1:$(PORT) (watching $(ROOT))"
	@echo "Agent notifications: make hooks   |   start before login: make linger"
	@if [ -n "$(INSTALL_TERMINALS)$(INSTALL_CONTROL)" ]; then echo "Opt-in features ENABLED:$(INSTALL_FLAGS). Open once with: make open-terminals (the token is shared)"; fi
	@if [ -n "$(INSTALL_CONTROL)" ]; then echo "Priority control is ON: restarting the service resets every priority to normal."; fi
	@echo "To install without them: make install TERMINALS= CONTROL="

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
