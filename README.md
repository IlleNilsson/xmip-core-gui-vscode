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
extension passes from its `xmip.runtime.library` setting, required (the
owner, 2026-10-22); with none named, every validation says where to point
it — the full path of `xmip_core_runtime.dll`, `libxmip_core_runtime.so` or
`libxmip_core_runtime.dylib`, which `cargo build` leaves in
`module/platform/runtime/target/debug`. The server finds no library on its own: runtime
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
`contract = "`, the contracts the runtime carries; in
`[receive_locations.settings]`, the settings of the transport the
Location's address chooses by its scheme (amendment 2026-10-28), and in
`[…contract_settings]` those of the named contract, each for that side and the Location has not
written yet, each with its kind, default or requirement and meaning, and the
value's snippet; on a setting's line, its declaration. Every word is the
technology's own, read through the runtime's `xmip_technology_catalogue_v1`
(section 12); what the server knows is only where the cursor is. What the
declaration refuses — an unknown setting, one for the other side, a wrong
kind, a required one missing — comes back as the runtime's diagnostics.
`xmip/technologies` `{ technology? }` answers the catalogue, or one
technology's declaration, for the Location form the next slice draws
(`.src/settings.rs`).

## Editing the cluster's xmip.toml

The owner, 2026-10-13, trying the extension: *Nice attempt, but not really
usefull VS Code extension* — it must *Create, Edit, Delete. Suggestions,
Lookups, Validations* (ADR-0064, amendment 2026-10-13). In the text editor
over the cluster's `xmip.toml`:

- **Suggestions** (Ctrl+Space, and on `"`, `[`, `.`, `=`, `{`, `,`): on a
  line opening with `[`, every header the file may open, a node's tables
  for each node it declares; on an empty line, the keys the table may hold
  and does not write yet, each with its kind and whether it is required,
  inserted with a value of its kind; after `=`, the names a reference may
  take (a Receive Location's `receive_port`, a Subscription's
  `destination = { send-port = "…" }`, a Send Port Group's `send_ports`, a
  binding's `name`, a bound Location's `name` and `node`), a choice's words
  (`interaction`, `depth`, `failover`, `on_failure`, …), the role
  words a node's `roles` takes, `true` or `false`; in a `filter`, the
  properties the file's filters compare and the operators a filter row
  offers; in `[tuning]`, the runtime's tuning keys; a Location's contract
  and settings as below.
- **Lookups**: F12 on a reference goes to what it names; Shift+F12 on a
  name or a reference lists every place that names it; hover on a key says
  its kind, whether it is required, its words and what it names, on a
  reference what it names and that entry's values, on a header or name the
  entry and how often it is named; the Outline lists every entry within the
  one it belongs to; F2 on a name or a reference renames the entry and every
  name standing for it.
- **Create, rename, delete**: Ctrl+. in an entry offers *Rename*, *Delete*,
  *Delete … and the names that stand for it* where anything names it, and
  *Add …* for each list of the entry or table the cursor is in (*Add
  Subscription to …* in an Xmip Application, *Declare a node* at the top);
  the commands **Xmip: Add to the cluster's xmip.toml**, **Xmip: Rename in
  the cluster's xmip.toml** and **Xmip: Delete from the cluster's xmip.toml**
  do the same from a list of every place or entry. A plain delete of
  something still named is refused, naming each reference that holds it.
- **Validations**: the runtime's report on every change, as before; Ctrl+.
  on a problem offers its fix where one is determinable — a key no reader
  takes, changed to the nearest key that is taken; a key a table cannot be
  without, inserted with its kind's empty value to fill in; a name nothing
  is declared under, changed to each name that is.

Every list, key, kind and word is the runtime's: `xmip_cluster_views_v1`'s
answer — `xmip-core-configure`'s views, `written`, `references` and `keys`,
and the runtime's `[tuning]` declaration — and `xmip_role_words_v1`; every
change is `xmip_cluster_edit_v1`'s (`rename`, `remove-entry`, `refer`,
`rename-key`, `set`, `add-entry`, `add-node`, an Application's
`add-subscription`), answered as the one text edit that makes it
(`.src/model.rs`, `completion.rs`, `lookup.rs`, `action.rs`,
`quickfix.rs`, `edit.rs`, dispatched by `language.rs`). The extension's
`src/commands.ts` asks the developer only what the server sends it to ask
(`xmip/places`, `xmip/entries`) and runs the server's commands
(`xmip.server.add`, `xmip.server.rename`, `xmip.server.edit`), which the
language client registers. A Receive Location's `accept.party` suggests the
Parties `[[parties]]` declares; a call to a compiled unit suggests its
module, the units of that module that fit, and in its `settings` what its
unit declares; and a Port's or Location's `prepare`, `transform`, `promote`
and `demote` suggest the calls whose unit serves its stage (ADR-0066,
amendment 2026-10-22).

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
entry: Cluster, Node, Party, Module, Receive Port, Receive Location, Send
Port, Send Location, Send Port Group, Prepare, Promote, Demote, Route,
Transformation and Work Process. Each lists the entries the file holds of
it — wherever it holds them: the cluster's shared sections, a node's own
under `[nodes.<name>]`, a binding, or an Xmip Application held as
`[[xmip_applications]]` — with its values as the file writes them, each
edited in place, a value added or removed, an entry added to any list that
may hold one, a node declared, and an entry or node removed. What may be
set on an entry is the runtime's offer: each key its table may hold and
does not write, with its kind, whether it is required and the words or
names its value may take, picked from a list; a Party's identities are
added one at a time, each its mechanism, purpose and value, and a new
Party's `id` is minted by the runtime as it is added. A Route entry is an Xmip Application: its routes
drawn — Receive Locations, Subscriptions, Work Processes, Send Port Groups
and Send Ports, and what routes where — and a Subscription's filter edited
as rows of property, operator, value and kind gathered in And and Or
groups, with Not around any part, beside the filter's text, one line of
Xmip's expression language (ADR-0066). A Receive Port entry is an
Application's `[[xmip_applications.receive_ports]]`, and each Receive
Location of an Application names its `receive_port` and states its
`interaction` and `depth` (ADR-0031, amendment 2026-10-01). Prepare,
Promote, Demote and Transformation list the calls to compiled units —
`[[preparation_steps]]`, `[[promotions]]`, `[[demotions]]` and
`[[transformations]]`, each a module, a unit and its settings — and Module
the `[[modules]]` whose manifests declare those units, each with the
stages it serves and `offline = true`; Work Process lists the
`[[work_processes]]`, each naming the unit that is its flow (ADR-0066,
amendment 2026-10-22). Which kinds exist, where an entry is, what may be
set on it and what an edit does are the runtime's: the designer holds no
rule.

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
  connect it to a target), or add an element to an array of unnamed
  tables (`add-element`, a Party's identity); answered as the one text edit
  that makes the change, so the webview never writes TOML. An edit that would leave a node
  that read unable to read, or an Application section that read unable to,
  is refused in the reader's words.

A developer draws a route by choosing the Xmip Application under Route,
declaring its ends in the toolbar (a Receive Location and a Send Port, say),
adding a Subscription that routes to the Send Port, choosing it on the
canvas, building its filter in the rows and applying it; with a Subscription
chosen, clicking another Work Process, Send Port Group or Send Port routes it
there instead.

## The Work Process designer

A Work Process is designed in VS Code and, at the end of the day, is a
compiled piece executed when called upon (the owner, 2026-10-22; ADR-0066,
amendment 2026-10-22). A `.bpmn` file opens in the **Xmip Work Process
designer**, the default editor for it: a BPMN 2.0 diagram, saved as standard
BPMN 2.0 XML with its BPMN DI, so every other BPMN tool opens what is drawn
here and a design drawn elsewhere opens here. The text editor and the
designer are two views of one document.

The palette adds each step the design format offers — start, end, task,
a call to a unit, publish, wait for a Message, wait for a time, decide,
split or join, subprocess, and on an activity an error catch or a time-out;
a step is dragged to move it, **Connect** draws a flow from one step to the
next, and the properties panel sets a step's name, a flow's condition in
Xmip's expression language, the module and unit a service task calls, a
receive task's batch, the Correlation a step takes its Message by and a
timer's duration. What keeps the design from compiling is shown on the step
it names. **Compile** asks for the unit's module name and writes the Work
Process unit beside the design, in a folder of that name: its crate's
`Cargo.toml` and `src/lib.rs`, built into the library a node loads through
the module boundary, and `module.toml`, its `[[modules]]` entry for the
cluster's `xmip.toml`.

It holds no rule. The webview (`extension/webview/process.ts`, TypeScript
and SVG) draws what the server sends and forwards what the developer does;
`extension/src/process.ts` carries the messages, applies the text edit the
server returns and writes what a compile answers; the server's three
requests (`.src/process.rs`) are answered by the server itself
(`.src/process_design.rs`), through the design formats it carries — a
design format is a technology, which the runtime's library, a platform
service, never names — forwarding to the design format,
`xmip-core-process-bpmn` (`diagram.rs`, `edit.rs`), and to
`xmip-core-process` (`generate.rs`):

- `xmip/process` — the design as drawn: every step with its kind, name,
  BPMN DI bounds and what Xmip adds to it, every flow with its condition
  and waypoints, the problems by element, the kinds the palette offers;
- `xmip/processEdit` — add, move, connect, rename, a condition, a default
  flow, an Xmip extension, a timer, remove; made in place in the XML, the
  rest of the design byte for byte, and answered as the one text edit that
  makes it, so the webview never writes XML;
- `xmip/processCompile` — the design compiled, the format it names or BPMN
  2.0, into its unit's crate and module entry, or every refusal.

## Building

The server: `cargo build`, then `cargo test`. The tests cover the framing,
the diagnostics, the runtime loader, the protocol and the designer's layout
and text edits; those that need the built runtime — the library
`XMIP_RUNTIME_LIBRARY` names, else
`../../../../platform/runtime/target/debug`, the estate's own build, found
once in `.src/built.rs` — validate `../samples/xmip.toml` (the cluster the
desktop GUI edits and starts a node of, read from the repository beside this
one, never copied), ask every editing request of the test cluster's
`xmip.toml` (the estate's `test/xmip.toml`, or the file `XMIP_TEST_CLUSTER`
names, its nodes' names read from it through the runtime, never written)
with an Xmip Application added to it — suggestions, definition,
references, hover, the outline, rename, code actions, each command and
each quick fix — and ask the designer's exports for the views, a filter's round trip
and every edit. Until
2026-09-26 they looked three levels up, where no runtime is, and skipped
without saying so to anyone who did not read the output.

The shell is built with no Node.js (the owner, 2026-10-22; ADR-0014,
amendment 2026-10-22): `Build-XmipService -Extension VsCode`, from the
estate's PowerShell module, restores the packages it compiles against and
runs with from `extension/package-lock.json` — each tarball checked against
the lock's integrity — compiles the shell and its webview (`tsconfig.json`,
`webview/tsconfig.json`, for the browser) with TypeScript 7's native
compiler, `prerequisite.toml`'s `[archive.typescript]`, and zips the VSIX
by PowerShell into `.local-work/package/vscode`: `extension.vsixmanifest`,
`[Content_Types].xml` and `extension/`. No vsce. The landing compiles the
same way and says SKIPPED where the compiler is absent. The shell is
TypeScript, its ESLint configuration too (`eslint.config.ts`); the estate
holds no hand-written JavaScript (the root's `test/JavaScript.Test.ps1`).
Nothing the extension runs is fetched: the webview loads only its own files,
and no schema, catalogue, icon, font or package is reached for at run time.

Every rule is the Rust server's and tested in Rust. The shell's lint and its
tests run in this repository's CI alone (`.github/workflows/verify.yml`),
where Node.js lives in a throwaway runner: `npm ci`, `npm run compile` with
TypeScript 6, which the latest typescript-eslint still needs, `npm run lint`
and `npm test`, which runs in plain node with the `vscode` API and the
language client replaced by recording stubs — that activation registers the
commands and the designer and starts the server from the settings, that the
designer carries the webview's messages to the server and applies the text
edit it answers, and that its page reaches nothing online. Locally they are
SKIPPED. Then point the `xmip.server.path` setting at
`target/debug/xmip-lsp` and install the VSIX (`code --install-extension`).
## Shared governance

Licensing is explicit in [LICENSE](LICENSE). Contribution, security, support,
issue and pull-request defaults are inherited from
[IlleNilsson/.github](https://github.com/IlleNilsson/.github) when they are
not overridden locally. The included workflow is manual-only and verifies the
Rust half through `IlleNilsson/.github@v1` and the TypeScript half with
`npm ci`, compile, lint and test (ADR-0052 clause 6).
