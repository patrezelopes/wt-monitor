# Security

## Reporting a vulnerability

Please report security problems privately through GitHub's **private vulnerability reporting** (the *Security* tab of the repository, "Report a vulnerability"), not in a public issue. Include the version, how to reproduce it and the impact you see.

## Threat model

wt-monitor is a local tool for a single user's machine.

**Always on (no terminals):**
- Listens on `127.0.0.1` only and rejects any `Host` header other than `127.0.0.1:<port>` / `localhost:<port>` (DNS-rebinding protection).
- `POST /event` requires the custom header `X-WT: 1`, which makes browsers send a CORS preflight that the server never approves, so other web pages cannot inject events.
- The preview reads Claude Code transcripts only for worktrees inside the watched root; paths are validated (absolute, no `..`, symlinks resolved, must contain `.git`).
- The dashboard reads `/proc` and shows processes, commands and transcript excerpts to **anyone who can reach the port**. On a machine shared with other users, other local users can see them: `127.0.0.1` is not private between users.

**Terminals (opt-in with `--terminals`):** the dashboard can start a login shell (or run `claude` in one) in a worktree. That is remote-code-execution capability for whoever is authorised, so:
- It is **off by default**; every `/api/terminals*` and `/ws/term/*` route answers 404 unless started with `--terminals` (or `WT_TERMINALS=1`).
- Authorisation is a 256-bit secret kept in a file that only you can read (`~/.config/wt-monitor/token`, mode `0600`, directory `0700`). It is **never returned by any API** — you receive it once through the fragment of a link (`/#token=…`, which browsers never send to the server) printed by `wt-monitor --print-token` / `make open-terminals`. Other local users, who can reach the port, cannot obtain it.
- Mutating requests need the token **and** the `X-WT: 1` header (preflight again); the WebSocket needs the token as its first message and an `Origin` equal to the dashboard's own, so no third-party site can open a shell.
- Only two kinds exist, `shell` and `claude`; the client cannot choose a command. The working directory must be a valid worktree inside the root.
- Only terminals **started by the panel** are managed; it never attaches to other terminals. At most 16 at a time; each is killed (SIGHUP, then SIGKILL) when closed.
- The token is compared in constant time. Anyone holding it can run commands as your user: treat it like an SSH key and rotate it by deleting the file and restarting.

**Out of scope:** attackers who already run code as your user (they can do anything), and exposing the port beyond loopback (don't: there is no TLS and no user authentication).
