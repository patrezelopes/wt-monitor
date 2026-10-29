# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

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

[0.2.0]: https://github.com/patrezelopes/wt-monitor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/patrezelopes/wt-monitor/releases/tag/v0.1.0
