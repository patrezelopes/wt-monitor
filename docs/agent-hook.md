# Agent hook

`hook/wt-agent-hook` is a [Claude Code hook](https://code.claude.com/docs/en/hooks) installed for three events. It tells them apart by `hook_event_name`:

| Event | Matcher | What it sends |
|---|---|---|
| `Stop` | — | The agent finished responding: its last message (`last_assistant_message`, or the last assistant text from the transcript tail). Shown in the "Agents" card and as a *finished* notification. |
| `Notification` | `permission_prompt\|elicitation_dialog` | The agent is waiting for you (`kind: ask`). Claude Code only sends `permission_prompt` after the prompt has waited about 6 s, and not at all if you answer first. |
| `PreToolUse` | `AskUserQuestion` | The agent asked you a question (`kind: ask`), but only if it is still unanswered after `WT_ASK_DELAY` seconds (default 10). The hook forks a detached process to wait and returns at once, so the agent is never delayed; the child checks the transcript for the question's `tool_result`. |

Everything goes to `http://127.0.0.1:$WT_PORT/event` (default 8765) with a 2 s timeout. Any failure is swallowed and the exit code is always 0 with no output, so it never blocks, approves or denies anything.

## Install / uninstall

```bash
make hooks                           # same as: python3 install-hooks.py
make hooks CONFIG_DIRS=~/.claude     # only these config dirs
make hooks-uninstall                 # same as: python3 install-hooks.py --uninstall
```

Without `CONFIG_DIRS` it targets `~/.claude` and every `~/.claude-*` directory. For each one it:

1. copies the script to `~/.local/bin/wt-agent-hook`;
2. backs up `settings.json` to `settings.json.bak-wt-monitor`;
3. appends an entry for `Stop`, `Notification` and `PreToolUse` (with the matchers above) running `~/.local/bin/wt-agent-hook` with a 5 s timeout, skipping the events where it is already present. Run it again after upgrading to add new events.

`make hooks-uninstall` removes only wt-monitor's entries (other hooks are left alone), saves `settings.json.bak-wt-monitor-uninstall`, and — when run without `CONFIG_DIRS` — deletes `~/.local/bin/wt-agent-hook`. Both commands are idempotent.

## Test

```bash
make hook-test        # posts a fake event to the dashboard on PORT (default 8765)
```

The message appears in the "Agents" card. Note `Stop` fires after every completed response; if that is too chatty, filter in the hook (for example by response duration).
