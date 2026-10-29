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

## Preview

`GET /api/preview?path=<worktree>` validates that the path is absolute, inside the root, free of `..`, and contains `.git`. It then finds the newest `*.jsonl` transcript in `~/.claude*/projects/<path with every non-alphanumeric char replaced by '-'>/` (falling back to sub-directories of it), and returns the last ≤ 60 complete lines (≤ 400 KB tail) as JSON-escaped strings. Parsing of the transcript happens in the browser, so the server needs no JSON parser.

## UI

A single page, no build step, no libraries. Worktree cards are reconciled by key (`sync()`), so focus and CSS animations survive the 1 Hz refresh. Sober and playful views share the same data and are separate render functions. `?demo` replaces `fetch` with a client-side generator of fake data.

## Security

See the README. The server only binds to loopback and validates `Host`; mutation requires a header that browsers will not send cross-origin without a preflight that the server never approves.
