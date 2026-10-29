# Contributing

Thanks for helping! The project is intentionally tiny: Rust standard library only and a single HTML file with no build step. Please keep it that way unless there is a strong reason.

```bash
make check     # fmt-check + clippy (-D warnings) + tests + page-script syntax check: what CI runs
make run       # serve ~/dev on :8765   (make help lists every target)
```

- **UI work:** `make run`, then open `/?demo` (or `/?demo&mode=fun`) to iterate with fake data. `make js-check` syntax-checks the page script. Respect `prefers-reduced-motion`.
- **Server work:** keep it dependency-free, Linux-only (`/proc`), and loopback-only. Anything that reads files or accepts input must validate its path/headers (see `preview` and `serve`).
- **Screenshots** in `docs/img/` are generated from the demo data (never from real projects) with `make screenshots` (overview dark/light, arena and the session drawer).
- **Hook work:** test `install-hooks.py` against a throw-away directory and a fake `HOME` rather than your real settings: `HOME=$(mktemp -d) make hooks CONFIG_DIRS=$HOME/cfg` (create `cfg/` first).
- Open an issue before large changes. Commit messages in the imperative mood, small focused PRs.

By contributing you agree your work is released under the MIT License.
