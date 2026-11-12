# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **Terminals without an active worktree.** The Terminals tab always offers the watched root (`(raiz) name`) as a destination, so you can open a shell or Claude there before any worktree exists. `POST /api/terminals` accepts the root as `path`; anything else outside a worktree is still refused. Terminals in the root are not placed in a worktree cgroup.
- **External notifications (opt-in, `WT_NOTIFY_CMD`).** Every notification can also run a command of your choice, with the event in `WT_EVENT`, `WT_WORKTREE`, `WT_TITLE`, `WT_BODY`, `WT_URGENT` and `WT_NOTIFY_JSON`; `WT_NOTIFY_EVENTS` picks which events (`done`, `cpu`, `wait`, `mem`, `sysmem`). It runs in the background and is killed after 10 s. The command only comes from the environment, and nothing is sent anywhere unless it is set.
- **"Waiting for you" notifications.** The agent hook now also handles `Notification` (`permission_prompt`, `elicitation_dialog`: Claude Code only sends it after the prompt has waited ~6 s) and `PreToolUse` for `AskUserQuestion` (notifies only if the question is still unanswered after `WT_ASK_DELAY` seconds, default 10, checked in a detached process so the agent is never delayed). They raise a desktop notification and the `ask` event for `WT_NOTIFY_CMD`. Run `make hooks` again to add the new entries.
- `contrib/home-assistant`: a script that posts the event to a Home Assistant webhook and an automation that announces it on an Echo through Alexa Media Player.

## [0.4.0] - 2026-11-01

### Added
- **Resume a session in a new terminal.** A *Resume session* button in the session drawer opens a terminal and runs `claude --resume <id>` with the most recent conversation of that worktree, even if Claude has already exited. `POST /api/terminals` accepts `kind=resume`; the session id is read from the transcript file name on the server (never from the request) and checked to be hex and dashes only.
- **Open a worktree in your editor.** An *Open in code* button in the session drawer launches VS Code (or Cursor, VSCodium, Windsurf, Zed, or `WT_EDITOR`) on the worktree. `GET/POST /api/open`, guarded by the same token as the terminals; the command never comes from the request.
- **CPU contention, always on.** Each worktree shows its *queue %* (the share of its ready-to-run time spent waiting for a core, from `/proc/<pid>/task/*/schedstat`) and the dashboard shows system CPU/memory pressure (PSI). New alert *CPU contended* (`WT_WAIT_PCT`, `WT_WAIT_SECS`).
- **Priority control (opt-in, `--control` / `WT_CONTROL=1`, `make install CONTROL=1`).** A *Priority* tab in the session drawer: CPU level low / normal / high (`cpu.weight` 25 / 100 / 400), **Focus** on one worktree (1000 against 20 for the others), a soft memory limit (`memory.high`) and memory protection (`memory.low`), through cgroup v2 in the slice systemd delegates to the user. Reversible: processes go back to the cgroup they came from. Nothing is ever killed (`memory.max` is never set). Terminals opened by the panel are born inside their worktree's cgroup, so memory limits cover everything they start.
- `GET /api/control`, `POST /api/control`, `POST /api/control/focus`, guarded by the same token as the terminals.
- `make open-control`, `scripts/e2e-control.mjs` (moves real processes between cgroups and checks that high priority wins a shared core) and browser tests for the Priority tab.
- `examples/`: generic skills and agents to copy into your projects: PO intake, Agile coach and agent orchestration (skill + orchestrator agent).

### Changed
- **`make install` now enables terminals and priority control by default.** Pass `TERMINALS=` and/or `CONTROL=` (empty) to install without them. `make run` is unchanged (both off unless asked).
- **Activity tab is readable.** Agent and user messages render Markdown (bold, italics, lists, headings, tables, code blocks, links), and Claude Code's XML-like tags (`<task-notification>`, `<system-reminder>`…) become labelled cards with the status as a badge. One-line spots (finished-task list, sub-agent conversation, Arena bubbles) show the text without markup. Everything is escaped before it becomes HTML.
- CPU is shown as a percentage of the whole machine (100% = all cores) instead of per-core percentages above 100%, with the raw cores in the tooltip.
- The session drawer header no longer wraps the close button on long worktree names.

## [0.3.1] - 2026-10-29

### Added
- README section "How is this different?" positioning wt-monitor as a lightweight observer next to workspace and orchestrator tools.

### Documentation
- The README now documents `--print-token`, `WT_TERMINALS`, `WT_TOKEN_FILE`, the `mode=term` and `layout` URL parameters, and links the changelog and the third-party notices.

### Fixed
- The `Makefile` finds `cargo` in `~/.cargo/bin`, so `make` works from shells where rustup's directory is not on `PATH`.

## [0.3.0] - 2026-10-29

### Added
- **Terminals started by the panel (opt-in).** A new *Terminals* view runs real terminals (a login shell, or `claude` in one) in a worktree, rendered with xterm.js. Open several, group them by worktree, show 1, 2 or 4 side by side, close and reconnect (the last 256 KB of output is replayed). Buttons in the session drawer open a terminal or Claude directly in that worktree. Start with `--terminals` / `make run TERMINALS=1` / `make install TERMINALS=1`.
- `wt-monitor --print-token` and `make open-terminals`: the authorisation secret lives in a `0600` file and is delivered to the browser once, through the fragment of a link.
- `make e2e`: end-to-end tests for the terminals over HTTP/WebSocket and in a real headless Chrome; run by CI.
- `SECURITY.md` with the threat model, and `THIRD_PARTY_NOTICES.md`.

### Notes
- Still **zero Rust dependencies**: the PTY uses the libc that `std` already links, and WebSocket (handshake, SHA-1, base64, framing) is implemented in `src/ws.rs` with the RFC 6455 test vectors. The UI embeds xterm.js 5.5.0 (MIT) under `vendor/xterm/`; it makes no network requests.
- Terminals are **off by default**. See [SECURITY.md](SECURITY.md) before enabling them.

## [0.2.0] - 2026-10-29

### Added
- **Agents, skills and sub-agents.** For worktrees running Claude Code the dashboard now shows the main agent and each running sub-agent (type, model, current tool), the skill in use, and who is talking to whom (delegations, `SendMessage`, completions), all derived from the session transcripts.
- **Derived worktree status**: *working*, *waiting for you*, *idle* or *at the limit*.
- **Session drawer** with tabs (overview, agents, activity, processes), opened by clicking a worktree. The agents tab draws the agent topology with message packets travelling between agents.
- **Branch** of each worktree (read from `.git/HEAD`), status filters, and search by worktree, branch, agent or skill (`Ctrl K`).
- **Light theme** (follows the system; `?theme=dark|light` overrides it).
- `GET /api/preview` accepts `n`, `b` and `subs=1`; `/api` groups carry `branch`.
- `?demo` now simulates agents, skills and sub-agents; `make screenshots` regenerates the four documentation images.
- `Makefile` covering build, run, quality checks, the systemd user service and the Claude Code hook (`make help`); `make hooks-uninstall`; a CI workflow that runs `make check`.
- Documentation: README in English and Portuguese, `docs/architecture.md`, `docs/agent-hook.md`, `CONTRIBUTING.md`.

### Changed
- **The UI was redesigned**: a dense table-based *Overview* and a refined *Arena* (geometric characters instead of emoji, sub-agents wired to the main agent). No webfonts or network requests; Inter / JetBrains Mono are used when installed.
- Installation moved from `install.sh` to `make install`; the systemd unit is generated from `contrib/wt-monitor.service.in`.
- `install-hooks.py` discovers every `~/.claude-*` directory instead of a fixed list.

### Fixed
- The activity feed and agent detection ignore `api-request-blob` transcript lines, which duplicate request payloads and produced repeated messages.

### Notes
- Agent, skill and *waiting for you* detection is inferred from transcript files, whose format is not a stable API; treat it as best-effort. See [docs/architecture.md](docs/architecture.md#status).

## [0.1.0] - 2026-10-28

### Added
- Initial release: per-worktree CPU, memory, threads and CPU affinity from `/proc`; alerts with desktop notifications; Claude Code `Stop` hook for "task finished" events; session preview; pins; playful view; systemd user service.

[Unreleased]: https://github.com/patrezelopes/wt-monitor/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/patrezelopes/wt-monitor/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/patrezelopes/wt-monitor/releases/tag/v0.1.0
