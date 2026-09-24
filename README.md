# xmip-core-gui-vscode

The VS Code extension: the developer's face of the configuration tool, whose
operator's face is the MAUI desktop application in xmip-core-gui. ADR-0014,
amendment 2026-09-10. A technology of xmip-core-gui, mounted at
`module/core/operation/gui/vscode`.

Two parts, and the second is the substance:

- `extension/` — a TypeScript shell. It starts the server, hands it the
  runtime's path, and shows what comes back. This is the one place in the
  estate where TypeScript or JavaScript lives, and it stays as thin as the
  VS Code extension host allows.
- `.src/` — `xmip-lsp`, a Rust language server over stdio. It loads the
  runtime's native library and calls `xmip_validate_v1` through
  `xmip_operate.h`: the same call the desktop GUI makes through `Xmip.Abi`,
  and the only route through the boundary. It does not run the `xmip`
  command and does not link the runtime's crates.

## What it does

A `.toml` document is validated on open, change and save, and the runtime's
own report becomes diagnostics. The command **Xmip: Validate node
configuration** (`xmip/validate` to the server) prints the whole report to the
Xmip output channel, with the runtime that produced it.

The runtime library is the one the `--runtime <path>` flag names, which the
extension passes from its `xmip.runtime.library` setting; with none named,
every validation says so. The server finds no library on its own: runtime
discovery is one rule, the .NET surfaces' `RuntimeLibrary`, because a surface
has to find the runtime before it can call anything in it, and the copy of
that rule this server kept in Rust went on 2026-09-24 (ADR-0052, amendment of
that date). The server loads the library the first time a
document needs validating and tries again on every validation until it
succeeds, so the runtime may be built after the editor is open.

## Building

The server: `cargo build`, then `cargo test`. The tests cover the framing,
the diagnostics, the runtime loader and the protocol, and two of them
validate `../samples/edge-01.xmip.toml` — the node the desktop GUI starts,
read from the repository beside this one, never copied — through the built
runtime.

The shell: `npm ci`, then `npm run compile`, `npm run lint` and `npm test`
in `extension/`, which needs Node.js — the only repository in the estate that
does. The test runs in plain node, offline, with the `vscode` API and the
language client replaced by recording stubs: it checks that activation
registers the command and starts the server from the settings, and nothing
the server does. Then point the `xmip.server.path` setting at
`target/debug/xmip-lsp` and press F5 in `extension/` to run it in an
Extension Development Host.

## Shared governance

Licensing is explicit in [LICENSE](LICENSE). Contribution, security, support,
issue and pull-request defaults are inherited from
[IlleNilsson/.github](https://github.com/IlleNilsson/.github) when they are
not overridden locally. The included workflow is manual-only and verifies the
Rust half through `IlleNilsson/.github@v1` and the TypeScript half with
`npm ci`, compile, lint and test (ADR-0052 clause 6).
