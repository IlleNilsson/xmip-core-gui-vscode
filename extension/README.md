# Xmip

Validates an Xmip node configuration or Xmip Application through the runtime
itself, as you edit it. Diagnostics appear on open, change and save; **Xmip: Validate node
configuration** prints the runtime's whole report to the Xmip output channel.

An Xmip Application also opens in the **Xmip Application designer**
(**Open With...**, or **Xmip: Open in the Application designer**): its
routes drawn, and a Subscription's filter edited as rows of property,
operator and value in And and Or groups, with its one line of Xmip's
expression language shown as text (`MessageType = 'Order' and not Amount >
1000`). The designer and the text editor are two views of one document.

The extension is a shell around `xmip-lsp`, a language server that loads the
runtime's native library and calls the same validation the Xmip desktop tool
calls. Point `xmip.server.path` at the server and `xmip.runtime.library` at
the runtime; the server looks for no runtime on its own.

Source and license: https://github.com/IlleNilsson/xmip-core-gui-vscode
