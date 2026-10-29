# Agent hook

`hook/wt-agent-hook` is a [Claude Code hook](https://docs.claude.com/en/docs/claude-code/hooks) for the **`Stop`** event, which fires when an agent finishes responding.

It reads the hook payload from stdin (`cwd`, `session_id`, `transcript_path`, and `last_assistant_message` when the Claude Code version provides it), falls back to reading the last assistant text from the transcript tail, and sends it to `http://127.0.0.1:$WT_PORT/event` (default 8765) with a 2 s timeout. Any failure is swallowed and the exit code is always 0, so it can never block or slow down an agent.

## Install / uninstall

```bash
python3 install-hooks.py [config-dir ...]
```

Without arguments it targets `~/.claude` and every `~/.claude-*` directory. For each one it:

1. copies the script to `~/.local/bin/wt-agent-hook`;
2. backs up `settings.json` to `settings.json.bak-wt-monitor`;
3. appends `{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"~/.local/bin/wt-agent-hook","timeout":5}]}]}}` — skipping directories where it is already present.

To uninstall, remove that entry from `settings.json` (or restore the backup) and delete `~/.local/bin/wt-agent-hook`.

## Manual test

```bash
printf '{"cwd":"%s","session_id":"abcdef123456","last_assistant_message":"hello"}' "$PWD" | ./hook/wt-agent-hook
```

The message appears in the "Agents" card. Note `Stop` fires after every completed response; if that is too chatty, filter in the hook (for example by response duration).
