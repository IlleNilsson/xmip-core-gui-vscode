// The designer's webview (ADR-0064, amendments 2026-10-03 and 2026-10-22): the
// cluster's one xmip.toml, a view per artifact kind, navigated by kind, then
// entry. Each entry's values are shown as the file writes them and edited in
// place, and what may be set on it is offered; a Route entry is an Xmip
// Application, drawn by routes.ts. It holds no rule: which kinds exist, where
// an entry is, which keys an entry may take and the words or names each may
// take, and what an edit does to the text are all xmip-lsp's, and the TOML is
// only ever written there.

import { button, byId, edit, host, html, tell } from "./dom.js";
import { type Part, type Routes, filterStructureAnswered, filterTextAnswered, showRoutes } from "./routes.js";

interface Field {
  key: string[];
  value: string;
  kind: string;
}
/** A key the server offers to set on an entry, and what it may take. */
interface Offered {
  key: string;
  kind: string;
  required: boolean;
  choices: string[];
  meaning?: string;
  /** The keys one element of an array of unnamed tables holds. */
  elements?: Offered[];
}
interface Entry {
  section: string[];
  name: string;
  scope: string;
  fields: Field[];
  offered?: Offered[];
  routes?: Routes;
  problems?: string[];
}
interface Place {
  section: string[];
  scope: string;
}
interface View {
  kind: string;
  title: string;
  note?: string;
  entries: Entry[];
  places: Place[];
}
interface Views {
  cluster: string;
  views: View[];
}

type ToView =
  | { type: "views"; answer: Views }
  | { type: "filterText"; answer: { text: string } }
  | { type: "filterStructure"; answer: Part }
  | { type: "refused"; reason: string };

const title = byId("title");
const kinds = byId("kinds");
const shown = byId("view");

let views: Views | undefined;
/** The kind chosen, by its name. */
let kind = "cluster";
/** The entry chosen, by its section's path, or none for the kind itself. */
let entry: string | undefined;

function path(section: string[]): string {
  return section.join("\u0000");
}

function chosenView(): View | undefined {
  return views?.views.find((view) => view.kind === kind);
}

function chosenEntry(): Entry | undefined {
  return chosenView()?.entries.find((candidate) => path(candidate.section) === entry);
}

function choose(nextKind: string, nextEntry: string | undefined): void {
  kind = nextKind;
  entry = nextEntry;
  draw();
}

// -- The navigation: every kind, and its entries ----------------------------

function drawKinds(): void {
  kinds.replaceChildren();
  for (const view of views?.views ?? []) {
    const item = html("li", undefined, view.kind === kind ? "chosen" : undefined);
    const label = `${view.title} (${view.entries.length})`;
    const open = button(label, () => choose(view.kind, undefined));
    open.className = "kind";
    item.append(open);
    if (view.kind === kind && view.entries.length > 0) {
      const list = html("ul");
      for (const listed of view.entries) {
        const here = path(listed.section) === entry;
        const line = html("li", undefined, here ? "chosen" : undefined);
        const name = listed.scope === "" ? listed.name : `${listed.name} · ${listed.scope}`;
        line.append(button(name, () => choose(view.kind, path(listed.section))));
        list.append(line);
      }
      item.append(list);
    }
    kinds.append(item);
  }
}

// -- A kind: what it is, and where a new entry goes -------------------------

function drawKind(view: View): void {
  shown.append(html("h2", view.title));
  if (view.note !== undefined) {
    shown.append(html("p", view.note, "hint"));
  }
  if (view.entries.length === 0) {
    shown.append(html("p", "The file holds none.", "hint"));
  }
  if (view.kind === "node") {
    const name = html("input");
    name.placeholder = "node name";
    shown.append(
      html("h3", "Declare a node"),
      name,
      button("Add", () => edit({ "add-node": { name: name.value } })),
    );
  }
  if (view.places.length > 0) {
    const where = html("select");
    for (const place of view.places) {
      where.append(new Option(`${place.section.join(".")} · ${place.scope}`, path(place.section)));
    }
    const name = html("input");
    name.placeholder = "name";
    const add = button("Add", () => {
      const place = view.places.find((candidate) => path(candidate.section) === where.value);
      if (place !== undefined) {
        edit({ "add-entry": { section: place.section, name: name.value, values: [] } });
      }
    });
    shown.append(html("h3", `Add a ${view.title}`), where, name, add);
  }
}

// -- An entry: its values as the file writes them ---------------------------

function drawEntry(view: View, chosen: Entry): void {
  shown.append(html("h2", `${view.title} ${chosen.name}`));
  shown.append(html("p", `[${chosen.section.join(".")}] · ${chosen.scope}`, "hint"));
  for (const problem of chosen.problems ?? []) {
    shown.append(html("p", problem, "undefined"));
  }

  const table = html("table", undefined, "fields");
  for (const field of chosen.fields) {
    const value = html("input");
    value.value = field.value;
    value.spellcheck = false;
    const set = (): void =>
      edit({ set: { section: chosen.section, key: field.key, value: value.value } });
    value.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        set();
      }
    });
    const line = html("tr");
    const cells = [
      html("td", field.key.join(".")),
      html("td"),
      html("td", field.kind, "hint"),
      html("td"),
    ];
    cells[1]?.append(value);
    cells[3]?.append(
      button("Set", set),
      button("Remove", () => edit({ remove: { section: chosen.section, key: field.key } })),
    );
    line.append(...cells);
    table.append(line);
  }
  shown.append(table);

  // What may be set, as the server offers it: a key's kind, whether it is
  // required, what it means, and the words or names its value may take.
  const offered = chosen.offered ?? [];
  const keys = html("datalist");
  keys.id = "offered-keys";
  for (const one of offered) {
    keys.append(new Option(`${one.kind}${one.required ? " · required" : ""}`, one.key));
  }
  const meaning = html("p", undefined, "hint");
  const values = html("datalist");
  values.id = "offered-values";
  const key = html("input");
  key.placeholder = "key, a.b for a sub-table";
  key.setAttribute("list", keys.id);
  const value = html("input");
  value.placeholder = "value in TOML: \"text\", 4, true";
  value.setAttribute("list", values.id);
  key.addEventListener("input", () => {
    values.replaceChildren();
    const one = offered.find((candidate) => candidate.key === key.value);
    for (const choice of one?.choices ?? []) {
      values.append(new Option(choice, `"${choice}"`));
    }
    meaning.textContent = one?.meaning ?? "";
  });
  shown.append(keys, values);
  const add = button("Set", () =>
    edit({ set: { section: chosen.section, key: key.value.split("."), value: value.value } }),
  );
  const remove = button(`Remove ${chosen.name}`, () => {
    edit({ "remove-entry": { section: chosen.section } });
  });
  const actions = html("div", undefined, "actions");
  actions.append(key, value, add, remove);
  shown.append(actions, meaning);

  for (const list of offered.filter((one) => (one.elements ?? []).length > 0)) {
    drawElement(chosen, list);
  }

  if (chosen.routes !== undefined) {
    const drawing = html("section", undefined, "routes");
    shown.append(html("h3", "Routes"), drawing);
    showRoutes(drawing, chosen.routes);
  }
}

/** A form adding one element to an array of unnamed tables, such as a
 * Party's identity: one value for each key the server offers, in TOML. */
function drawElement(chosen: Entry, list: Offered): void {
  const row = html("div", undefined, "actions");
  const inputs: [Offered, HTMLInputElement][] = [];
  for (const inner of list.elements ?? []) {
    const value = html("input");
    value.placeholder = `${inner.key}${inner.required ? " (required)" : ""}, in TOML`;
    if (inner.choices.length > 0) {
      const words = html("datalist");
      words.id = `offered-${list.key}-${inner.key}`;
      for (const choice of inner.choices) {
        words.append(new Option(choice, `"${choice}"`));
      }
      value.setAttribute("list", words.id);
      row.append(words);
    }
    inputs.push([inner, value]);
    row.append(value);
  }
  const add = button(`Add to ${list.key}`, () => {
    const values = inputs
      .filter(([, value]) => value.value !== "")
      .map(([inner, value]) => ({ key: [inner.key], value: value.value }));
    edit({ "add-element": { section: chosen.section, key: [list.key], values } });
  });
  row.append(add);
  shown.append(html("h3", `Add to ${list.key}`), row);
}

function draw(): void {
  drawKinds();
  shown.replaceChildren();
  const view = chosenView();
  if (view === undefined) {
    return;
  }
  const chosen = chosenEntry();
  if (chosen === undefined) {
    drawKind(view);
  } else {
    drawEntry(view, chosen);
  }
}

window.addEventListener("message", (event: MessageEvent<ToView>) => {
  const message = event.data;
  switch (message.type) {
    case "views":
      views = message.answer;
      title.textContent = `Xmip cluster ${views.cluster}`;
      if (chosenEntry() === undefined) {
        entry = undefined;
      }
      tell("");
      draw();
      break;
    case "filterText":
      filterTextAnswered(message.answer.text);
      break;
    case "filterStructure":
      filterStructureAnswered(message.answer);
      break;
    case "refused":
      tell(message.reason, true);
      break;
  }
});

host.postMessage({ type: "ready" });
