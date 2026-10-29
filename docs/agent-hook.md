# Agent hook

`hook/wt-agent-hook` is a [Claude Code hook](https://docs.claude.com/en/docs/claude-code/hooks) for the **`Stop`** event, which fires when an agent finishes responding.

It reads the hook payload from stdin (`cwd`, `session_id`, `transcript_path`, and `last_assistant_message` when the Claude Code version provides it), falls back to reading the last assistant text from the transcript tail, and sends it to `http://127.0.0.1:$WT_PORT/event` (default 8765) with a 2 s timeout. Any failure is swallowed and the exit code is always 0, so it can never block or slow down an agent.

## Install / uninstall

```bash
make hooks                           # same as: python3 install-hooks.py
make hooks CONFIG_DIRS=~/.claude     # only these config dirs
make hooks-uninstall                 # same as: python3 install-hooks.py --uninstall
```

Without `CONFIG_DIRS` it targets `~/.claude` and every `~/.claude-*` directory. For each one it:

1. copies the script to `~/.local/bin/wt-agent-hook`;
2. backs up `settings.json` to `settings.json.bak-wt-monitor`;
3. appends `{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"~/.local/bin/wt-agent-hook","timeout":5}]}]}}` — skipping directories where it is already present.

`make hooks-uninstall` removes only wt-monitor's entry (other hooks are left alone), saves `settings.json.bak-wt-monitor-uninstall`, and — when run without `CONFIG_DIRS` — deletes `~/.local/bin/wt-agent-hook`. Both commands are idempotent.

## Test

```bash
make hook-test        # posts a fake event to the dashboard on PORT (default 8765)
```

The message appears in the "Agents" card. Note `Stop` fires after every completed response; if that is too chatty, filter in the hook (for example by response duration).
