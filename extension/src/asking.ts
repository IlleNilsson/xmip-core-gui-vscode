// What both designers do with xmip-lsp, placed once: ask it, posting a
// refusal to the webview where it does not answer; and apply the one text
// edit it answers, unless the document changed meanwhile.

import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

interface Position {
  line: number;
  character: number;
}

/** What an edit request answers: the one text edit that makes the change. */
export interface TextEdit {
  range: { start: Position; end: Position };
  newText: string;
}

/** The server's answer, or undefined after the refusal was posted. */
export async function asked<T>(
  client: LanguageClient | undefined,
  webview: vscode.Webview,
  method: string,
  params: object,
): Promise<T | undefined> {
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

/** `answer` made to `document`, where it is still at `version`. */
export async function applied(
  document: vscode.TextDocument,
  webview: vscode.Webview,
  version: number,
  answer: TextEdit,
): Promise<void> {
  if (document.version !== version) {
    const reason = "The text changed meanwhile; do it again.";
    await webview.postMessage({ type: "refused", reason });
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

/** The document as a request names it: its uri, and its text as it is now. */
export function request(document: vscode.TextDocument): object {
  return { textDocument: { uri: document.uri.toString() }, text: document.getText() };
}
