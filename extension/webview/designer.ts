// The designer's webview (ADR-0064, amendment 2026-10-03): the cluster's one
// xmip.toml, a view per artifact kind — Cluster, Node, Receive Port, Receive
// Location, Send Port, Send Location, Send Port Group, Prepare, Promote,
// Demote, Route, Transformation, Work Process — navigated by kind, then entry.
// Each entry's values are shown as the file writes them and edited in
// place; a Route entry is an Xmip Application, drawn by routes.ts. It holds
// no rule: which kinds exist, where an entry is, whether the configuration
// defines a kind and what an edit does to the text are all xmip-lsp's, and
// the TOML is only ever written there.

import { button, byId, edit, host, html, tell } from "./dom.js";
import { type Part, type Routes, filterStructureAnswered, filterTextAnswered, showRoutes } from "./routes.js";

interface Field {
  key: string[];
  value: string;
  kind: string;
}
interface Entry {
  section: string[];
  name: string;
  scope: string;
  fields: Field[];
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
  defined: boolean;
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
    const label = view.defined ? `${view.title} (${view.entries.length})` : view.title;
    const open = button(label, () => choose(view.kind, undefined));
    open.className = view.defined ? "kind" : "kind undefined";
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
    shown.append(html("p", view.note, view.defined ? "hint" : "undefined"));
  }
  if (!view.defined) {
    return;
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

  const key = html("input");
  key.placeholder = "key, a.b for a sub-table";
  const value = html("input");
  value.placeholder = "value in TOML: \"text\", 4, true";
  const add = button("Set", () =>
    edit({ set: { section: chosen.section, key: key.value.split("."), value: value.value } }),
  );
  const remove = button(`Remove ${chosen.name}`, () => {
    edit({ "remove-entry": { section: chosen.section } });
  });
  const actions = html("div", undefined, "actions");
  actions.append(key, value, add, remove);
  shown.append(actions);

  if (chosen.routes !== undefined) {
    const drawing = html("section", undefined, "routes");
    shown.append(html("h3", "Routes"), drawing);
    showRoutes(drawing, chosen.routes);
  }
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
