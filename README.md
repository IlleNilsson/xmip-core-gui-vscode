# xmip-core-gui-vscode

The VS Code extension: the developer's face of the configuration tool, whose
operator's face is the MAUI desktop application in xmip-core-gui. ADR-0014,
amendment 2026-09-10. A technology of xmip-core-gui, mounted at
`module/operation/gui/vscode`.

Two parts, and the second is the substance:

- `extension/` — a TypeScript shell. It starts the server, hands it the
  runtime's path, and shows what comes back. This is the one place in the
  estate where TypeScript or JavaScript lives, and it stays as thin as the
  VS Code extension host allows.
- `src/` — `xmip-lsp`, a Rust language server over stdio. It loads the
  runtime's native library and calls `xmip_validate_v1` through
  `xmip_operate.h`: the same call the desktop GUI makes through `Xmip.Abi`,
  and the only route through the boundary. It does not run the `xmip`
  command and does not link the runtime's crates.

## What it does

A `.toml` document is validated on open, change and save, and the runtime's
own report becomes diagnostics. The command **Xmip: Validate node
configuration** (`xmip/validate` to the server) prints the whole report to the
Xmip output channel, with the runtime that produced it.

Where the runtime library is comes from, in order: the `--runtime <path>`
flag, the `XMIP_RUNTIME_LIBRARY` variable, the library's name beside the
server binary. The extension passes the flag from its `xmip.runtime.library`
setting when that is set. The server loads the library the first time a
document needs validating and tries again on every validation until it
succeeds, so the runtime may be built after the editor is open.

## Building

The server: `cargo build`, then `cargo test`. The tests cover the framing,
the diagnostics, the runtime loader and the protocol, and two of them
validate a sample node through the built runtime.

The shell: `npm install` and `npm run compile` in `extension/`, which needs
Node.js — the only repository in the estate that does. Then point the
`xmip.server.path` setting at `target/debug/xmip-lsp` and press F5 in
`extension/` to run it in an Extension Development Host. The shell has no
logic of its own to test; its correctness is the compile and the server's
tests.

## Shared governance

Licensing is explicit in [LICENSE](LICENSE). Contribution, security, support,
issue and pull-request defaults are inherited from
[IlleNilsson/.github](https://github.com/IlleNilsson/.github) when they are
not overridden locally. The included workflow is manual-only and verifies the
Rust half through `IlleNilsson/.github@v1`.
