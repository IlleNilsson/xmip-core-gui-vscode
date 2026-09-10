// The shell of the VS Code extension, the developer's face of the
// configuration tool (ADR-0014, amendment 2026-09-10). It starts xmip-lsp,
// hands it the runtime's path, and shows what comes back. Nothing here
// validates anything: that is the server's, through the runtime's C ABI.
//
// This is the one place in the estate where TypeScript lives (the same
// amendment). Anything that could be done in the Rust server is done there.

import * as vscode from "vscode";
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

/** What `xmip/validate` answers: the runtime's own report, whole. */
interface Validation {
  status: number;
  valid: boolean;
  report: string;
  runtime: string;
}

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("Xmip");
  context.subscriptions.push(output);

  context.subscriptions.push(
    vscode.commands.registerCommand("xmip.validate", () => validate(output)),
    vscode.workspace.onDidChangeConfiguration(async (change) => {
      if (change.affectsConfiguration("xmip")) {
        await start(output);
      }
    }),
  );

  await start(output);
}

export async function deactivate(): Promise<void> {
  await client?.stop();
  client = undefined;
}

/** Start the server from the current settings; stop the one running first. */
async function start(output: vscode.OutputChannel): Promise<void> {
  await client?.stop();

  const settings = vscode.workspace.getConfiguration("xmip");
  const command = settings.get<string>("server.path", "xmip-lsp");
  const runtime = settings.get<string>("runtime.library", "");

  const server: ServerOptions = {
    command,
    args: runtime === "" ? [] : ["--runtime", runtime],
  };
  const options: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", pattern: "**/*.toml" }],
    outputChannel: output,
  };

  client = new LanguageClient("xmip", "Xmip", server, options);
  await client.start();
}

/** The command: validate the active document on demand and print the report. */
async function validate(output: vscode.OutputChannel): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (editor === undefined || client === undefined) {
    void vscode.window.showInformationMessage("Xmip: open a node configuration first.");
    return;
  }

  const document = editor.document;
  try {
    const validation = await client.sendRequest<Validation>("xmip/validate", {
      textDocument: { uri: document.uri.toString() },
      text: document.getText(),
    });
    const outcome = validation.valid ? "VALID" : "INVALID";
    output.appendLine(`${outcome} ${document.fileName} (runtime ${validation.runtime})`);
    output.appendLine(validation.report);
  } catch (error) {
    output.appendLine(`REFUSED ${document.fileName}: ${String(error)}`);
  }
  output.show(true);
}
