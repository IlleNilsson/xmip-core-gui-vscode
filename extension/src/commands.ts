// The commands that create, rename and delete in the cluster's xmip.toml
// (ADR-0064, amendment 2026-10-13). Each asks the developer what the
// server says to ask — which place, which entry, a name, each answer — and
// runs the server's command with the answers; the server makes the edit
// through the runtime's xmip_cluster_edit_v1 and the editor applies it.
// Nothing here knows a section, a key or an edit: the server sends every
// list and every question.

import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

/** One answer a question offers: what the developer reads, what is sent. */
interface Option {
  label: string;
  value: string;
}

/** What a new entry cannot be without, beyond its name. */
interface Question {
  key: string;
  kind: string;
  options: Option[];
}

/** What the server needs to add an entry, as it sent it. */
interface AddArguments {
  uri: string;
  title: string;
  questions: Question[];
  [more: string]: unknown;
}

/** A place a new entry may be added, as `xmip/places` answers it. */
interface Place {
  label: string;
  arguments: AddArguments;
}

/** One way to delete an entry: alone, or with what names it. */
interface Choice {
  title: string;
  command: string;
  arguments: unknown;
}

/** An entry that may be renamed or deleted, as `xmip/entries` answers it. */
interface Listed {
  label: string;
  description: string;
  choices: Choice[];
  rename: RenameArguments;
}

interface RenameArguments {
  uri: string;
  name: string;
  title: string;
  [more: string]: unknown;
}

/** Register the three commands; `client` is the running language client. */
export function registerCommands(client: () => LanguageClient | undefined): vscode.Disposable[] {
  return [
    vscode.commands.registerCommand("xmip.add", (given?: AddArguments) => add(client, given)),
    vscode.commands.registerCommand("xmip.delete", () => remove(client)),
    vscode.commands.registerCommand("xmip.rename", (given?: RenameArguments) =>
      rename(client, given),
    ),
  ];
}

/** The active document's uri, where one is open. */
function active(): string | undefined {
  return vscode.window.activeTextEditor?.document.uri.toString();
}

/** Ask the server for a list over the active document. */
async function listed<T>(client: () => LanguageClient | undefined, method: string): Promise<T[]> {
  const uri = active();
  const running = client();
  if (uri === undefined || running === undefined) {
    void vscode.window.showInformationMessage("Xmip: open the cluster's xmip.toml first.");
    return [];
  }
  return running.sendRequest<T[]>(method, { textDocument: { uri } });
}

async function add(client: () => LanguageClient | undefined, given?: AddArguments): Promise<void> {
  let place = given;
  if (place === undefined) {
    const places = await listed<Place>(client, "xmip/places");
    const picked = await vscode.window.showQuickPick(
      places.map((one) => ({ label: one.label, place: one })),
      { placeHolder: "Add where" },
    );
    place = picked?.place.arguments;
  }
  if (place === undefined) {
    return;
  }
  const name = await vscode.window.showInputBox({ prompt: `Name of the new ${place.title}` });
  if (name === undefined) {
    return;
  }
  const answers: { key: string; kind: string; value: string }[] = [];
  for (const question of place.questions) {
    const value =
      question.options.length > 0
        ? (
            await vscode.window.showQuickPick(
              question.options.map((option) => ({ label: option.label, value: option.value })),
              { placeHolder: question.key },
            )
          )?.value
        : await vscode.window.showInputBox({ prompt: `${question.key} (${question.kind})` });
    if (value === undefined) {
      return;
    }
    answers.push({ key: question.key, kind: question.kind, value });
  }
  await vscode.commands.executeCommand("xmip.server.add", { ...place, name, answers });
}

async function remove(client: () => LanguageClient | undefined): Promise<void> {
  const entries = await listed<Listed>(client, "xmip/entries");
  const picked = await vscode.window.showQuickPick(
    entries.map((entry) => ({ label: entry.label, description: entry.description, entry })),
    { placeHolder: "Delete which" },
  );
  if (picked === undefined) {
    return;
  }
  const choices = picked.entry.choices;
  const choice =
    choices.length > 1
      ? (
          await vscode.window.showQuickPick(
            choices.map((one) => ({ label: one.title, one })),
            { placeHolder: "It is named elsewhere in the file" },
          )
        )?.one
      : choices[0];
  if (choice !== undefined) {
    await vscode.commands.executeCommand(choice.command, choice.arguments);
  }
}

async function rename(
  client: () => LanguageClient | undefined,
  given?: RenameArguments,
): Promise<void> {
  let entry = given;
  if (entry === undefined) {
    const entries = await listed<Listed>(client, "xmip/entries");
    const picked = await vscode.window.showQuickPick(
      entries.map((one) => ({ label: one.label, description: one.description, one })),
      { placeHolder: "Rename which" },
    );
    entry = picked?.one.rename;
  }
  if (entry === undefined) {
    return;
  }
  const name = await vscode.window.showInputBox({
    prompt: `New name for ${entry.title} '${entry.name}'`,
    value: entry.name,
  });
  if (name !== undefined && name !== entry.name) {
    await vscode.commands.executeCommand("xmip.server.rename", { ...entry, name });
  }
}
