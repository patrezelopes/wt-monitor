# Contributing

Thanks for helping! The project is intentionally tiny: Rust standard library only and a single HTML file with no build step. Please keep it that way unless there is a strong reason.

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run --release -- ~/dev 8765
```

- **UI work:** run the server and open `/?demo` (or `/?demo&mode=fun`) to iterate with fake data. Syntax-check the page script with `node --check` after extracting it, and respect `prefers-reduced-motion`.
- **Server work:** keep it dependency-free, Linux-only (`/proc`), and loopback-only. Anything that reads files or accepts input must validate its path/headers (see `preview` and `serve`).
- **Screenshots** in `docs/img/` are generated from the demo data (never from real projects):
  `google-chrome --headless=new --window-size=1440,1000 --virtual-time-budget=5000 --screenshot=docs/img/fun.png 'http://127.0.0.1:8765/?demo&mode=fun'`
- Open an issue before large changes. Commit messages in the imperative mood, small focused PRs.

By contributing you agree your work is released under the MIT License.
