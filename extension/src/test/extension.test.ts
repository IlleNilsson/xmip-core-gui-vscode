// The shell's test, in plain node and offline: no extension host, no VS Code
// download, no server process. The `vscode` API and the language client are
// replaced with recording stubs before the extension is loaded, so what is
// tested is exactly what the shell does — register its command and start the
// server from the settings — and nothing the server does.

import assert from "node:assert/strict";
import * as Module from "node:module";
import { test } from "node:test";
import type * as vscode from "vscode";

interface ServerOptions {
  command: string;
  args: string[];
}

/** What the extension asked of the client: constructed, started, stopped. */
class FakeClient {
  started = 0;
  stopped = 0;

  constructor(
    readonly id: string,
    readonly name: string,
    readonly server: ServerOptions,
  ) {
    clients.push(this);
  }

  start(): Promise<void> {
    this.started += 1;
    return Promise.resolve();
  }

  stop(): Promise<void> {
    this.stopped += 1;
    return Promise.resolve();
  }
}

const clients: FakeClient[] = [];
const commands = new Map<string, () => unknown>();
const settings = new Map<string, string>();
const messages: string[] = [];
const disposable = { dispose(): void {} };

const stubVscode = {
  window: {
    activeTextEditor: undefined,
    createOutputChannel: () => ({ ...disposable, appendLine(): void {}, show(): void {} }),
    showInformationMessage: (message: string) => {
      messages.push(message);
      return Promise.resolve(undefined);
    },
  },
  commands: {
    registerCommand: (command: string, callback: () => unknown) => {
      commands.set(command, callback);
      return disposable;
    },
  },
  workspace: {
    getConfiguration: () => ({
      get: (key: string, fallback: string) => settings.get(key) ?? fallback,
    }),
    onDidChangeConfiguration: () => disposable,
  },
};

const stubClient = { LanguageClient: FakeClient };

// `Module._load` is what every `require` reaches; answering two names from
// here is the whole extension host this test needs. The namespace import is
// a read-only view; `Module.Module` is the loader itself.
interface Loader {
  _load: (request: string, ...rest: unknown[]) => unknown;
}
const loader = (Module as unknown as { Module: Loader }).Module;
const load = loader._load;
loader._load = function (request: string, ...rest: unknown[]): unknown {
  if (request === "vscode") {
    return stubVscode;
  }
  if (request === "vscode-languageclient/node") {
    return stubClient;
  }
  return load.call(this, request, ...rest);
};

const context = { subscriptions: [] as { dispose(): void }[] };

async function extension(): Promise<typeof import("../extension.js")> {
  return import("../extension.js");
}

void test("activation registers the command and starts the server from the settings", async () => {
  settings.set("server.path", "C:/xmip/target/debug/xmip-lsp.exe");
  settings.set("runtime.library", "C:/xmip/target/debug/xmip_core_runtime.dll");

  const { activate } = await extension();
  await activate(context as unknown as vscode.ExtensionContext);

  assert.ok(commands.has("xmip.validate"), "xmip.validate is registered");
  assert.equal(clients.length, 1);
  assert.equal(clients[0]?.id, "xmip");
  assert.equal(clients[0]?.server.command, "C:/xmip/target/debug/xmip-lsp.exe");
  assert.deepEqual(clients[0]?.server.args, [
    "--runtime",
    "C:/xmip/target/debug/xmip_core_runtime.dll",
  ]);
  assert.equal(clients[0]?.started, 1);
  assert.ok(
    context.subscriptions.length >= 2,
    "the channel and the command are disposed with the extension",
  );
});

void test("deactivate stops the client; an empty runtime setting passes no flag", async () => {
  const { activate, deactivate } = await extension();
  await deactivate();
  assert.equal(clients[0]?.stopped, 1);

  settings.set("runtime.library", "");
  await activate(context as unknown as vscode.ExtensionContext);

  assert.equal(clients.length, 2);
  assert.deepEqual(clients[1]?.server.args, []);
  await deactivate();
});

void test("the command with no editor open says so instead of validating", async () => {
  const validate = commands.get("xmip.validate");
  assert.ok(validate, "the command is registered");

  await validate();

  assert.deepEqual(messages, ["Xmip: open a node configuration first."]);
});
