# Example skills and agents

Generic, ready-to-copy versions of three roles that work well with Claude Code and that wt-monitor makes visible while they run. They are adapted from real use, with everything project-specific removed. Edit them to match your own process.

| File | Kind | What it does |
|---|---|---|
| [`skills/po-intake/SKILL.md`](skills/po-intake/SKILL.md) | skill | Turns a vague request into a verifiable spec (asking only what changes the work), then a spec plus plan into small, traceable tasks. |
| [`agents/agile-coach.md`](agents/agile-coach.md) | agent | The one role that writes to the task board: moves cards, keeps titles and descriptions accurate, never writes code. |
| [`skills/agent-orchestration/SKILL.md`](skills/agent-orchestration/SKILL.md) + [`agents/orchestrator.md`](agents/orchestrator.md) | skill + agent | Coordinates implementers (one Git worktree each, in parallel), reviewers and QA, with claims and a state table. The coordinator never edits code. |

## Install

Per project:

```bash
mkdir -p .claude/skills .claude/agents
cp -r examples/skills/po-intake examples/skills/agent-orchestration .claude/skills/
cp examples/agents/*.md .claude/agents/
```

Or once for all your projects, copy them under `~/.claude/skills/` and `~/.claude/agents/` (with several Claude config directories, copy into each).

Skills are picked up by their `description`; agents are launched with the `Agent` tool by their `name`. Both files are plain Markdown, so adapting them is just editing text.

## What you will see in wt-monitor

- Running `po-intake` or `agent-orchestration` shows the skill's name on the worktree's row and in the Arena.
- The orchestrator launching implementers appears as sub-agents on the worktree, with their type, and the messages between them in the *Agents* tab of the session drawer.
- One worktree per task is exactly the unit wt-monitor groups by, so the CPU, memory and queue time of each parallel implementer show up separately, and with `--control` you can give the urgent one priority or put it in **Focus**.

## Things to adapt

- `agent-orchestration` assumes prompts under `docs/prompts/` and a claims file; point them at your own.
- `agile-coach` assumes a wrapper script in front of your tracker (`scripts/board.sh` is a placeholder). It deliberately has no direct API access.
- `po-intake` refers to a spec template; use your own, or drop that line.
