# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/patrezelopes/wt-monitor/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/patrezelopes/wt-monitor/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/patrezelopes/wt-monitor/releases/tag/v0.1.0
