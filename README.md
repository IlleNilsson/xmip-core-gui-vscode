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
  and the only route through the boundary. The symbol's name and shape are
  `xmip-core-abi`'s (`operate::XMIP_VALIDATE_ENTRYPOINT`, `ValidateFn`),
  never declared here again. It answers the cluster designer from section
  10 of the same header (`operate::design`, ADR-0064). It does not run the
  `xmip` command and does not link the runtime's crates.

## What it does

A `.toml` document is validated on open, change and save, and the runtime's
own report becomes diagnostics. The runtime reads a cluster's `xmip.toml` —
its Xmip Applications among its sections — or a node's configuration and
tells them apart itself. The command **Xmip: Validate node
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

The server audits through `xmip-core-audit` as `xmip-lsp` (ADR-0062):
`start` with the library it was told, `stop` with its exit code, a library
loaded as `load-runtime`, and as failures a refused argument (`start`), a
library that could not be loaded (`load-runtime`), a validation the runtime
could not answer (`validate`), a broken stdio stream (`serve`) and every
panic (`unhandled`). A failure the server retries on every keystroke is
recorded once per reason. The records go to `audit.toml` in
`XMIP_AUDIT_DIRECTORY`, or to the operating system's log where none is set;
the audit capability writes every one.

## A Location's settings

In a Receive or Send Location — a node's own or a bound one — the editor
completes what a Location may name and write, and explains it on hover, from
what each technology declares (ADR-0064, amendment 2026-09-26): on
`transport = "` or `contract = "`, the transports or contracts the runtime
carries; in `[receive_locations.settings]` or `[…contract_settings]`, the
settings the named technology declares for that side and the Location has not
written yet, each with its kind, default or requirement and meaning, and the
value's snippet; on a setting's line, its declaration. Every word is the
technology's own, read through the runtime's `xmip_technology_catalogue_v1`
(section 12); what the server knows is only where the cursor is. What the
declaration refuses — an unknown setting, one for the other side, a wrong
kind, a required one missing — comes back as the runtime's diagnostics.
`xmip/technologies` `{ technology? }` answers the catalogue, or one
technology's declaration, for the Location form the next slice draws
(`.src/settings.rs`).

## The cluster designer

Developers and operators work on one `xmip.toml` per cluster, its Xmip
Applications among its sections, and publishing slices it into the file each
node runs (ADR-0064 and ADR-0031, amendments 2026-10-03;
`module/platform/configure/doc/cluster-configuration.md`); a node's own file
is never edited (ADR-0031, amendment 2026-10-05). The Operation Desktop's
Configure page edits the same file through the same section 10 exports, and
slices and ships it on save (`../README.md`). That file opens
in the **Xmip cluster designer**, a custom editor over the same text:
**Open With...** on `xmip.toml`, or the command **Xmip: Open in the cluster
designer**. The text editor and the designer are two views of one document,
and a change in either shows in the other.

The designer is one view per artifact kind, navigated by kind and then
entry: Cluster, Node, Receive Port, Receive Location, Send Port, Send
Location, Send Port Group, Prepare, Promote, Demote, Route, Transformation
and Process. Each lists the entries the file holds of it — wherever it holds
them: the cluster's shared sections, a node's own under `[nodes.<name>]`, a
binding, or an Xmip Application held as `[[xmip_applications]]` — with its
values as the file writes them, each edited in place, a value added or
removed, an entry added to any list that may hold one, a node declared, and
an entry or node removed. A Route entry is an Xmip Application: its routes
drawn — Receive Locations, Subscriptions, Work Processes, Send Port Groups
and Send Ports, and what routes where — and a Subscription's filter edited
as rows of property, operator, value and kind gathered in And and Or
groups, with Not around any part, beside the filter's text, one line of
Xmip's expression language (ADR-0066). A Receive Port entry is an
Application's `[[xmip_applications.receive_ports]]`, and each Receive
Location of an Application names its `receive_port` and states its
`interaction` and `depth` (ADR-0031, amendment 2026-10-01). Prepare,
Promote, Demote and Transformation are present and say plainly that the
configuration does not define them yet; Process shows the
`[[work_processes]]` entries and says that a Process's flow is not defined
yet. Which kinds exist, which the configuration defines, where an entry is
and what an edit does are the server's: the designer holds no rule.

The webview (`extension/webview/`, vanilla TypeScript and SVG:
`designer.ts` the navigation and the entries, `routes.ts` the Route view,
`dom.ts` what both draw with) draws what the server sends and forwards what
the developer does; `extension/src/designer.ts` carries the messages and
applies the text edit the server returns; the server's four requests
(`.src/designer.rs`) each call one of the runtime's section 10 exports,
which forward to `xmip-core-configure` (`views.rs`, `view_edit.rs`):

- `xmip/views` — the file as one view per artifact kind, every Xmip
  Application's routes placed by the server from each node's place along
  the route;
- `xmip/filterStructure` and `xmip/filterText` — a filter's line as rows
  and groups, and rows as the line in its canonical form; canonical text
  comes back byte for byte;
- `xmip/edit` — set or remove a value, add or remove an entry, declare a
  node, or one of an Application's own edits (declare a Receive Location,
  a Work Process or a Send Port, add a Subscription, set its filter,
  connect it to a target); answered as the one text edit that makes the
  change, so the webview never writes TOML. An edit that would leave a node
  that read unable to read, or an Application section that read unable to,
  is refused in the reader's words.

A developer draws a route by choosing the Xmip Application under Route,
declaring its ends in the toolbar (a Receive Location and a Send Port, say),
adding a Subscription that routes to the Send Port, choosing it on the
canvas, building its filter in the rows and applying it; with a Subscription
chosen, clicking another Work Process, Send Port Group or Send Port routes it
there instead.

## Building

The server: `cargo build`, then `cargo test`. The tests cover the framing,
the diagnostics, the runtime loader, the protocol and the designer's layout
and text edits; those that need the built runtime — the library
`XMIP_RUNTIME_LIBRARY` names, else
`../../../../platform/runtime/target/debug`, the estate's own build, found
once in `.src/built.rs` — validate `../samples/xmip.toml` (the cluster the
desktop GUI edits and starts a node of, read from the repository beside this
one, never copied) and ask the designer's exports for the views, a filter's round trip
and every edit. Until
2026-09-26 they looked three levels up, where no runtime is, and skipped
without saying so to anyone who did not read the output.

The shell: `npm ci`, then `npm run compile`, `npm run lint` and `npm test`
in `extension/`, which needs Node.js — the only repository in the estate that
does. The test runs in plain node, offline, with the `vscode` API and the
language client replaced by recording stubs: it checks that activation
registers the commands and the designer and starts the server from the
settings, and that the designer carries the webview's messages to the
server and applies the text edit it answers — and nothing the server does.
`npm run compile` builds the webview with its own `webview/tsconfig.json`,
for the browser. Then point the `xmip.server.path` setting at
`target/debug/xmip-lsp` and press F5 in `extension/` to run it in an
Extension Development Host.

## Shared governance

Licensing is explicit in [LICENSE](LICENSE). Contribution, security, support,
issue and pull-request defaults are inherited from
[IlleNilsson/.github](https://github.com/IlleNilsson/.github) when they are
not overridden locally. The included workflow is manual-only and verifies the
Rust half through `IlleNilsson/.github@v1` and the TypeScript half with
`npm ci`, compile, lint and test (ADR-0052 clause 6).
