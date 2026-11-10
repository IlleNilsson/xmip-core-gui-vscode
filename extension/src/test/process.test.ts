// The Work Process designer's carrier, tested as the cluster designer's is:
// in plain node, offline, with the `vscode` API replaced by recording stubs
// and the language client by one that answers from a script. Only what the
// carrier does is tested — pass the webview's messages to xmip-lsp, post the
// answers back, apply the text edit the server returns, write the unit a
// compile answers — never what the server or the runtime decides.

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
const written: { path: string; text: string }[] = [];
let answeredName: string | undefined = "orders-flow";
const disposable = { dispose(): void {} };

const uri = (path: string) => ({ path, toString: () => path });
const stubVscode = {
  Range,
  WorkspaceEdit,
  Uri: {
    joinPath: (base: { path: string }, ...parts: string[]) => {
      const joined = [base.path, ...parts].join("/").replace(/\/[^/]+\/\.\.\//, "/");
      return uri(joined);
    },
  },
  window: {
    showInputBox: () => Promise.resolve(answeredName),
  },
  workspace: {
    onDidChangeTextDocument: () => disposable,
    applyEdit: (edit: WorkspaceEdit) => {
      applied.push(...edit.replaced);
      return Promise.resolve(true);
    },
    fs: {
      writeFile: (at: { path: string }, bytes: Uint8Array) => {
        written.push({ path: at.path, text: Buffer.from(bytes).toString("utf8") });
        return Promise.resolve();
      },
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
  asWebviewUri: (at: { path: string }) => ({ toString: () => `webview:${at.path}` }),
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
  uri: uri("/designs/orders.bpmn"),
  version: 1,
  getText: () => "<bpmn:definitions/>",
};

async function designer(client: ScriptedClient | undefined): Promise<void> {
  const { ProcessDesigner } = await import("../process.js");
  const made = new ProcessDesigner(
    uri("/extension") as unknown as vscode.Uri,
    () => client as unknown as import("vscode-languageclient/node").LanguageClient,
  );
  made.resolveCustomTextEditor(
    document as unknown as vscode.TextDocument,
    panel as unknown as vscode.WebviewPanel,
  );
}

async function settled(): Promise<void> {
  for (let turn = 0; turn < 8; turn += 1) {
    await Promise.resolve();
  }
}

void test("the page loads the process script and stylesheet by nonce, nothing inline", async () => {
  await designer(new ScriptedClient({}));

  assert.match(webview.html, /src="webview:\/extension\/out\/webview\/process\.js"/);
  assert.match(webview.html, /href="webview:\/extension\/media\/process\.css"/);
  assert.match(webview.html, /<title>Xmip Work Process designer<\/title>/);
  assert.doesNotMatch(webview.html, /<script(?![^>]*src=)/);
});

void test("ready asks for the design as drawn and posts it", async () => {
  posted.length = 0;
  const drawn = { process: "OrderApproval", nodes: [], flows: [], problems: [], kinds: [] };
  const client = new ScriptedClient({ "xmip/process": drawn });
  await designer(client);

  received?.({ type: "ready" });
  await settled();

  assert.equal(client.asked[0]?.params.text, document.getText());
  assert.deepEqual(posted, [{ type: "diagram", answer: drawn }]);
});

void test("an edit is the server's text edit, applied to the design", async () => {
  applied.length = 0;
  const edit = { edit: "rename", id: "Check", name: "Vet" };
  const answer = {
    range: { start: { line: 4, character: 10 }, end: { line: 4, character: 15 } },
    newText: "Vet",
  };
  const client = new ScriptedClient({ "xmip/processEdit": answer });
  await designer(client);

  received?.({ type: "edit", edit });
  await settled();

  assert.deepEqual(client.asked[0]?.params.edit, edit);
  assert.deepEqual(applied, [{ uri: "/designs/orders.bpmn", range: [4, 10, 4, 15], text: "Vet" }]);
});

void test("a compile writes the unit's crate and module entry beside the design", async () => {
  posted.length = 0;
  written.length = 0;
  answeredName = "orders-flow";
  const compiled = {
    cargo: "[package]",
    library: "process::export_work_process!",
    module: "[[modules]]",
    crate: "orders-flow",
  };
  const client = new ScriptedClient({ "xmip/processCompile": compiled });
  await designer(client);

  received?.({ type: "compile" });
  await settled();

  assert.equal(client.asked[0]?.params.module, "orders-flow");
  assert.equal(client.asked[0]?.params.design, "orders.bpmn");
  assert.deepEqual(
    written.map((file) => [file.path, file.text]),
    [
      ["/designs/orders-flow/Cargo.toml", "[package]"],
      ["/designs/orders-flow/src/lib.rs", "process::export_work_process!"],
      ["/designs/orders-flow/module.toml", "[[modules]]"],
    ],
  );
  assert.deepEqual(posted, [{ type: "compiled", folder: "orders-flow" }]);
});

void test("a compile given no module name asks nothing of the server", async () => {
  answeredName = undefined;
  const client = new ScriptedClient({});
  await designer(client);

  received?.({ type: "compile" });
  await settled();

  assert.equal(client.asked.length, 0);
});

void test("a refused compile is posted and nothing is written", async () => {
  posted.length = 0;
  written.length = 0;
  answeredName = "orders-flow";
  const client = new ScriptedClient({ "xmip/processCompile": new Error("S: leads nowhere") });
  await designer(client);

  received?.({ type: "compile" });
  await settled();

  assert.deepEqual(posted, [{ type: "refused", reason: "S: leads nowhere" }]);
  assert.deepEqual(written, []);
});
