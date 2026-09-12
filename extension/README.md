# Xmip

Validates an Xmip node configuration through the runtime itself, as you edit
it. Diagnostics appear on open, change and save; **Xmip: Validate node
configuration** prints the runtime's whole report to the Xmip output channel.

The extension is a shell around `xmip-lsp`, a language server that loads the
runtime's native library and calls the same validation the Xmip desktop tool
calls. Point `xmip.server.path` at the server and `xmip.runtime.library` at
the runtime, or leave the second empty and set `XMIP_RUNTIME_LIBRARY`.

Source and license: https://github.com/IlleNilsson/xmip-core-gui-vscode
