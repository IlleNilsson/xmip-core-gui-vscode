// The designer (ADR-0064, amendment 2026-10-03): a custom editor over the
// cluster's one xmip.toml, a view per artifact kind. It holds no rule. The
// webview draws what xmip-lsp answers and forwards what the developer does;
// the server asks the runtime's library, and xmip-core-configure says what
// the file means and how an edit changes the text. This file only carries
// messages between the two and applies the text edit the server returns, so
// the text editor and the designer are two views of one document and stay
// in step both ways.

import { randomBytes } from "node:crypto";
import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

/** The custom editor's view type, as package.json contributes it. */
export const viewType = "xmip.clusterDesigner";

/** What the webview sends. */
type FromView =
  | { type: "ready" }
  | { type: "edit"; edit: unknown }
  | { type: "filterText"; structure: unknown }
  | { type: "filterStructure"; filter: string };

interface Position {
  line: number;
  character: number;
}

/** What `xmip/edit` answers: the one text edit that makes the change. */
export interface TextEdit {
  range: { start: Position; end: Position };
  newText: string;
}

export class ClusterDesigner implements vscode.CustomTextEditorProvider {
  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly client: () => LanguageClient | undefined,
  ) {}

  resolveCustomTextEditor(
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
  ): void {
    const webview = panel.webview;
    const scripts = vscode.Uri.joinPath(this.extensionUri, "out", "webview");
    const media = vscode.Uri.joinPath(this.extensionUri, "media");
    webview.options = { enableScripts: true, localResourceRoots: [scripts, media] };
    webview.html = page(
      webview.cspSource,
      webview.asWebviewUri(vscode.Uri.joinPath(scripts, "designer.js")).toString(),
      webview.asWebviewUri(vscode.Uri.joinPath(media, "designer.css")).toString(),
    );

    const changed = vscode.workspace.onDidChangeTextDocument((change) => {
      if (change.document.uri.toString() === document.uri.toString()) {
        void this.views(document, webview);
      }
    });
    panel.onDidDispose(() => {
      changed.dispose();
    });
    webview.onDidReceiveMessage((message: FromView) => {
      void this.received(document, webview, message);
    });
  }

  /** One message from the webview, answered by the server. */
  async received(
    document: vscode.TextDocument,
    webview: vscode.Webview,
    message: FromView,
  ): Promise<void> {
    switch (message.type) {
      case "ready":
        return this.views(document, webview);
      case "edit":
        return this.edit(document, webview, message.edit);
      case "filterText":
        return this.ask(webview, "xmip/filterText", { structure: message.structure }, "filterText");
      case "filterStructure":
        return this.ask(webview, "xmip/filterStructure", { filter: message.filter }, "filterStructure");
    }
  }

  /** The cluster's file, view by view, as the server answers it. */
  private async views(document: vscode.TextDocument, webview: vscode.Webview): Promise<void> {
    await this.ask(webview, "xmip/views", request(document), "views");
  }

  /** An edit the webview asked for, made to the text as the server says. */
  private async edit(
    document: vscode.TextDocument,
    webview: vscode.Webview,
    edit: unknown,
  ): Promise<void> {
    const version = document.version;
    const answer = await this.request<TextEdit>(webview, "xmip/edit", { ...request(document), edit });
    if (answer === undefined) {
      return;
    }
    if (document.version !== version) {
      await webview.postMessage({ type: "refused", reason: "The text changed meanwhile; do it again." });
      return;
    }

    const { start, end } = answer.range;
    const change = new vscode.WorkspaceEdit();
    change.replace(
      document.uri,
      new vscode.Range(start.line, start.character, end.line, end.character),
      answer.newText,
    );
    await vscode.workspace.applyEdit(change);
  }

  /** Ask the server and post what it answered to the webview as `type`. */
  private async ask(
    webview: vscode.Webview,
    method: string,
    params: object,
    type: string,
  ): Promise<void> {
    const answer = await this.request<unknown>(webview, method, params);
    if (answer !== undefined) {
      await webview.postMessage({ type, answer });
    }
  }

  /** The server's answer, or undefined after the refusal was posted. */
  private async request<T>(
    webview: vscode.Webview,
    method: string,
    params: object,
  ): Promise<T | undefined> {
    const client = this.client();
    if (client === undefined) {
      await webview.postMessage({ type: "refused", reason: "xmip-lsp is not running." });
      return undefined;
    }
    try {
      return await client.sendRequest<T>(method, params);
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      await webview.postMessage({ type: "refused", reason });
      return undefined;
    }
  }
}

/** The document as a request names it: its uri, and its text as it is now. */
function request(document: vscode.TextDocument): object {
  return { textDocument: { uri: document.uri.toString() }, text: document.getText() };
}

/**
 * The webview's page: a module script and a stylesheet, nothing inline. The
 * script loads by nonce, and the modules it imports from the extension's own
 * webview folder by the webview's source.
 */
export function page(source: string, script: string, style: string): string {
  const nonce = randomBytes(16).toString("base64");
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${source}; script-src 'nonce-${nonce}' ${source};">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<link rel="stylesheet" href="${style}">
<title>Xmip cluster designer</title>
</head>
<body>
<header id="bar"><h1 id="title">Xmip cluster</h1></header>
<p id="said" role="status"></p>
<main><nav aria-label="Artifacts"><ul id="kinds"></ul></nav><section id="view"></section></main>
<script type="module" nonce="${nonce}" src="${script}"></script>
</body>
</html>`;
}
