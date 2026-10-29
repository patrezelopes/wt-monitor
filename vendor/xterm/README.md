# Vendored: xterm.js

The terminal emulator used by the **Terminals** view. It is embedded in the binary (`include_str!`), so the dashboard makes no network requests.

| File | Package | Version | License |
|---|---|---|---|
| `xterm.js`, `xterm.css` | [`@xterm/xterm`](https://www.npmjs.com/package/@xterm/xterm) | 5.5.0 | MIT ([LICENSE-xterm.txt](LICENSE-xterm.txt)) |
| `addon-fit.js` | [`@xterm/addon-fit`](https://www.npmjs.com/package/@xterm/addon-fit) | 0.10.0 | MIT ([LICENSE-addon-fit.txt](LICENSE-addon-fit.txt)) |

Files are taken unmodified from the npm packages, except that the trailing `//# sourceMappingURL=…` comment is removed from the two scripts (the source maps are not shipped).

SHA-256 of the files **as published on npm** (before removing that comment):

```
1f991ac3b4b283ebf96e60ae23a00a52765dd3a2e46fa6fdda9f1aab032f7495  lib/xterm.js
bdaefa370b1bfc42ee88d46fe6072400902a4d4b2d45cd93438dda9b23c97089  lib/addon-fit.js
ba8e6985669488981ccf40c0cefe3aba80722cb6c92de7ad628b0bd717faf2b6  css/xterm.css
```

To update: `npm pack @xterm/xterm@<version> @xterm/addon-fit@<version>`, extract each tarball into its own empty directory, copy `lib/xterm.js`, `lib/addon-fit.js`, `css/xterm.css` and the licenses here, drop the `sourceMappingURL` line, update this table and the hashes, then run `make check e2e`.
