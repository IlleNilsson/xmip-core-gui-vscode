// The Work Process designer (ADR-0066, amendment 2026-10-22): a custom
// editor over a .bpmn design, drawn as BPMN 2.0 and saved as standard BPMN
// 2.0 XML with its BPMN DI, so every other BPMN tool opens it. It holds no
// rule. The webview draws what xmip-lsp answers and forwards what the
// developer does; the server asks the runtime's library, and the design
// format says what the design means, how an edit changes the text and what
// it compiles to. This file carries messages, applies the text edit the
// server returns, and writes the Work Process unit a compile answers beside
// the design.

import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";
import { type TextEdit, applied, asked, request } from "./asking.js";
import { page } from "./designer.js";

/** The custom editor's view type, as package.json contributes it. */
export const processViewType = "xmip.processDesigner";

/** What the webview sends. */
type FromView = { type: "ready" } | { type: "edit"; edit: unknown } | { type: "compile" };

/** What `xmip/processCompile` answers: the unit's crate and its module entry. */
interface Compiled {
  cargo: string;
  library: string;
  module: string;
  crate: string;
}

/** The Work Process designer's page: its bar, status line, palette, canvas and panel. */
const PROCESS_BODY = `<header id="bar"><h1 id="title">Xmip Work Process</h1>
<button id="compile" type="button">Compile</button></header>
<p id="said" role="status"></p>
<main class="process"><nav id="palette" aria-label="Steps"></nav>
<section id="canvas-holder"></section><aside id="properties"></aside></main>`;

export class ProcessDesigner implements vscode.CustomTextEditorProvider {
  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly client: () => LanguageClient | undefined,
  ) {}

  resolveCustomTextEditor(document: vscode.TextDocument, panel: vscode.WebviewPanel): void {
    const webview = panel.webview;
    const scripts = vscode.Uri.joinPath(this.extensionUri, "out", "webview");
    const media = vscode.Uri.joinPath(this.extensionUri, "media");
    webview.options = { enableScripts: true, localResourceRoots: [scripts, media] };
    webview.html = page(
      webview.cspSource,
      webview.asWebviewUri(vscode.Uri.joinPath(scripts, "process.js")).toString(),
      webview.asWebviewUri(vscode.Uri.joinPath(media, "process.css")).toString(),
      "Xmip Work Process designer",
      PROCESS_BODY,
    );

    const changed = vscode.workspace.onDidChangeTextDocument((change) => {
      if (change.document.uri.toString() === document.uri.toString()) {
        void this.drawn(document, webview);
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
        return this.drawn(document, webview);
      case "edit":
        return this.edit(document, webview, message.edit);
      case "compile":
        return this.compile(document, webview);
    }
  }

  /** The design as the server draws it. */
  private async drawn(document: vscode.TextDocument, webview: vscode.Webview): Promise<void> {
    const answer = await this.request<unknown>(webview, "xmip/process", request(document));
    if (answer !== undefined) {
      await webview.postMessage({ type: "diagram", answer });
    }
  }

  /** An edit the webview asked for, made to the text as the server says. */
  private async edit(
    document: vscode.TextDocument,
    webview: vscode.Webview,
    edit: unknown,
  ): Promise<void> {
    const version = document.version;
    const params = { ...request(document), edit };
    const answer = await this.request<TextEdit>(webview, "xmip/processEdit", params);
    if (answer !== undefined) {
      await applied(document, webview, version, answer);
    }
  }

  /**
   * The design compiled into its Work Process unit, written beside it in a
   * folder of the module's name: the crate's Cargo.toml and src/lib.rs, and
   * module.toml, the [[modules]] entry for the cluster's xmip.toml.
   */
  private async compile(document: vscode.TextDocument, webview: vscode.Webview): Promise<void> {
    const module = await vscode.window.showInputBox({
      prompt: "The Work Process unit's module name",
      placeHolder: "orders-flow",
    });
    if (module === undefined || module.trim() === "") {
      return;
    }
    const design = document.uri.path.split("/").pop() ?? "design.bpmn";
    const params = { ...request(document), design, module };
    const compiled = await this.request<Compiled>(webview, "xmip/processCompile", params);
    if (compiled === undefined) {
      return;
    }
    const folder = vscode.Uri.joinPath(document.uri, "..", compiled.crate);
    const write = (name: string, text: string) =>
      vscode.workspace.fs.writeFile(vscode.Uri.joinPath(folder, name), Buffer.from(text, "utf8"));
    await write("Cargo.toml", compiled.cargo);
    await write("src/lib.rs", compiled.library);
    await write("module.toml", compiled.module);
    await webview.postMessage({ type: "compiled", folder: compiled.crate });
  }

  private request<T>(webview: vscode.Webview, method: string, params: object) {
    return asked<T>(this.client(), webview, method, params);
  }
}
