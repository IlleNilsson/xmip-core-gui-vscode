# Xmip

Validates a cluster's `xmip.toml`, its Xmip Applications among its
sections, or an Xmip node configuration through the runtime itself, as you
edit it. Diagnostics appear
on open, change and save; **Xmip: Validate node configuration** prints the
runtime's whole report to the Xmip output channel.

A cluster's one `xmip.toml` also opens in the **Xmip cluster designer**
(**Open With...**, or **Xmip: Open in the cluster designer**): a view per
artifact — Cluster, Node, Party, Module, Receive Port, Receive Location,
Send Port, Send Location, Send Port Group, Prepare, Promote, Demote, Route,
Transformation, Work Process — chosen by kind, then entry, each entry's
values edited in place and what may be set on it offered.
A Route is an Xmip Application held in the file: its routes drawn, and a
Subscription's filter edited as rows of property, operator and value in And
and Or groups, with its one line of Xmip's expression language shown as
text (`MessageType = 'Order' and not Amount > 1000`). Prepare, Promote,
Demote and Transformation are calls to compiled units a Module declares.
The designer and the text editor are two views of one document.

A `.bpmn` file opens in the **Xmip Work Process designer**: a Work Process
drawn in BPMN 2.0 and saved as standard BPMN 2.0 XML with its diagram, so
other BPMN tools open it. Add steps from the palette, drag them, connect
them, and set a step's unit call, batch, correlation set or timer and a
flow's condition in the properties panel; what keeps the design from
compiling is shown on its step. **Compile** writes the Work Process unit
beside the design: the crate a node loads once built, and its `[[modules]]`
entry.

The extension is a shell around `xmip-lsp`, a language server that loads the
runtime's native library and calls the same validation the Xmip desktop tool
calls. Point `xmip.server.path` at the server and `xmip.runtime.library`,
required, at the runtime's library — the full path of
`xmip_core_runtime.dll`, `libxmip_core_runtime.so` or
`libxmip_core_runtime.dylib`; the server looks for no runtime on its own.
Nothing is fetched at run time: everything the extension uses is in its
package.

Source and license: https://github.com/IlleNilsson/xmip-core-gui-vscode
