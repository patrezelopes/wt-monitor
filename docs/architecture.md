# Architecture

```
 Claude Code ──Stop hook──► hook/wt-agent-hook ──POST /event──┐
                                                              ▼
 /proc ──► sampler thread (1 Hz) ──► JSON snapshot ──► HTTP server ◄── browser (index.html)
                  │                                        ▲
                  └── alerts ──► notify-send               └── GET /api/preview ──► ~/.claude*/projects/**.jsonl
```

Rust standard library only. `src/main.rs` holds the sampler, alerts and HTTP server; `src/terms.rs`, `src/pty.rs` and `src/ws.rs` implement the opt-in terminals; `src/sched.rs` holds contention measurement and the opt-in priority control. The UI is `index.html` plus the vendored xterm.js, all embedded with `include_str!`.

## Sampler

Once per second, for every PID in `/proc`:

1. `readlink /proc/<pid>/cwd`; skip it unless it is inside the watched root.
2. Resolve the **worktree**: the nearest ancestor containing a `.git` entry (cached per directory).
3. Read `stat` (utime+stime, last CPU, threads), `status` (`VmRSS`, `Cpus_allowed_list`) and `cmdline`.

Per process it also sums `/proc/<pid>/task/*/schedstat` (nanoseconds running and waiting in the run queue; the file of the pid alone only covers the main thread). CPU % is the delta of utime+stime between two samples over wall time (100 % = one core, `CLK_TCK` is assumed to be 100). Per-core load comes from `/proc/stat`. Results are grouped per worktree and serialised by hand into one JSON string that `GET /api` returns.

## Contention (always on)

Per worktree, the sampler turns the deltas of run and wait time into **queue %** = wait / (run + wait): the share of the time the worktree's threads were ready to run that they spent waiting for a core. Below 20 ms of ready time per sample the figure is reported as 0 (noise). It also reports `waiting`, the average number of cores' worth of threads that waited. The system-wide view is PSI (`/proc/pressure/{cpu,memory}`, `avg10`), `null` on kernels without it. A *CPU contended* alert fires when queue % is at least `WT_WAIT_PCT` for `WT_WAIT_SECS` and at least 0.3 cores were ready.

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

## Terminals (opt-in)

Started with `--terminals`; otherwise every `/api/terminals*` and `/ws/term/*` route is 404.

- **`pty.rs`** — pseudo-terminals through direct FFI to the libc that `std` already links (`posix_openpt`, `grantpt`, `unlockpt`, `ptsname_r`, `ioctl`, `setsid`). The child runs as a session leader with the slave as its controlling terminal; the master is `O_CLOEXEC` and the slave is opened `O_NOCTTY` so the server never adopts it.
- **`ws.rs`** — RFC 6455: handshake (SHA-1 + base64, tested against the RFC vectors), masked client frames, fragmentation, ping/pong, close. Client frames must be masked; messages are capped at 1 MiB.
- **`terms.rs`** — the registry. Each terminal has a reader thread that appends to a 256 KB ring buffer and fans out to subscribers, and a waiter thread that records the exit code. Lock order is always ring → subs → exit. Attaching sends the ring first and then live output with no gap. Dead terminals are kept 10 minutes; at most 16 exist.

Routes (mutations need `X-WT: 1` + `X-WT-Token`):

| Route | Purpose |
|---|---|
| `GET /api/terminals` | `{enabled, auth}`; with a valid token also `terms: [...]`. The token is never returned. |
| `POST /api/terminals` | body `path=<worktree>&kind=shell\|claude&cols=&rows=` → `{id}` |
| `DELETE /api/terminals/<id>` | SIGHUP to the process group, SIGKILL after 2 s |
| `GET /ws/term/<id>` | WebSocket. Origin must be the dashboard's own. First message `{"token":"…"}`; then binary = keyboard, text `{"resize":[cols,rows]}`; the server sends binary output and `{"exit":N}`. |

A `shell` terminal runs `$SHELL -l` (so `PATH`, `nvm` and per-folder account switching from the user's profile apply); a `claude` terminal is the same shell with `claude\n` typed. The browser keeps one xterm.js instance per terminal (`TERM.panes`), reconnects after drops, and resets and replays on reconnect. The token lives in `~/.config/wt-monitor/token` (`0600`) and reaches the browser once, through the `#token=` fragment. See [SECURITY.md](../SECURITY.md).

## Priority control (opt-in)

Started with `--control` (or `WT_CONTROL=1`); otherwise `/api/control*` answers 404 and `/api` reports `control.enabled: false`. It shares the terminals' token (`terms::authed`).

- **Where.** At startup `sched.rs` finds the `user@UID.service` ancestor of its own cgroup (from `/proc/self/cgroup`) and probes it by creating and removing a test cgroup. systemd delegates `cpu` and `memory` there, so the panel can create children without privileges. If that fails, `control.supported` is `false` with a `reason` and nothing is touched.
- **Desired vs applied.** `POST /api/control` (`path`, and any of `cpu=baixa|normal|alta`, `high=<GiB>`, `low=<GiB>`; 0 clears) and `POST /api/control/focus` (`path`, empty to clear) only update the desired state under a mutex. The **sampler thread** applies it every cycle (`Control::enforce`): a worktree whose effective setting is not the default gets a cgroup `wtm-<fnv64 of path>` created next to `app.slice` (so it competes as a peer of your other apps), `cpu.weight`, `memory.high` and `memory.low` written when they change, and every process of the worktree moved in through `cgroup.procs` (the panel's own pid is never moved). Focus overrides the CPU level: the focused worktree gets weight 1000, the others 20.
- **Reverting.** For every process moved, the cgroup it came from is remembered. Returning to normal writes each pid back there, resets the files to neutral values and removes the empty cgroup. Leftovers of a previous run are reset at startup; those that still hold processes stay with neutral weights and are reused.
- **Terminals.** `Terms::create` calls `Control::on_spawn(pid)` right after the fork and before `claude` is typed, so a terminal starts inside its worktree's cgroup and `memory.high` covers everything it launches.
- **Never `memory.max`.** The only memory knobs are `memory.high` (throttle) and `memory.low` (protect); exceeding a limit slows a worktree, never kills it. Memory a process already allocated is not charged to a cgroup it joins, so limits cover future growth for existing processes.
- The JSON carries `control` (`enabled`, `supported`, `reason`, `mem_low`, `focus`) at the top and `ctl` (`cpu`, `want`, `weight`, `high`, `low`, `inside`, `mem`, `throttled`, `err`) per worktree. The browser shows them as tags on each row and in the drawer's *Priority* tab.

## Security

See the README. The server only binds to loopback and validates `Host`; mutation requires a header that browsers will not send cross-origin without a preflight that the server never approves.
