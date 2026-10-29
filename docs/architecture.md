# Architecture

```
 Claude Code ──Stop hook──► hook/wt-agent-hook ──POST /event──┐
                                                              ▼
 /proc ──► sampler thread (1 Hz) ──► JSON snapshot ──► HTTP server ◄── browser (index.html)
                  │                                        ▲
                  └── alerts ──► notify-send               └── GET /api/preview ──► ~/.claude*/projects/**.jsonl
```

Everything is in `src/main.rs` (std only) plus `index.html`, embedded with `include_str!`.

## Sampler

Once per second, for every PID in `/proc`:

1. `readlink /proc/<pid>/cwd`; skip it unless it is inside the watched root.
2. Resolve the **worktree**: the nearest ancestor containing a `.git` entry (cached per directory).
3. Read `stat` (utime+stime, last CPU, threads), `status` (`VmRSS`, `Cpus_allowed_list`) and `cmdline`.

CPU % is the delta of utime+stime between two samples over wall time (100 % = one core, `CLK_TCK` is assumed to be 100). Per-core load comes from `/proc/stat`. Results are grouped per worktree and serialised by hand into one JSON string that `GET /api` returns.

## Alerts

`Alerts::check` evaluates a `Rule` per sample. A rule fires after its condition holds for `hold`, once, then re-arms when the condition clears; a 2-minute cool-down prevents flapping. Firing appends to the in-memory alert log and spawns `notify-send`.

## Agent events

`POST /event` (header `X-WT: 1` required) takes a plain-text body:

```
cwd: /path/to/worktree
session: abcd1234

<last agent message>
```

The server maps `cwd` to a worktree, stores the event (last 30), and notifies. The UI shows them in the "Agents" card and under the worktree, and counts them as XP in the playful view.

## Preview and agent data

`GET /api/preview?path=<worktree>[&n=60][&b=400000][&subs=1]` validates that the path is absolute, inside the root, free of `..`, and contains `.git`. It then finds the newest `*.jsonl` transcript in `~/.claude*/projects/<path with every non-alphanumeric char replaced by '-'>/` (falling back to sub-directories of it), and returns the last `n` complete lines (default 60, max 400; reading at most `b` bytes from the end, max 800 KB) as JSON-escaped strings. With `subs=1` it also returns up to 8 sub-agents modified in the last 2 hours, read from `<session>/subagents/agent-<id>.jsonl` and `.meta.json` (agent type, description, model, `finished`), each with the tail of its transcript. **Parsing happens in the browser**, so the server needs no JSON parser.

Each group in `/api` also carries the worktree's `branch`, read from `.git/HEAD` (or the `gitdir:` a linked worktree points to).

### What the browser derives from a transcript {#status}

From the main transcript tail and the sub-agent tails, `derive()` extracts:

- **skill** – the latest `Skill` tool call; shown as *active* for 15 minutes;
- **delegations** – `Agent`/`Task` tool calls (main → sub-agent type);
- **messages** – `SendMessage` calls, from the main agent or from a sub-agent;
- **completions** – `<task-notification>` messages (sub-agent → main, with `status` and `summary`);
- **sub-agent state** – *running* when its meta says `finished: false` and its file changed in the last 5 minutes; its current tool is its last `tool_use`;
- **main-agent state** – from the last entry: assistant text (turn ended), assistant `tool_use` (waiting for a tool), or user/tool result (processing).

The worktree **status** is then: *at the limit* if CPU or memory is over its alert threshold; else *working* if a sub-agent is running or CPU ≥ 15 %; *waiting for you* if the turn ended (up to 30 min) or a tool has been pending for more than 90 s (likely a permission prompt); *idle* otherwise. Worktrees without a Claude process are *working* when CPU ≥ 5 %. This is a heuristic over an undocumented file format.

The browser polls one worktree per second (each every 5 s, the open one every 2 s) and renders rows by key so focus and animations survive refreshes.

## UI

A single page, no build step, no libraries, no network requests. Rows and arena stations are reconciled by key (`sync()`) and cell contents are only rewritten when they change (`setHTML()`), so focus and CSS animations survive the 1 Hz refresh. *Overview* and *Arena* share the same data and are separate render functions; the session drawer is shared. Colours are CSS custom properties (dark by default, light via `prefers-color-scheme` or `?theme=`). `?demo` replaces `fetch` with a client-side generator of fake data, including sub-agents and skills.

## Security

See the README. The server only binds to loopback and validates `Host`; mutation requires a header that browsers will not send cross-origin without a preflight that the server never approves.
