// The routes designer's carrier, tested the way the shell is: in plain node,
// offline, with the `vscode` API replaced by recording stubs and the language
// client by one that answers from a script. What is tested is only what the
// carrier does — pass the webview's messages to xmip-lsp, post the answers
// back, apply the text edit the server returns — and nothing the server or
// the runtime decides.

import assert from "node:assert/strict";
import * as Module from "node:module";
import { test } from "node:test";
import type * as vscode from "vscode";

interface Recorded {
  uri: string;
  range: number[];
  text: string;
}

class Range {
  readonly numbers: number[];
  constructor(...numbers: number[]) {
    this.numbers = numbers;
  }
}

class WorkspaceEdit {
  readonly replaced: Recorded[] = [];
  replace(uri: { toString(): string }, range: Range, text: string): void {
    this.replaced.push({ uri: uri.toString(), range: range.numbers, text });
  }
}

const applied: Recorded[] = [];
let changeListener: ((change: { document: { uri: { toString(): string } } }) => void) | undefined;
const disposable = { dispose(): void {} };

const stubVscode = {
  Range,
  WorkspaceEdit,
  Uri: {
    joinPath: (base: { path: string }, ...parts: string[]) => {
      const path = [base.path, ...parts].join("/");
      return { path, toString: () => path };
    },
  },
  workspace: {
    onDidChangeTextDocument: (listener: typeof changeListener) => {
      changeListener = listener;
      return disposable;
    },
    applyEdit: (edit: WorkspaceEdit) => {
      applied.push(...edit.replaced);
      return Promise.resolve(true);
    },
  },
};

interface Loader {
  _load: (request: string, ...rest: unknown[]) => unknown;
}
const loader = (Module as unknown as { Module: Loader }).Module;
const load = loader._load;
loader._load = function (request: string, ...rest: unknown[]): unknown {
  if (request === "vscode") {
    return stubVscode;
  }
  return load.call(this, request, ...rest);
};

/** A language client that answers each method from `answers`, or throws. */
class ScriptedClient {
  readonly asked: { method: string; params: Record<string, unknown> }[] = [];
  constructor(private readonly answers: Record<string, unknown>) {}

  sendRequest<T>(method: string, params: Record<string, unknown>): Promise<T> {
    this.asked.push({ method, params });
    const answer = this.answers[method];
    if (answer instanceof Error) {
      return Promise.reject(answer);
    }
    return Promise.resolve(answer as T);
  }
}

const posted: unknown[] = [];
let received: ((message: unknown) => void) | undefined;

const webview = {
  options: {},
  html: "",
  cspSource: "vscode-resource:",
  asWebviewUri: (uri: { path: string }) => ({ toString: () => `webview:${uri.path}` }),
  onDidReceiveMessage: (listener: (message: unknown) => void) => {
    received = listener;
    return disposable;
  },
  postMessage: (message: unknown) => {
    posted.push(message);
    return Promise.resolve(true);
  },
};
const panel = { webview, onDidDispose: () => disposable };

const document = {
  uri: { toString: () => "file:///orders.application.toml" },
  version: 1,
  getText: () => "[application]\nname = \"Orders\"\n",
};

async function designer(
  client: ScriptedClient | undefined,
): Promise<import("../designer.js").ApplicationDesigner> {
  const { ApplicationDesigner } = await import("../designer.js");
  const made = new ApplicationDesigner(
    { path: "/extension" } as unknown as vscode.Uri,
    () => client as unknown as import("vscode-languageclient/node").LanguageClient,
  );
  made.resolveCustomTextEditor(
    document as unknown as vscode.TextDocument,
    panel as unknown as vscode.WebviewPanel,
  );
  return made;
}

/** Let the carrier's promises settle. */
async function settled(): Promise<void> {
  for (let turn = 0; turn < 5; turn += 1) {
    await Promise.resolve();
  }
}

void test("the page loads one script by nonce and one stylesheet, nothing inline", async () => {
  await designer(new ScriptedClient({}));

  assert.match(webview.html, /script-src 'nonce-[^']+'/);
  assert.match(webview.html, /<script type="module" nonce="[^"]+" src="webview:\/extension\/out\/webview\/designer\.js">/);
  assert.match(webview.html, /href="webview:\/extension\/media\/designer\.css"/);
  assert.doesNotMatch(webview.html, /<script(?![^>]*src=)/);
});

void test("ready asks for the routes of the text as it is and posts them", async () => {
  posted.length = 0;
  const routes = { application: "Orders", nodes: [], edges: [] };
  const client = new ScriptedClient({ "xmip/routes": routes });
  await designer(client);

  received?.({ type: "ready" });
  await settled();

  assert.equal(client.asked[0]?.method, "xmip/routes");
  assert.equal(client.asked[0]?.params.text, document.getText());
  assert.deepEqual(posted, [{ type: "routes", answer: routes }]);
});

void test("an edit is the server's text edit, applied to the document", async () => {
  applied.length = 0;
  const edit = { "add-send-port": { name: "Billing" } };
  const answer = {
    range: { start: { line: 2, character: 0 }, end: { line: 2, character: 0 } },
    newText: "\n[[send_ports]]\nname = \"Billing\"\n",
  };
  const client = new ScriptedClient({ "xmip/edit": answer });
  await designer(client);

  received?.({ type: "edit", edit });
  await settled();

  assert.deepEqual(client.asked[0]?.params.edit, edit);
  assert.deepEqual(applied, [
    { uri: "file:///orders.application.toml", range: [2, 0, 2, 0], text: answer.newText },
  ]);
});

void test("a refusal is posted to the webview and nothing is applied", async () => {
  posted.length = 0;
  applied.length = 0;
  const client = new ScriptedClient({
    "xmip/edit": new Error("the Application has no Subscription 'invoices'"),
  });
  await designer(client);

  received?.({ type: "edit", edit: { connect: { subscription: "invoices", target: "x" } } });
  await settled();

  assert.deepEqual(posted, [
    { type: "refused", reason: "the Application has no Subscription 'invoices'" },
  ]);
  assert.deepEqual(applied, []);
});

void test("a filter's rows go to the server for their text, and text for its rows", async () => {
  posted.length = 0;
  const client = new ScriptedClient({
    "xmip/filterText": { text: "true" },
    "xmip/filterStructure": { shape: "group", join: "and", parts: [] },
  });
  await designer(client);

  received?.({ type: "filterText", structure: { shape: "group", join: "and", parts: [] } });
  received?.({ type: "filterStructure", filter: "true" });
  await settled();

  assert.deepEqual(
    client.asked.map((asked) => asked.method),
    ["xmip/filterText", "xmip/filterStructure"],
  );
  assert.deepEqual(posted, [
    { type: "filterText", answer: { text: "true" } },
    { type: "filterStructure", answer: { shape: "group", join: "and", parts: [] } },
  ]);
});

void test("a change made in the text editor draws the routes again", async () => {
  posted.length = 0;
  const client = new ScriptedClient({ "xmip/routes": { application: "Orders" } });
  await designer(client);

  changeListener?.({ document: { uri: { toString: () => "file:///elsewhere.toml" } } });
  changeListener?.({ document });
  await settled();

  assert.equal(client.asked.length, 1, "only its own document");
  assert.deepEqual(posted, [{ type: "routes", answer: { application: "Orders" } }]);
});

void test("without a running server the webview is told so", async () => {
  posted.length = 0;
  await designer(undefined);

  received?.({ type: "ready" });
  await settled();

  assert.deepEqual(posted, [{ type: "refused", reason: "xmip-lsp is not running." }]);
});
