// The Route view (ADR-0064): an Xmip Application held as a section of the
// cluster's xmip.toml, its routes drawn as the graph xmip-lsp sends, and a
// filter editor in the shape of the classic filter dialog — rows of
// property, operator and value, gathered in And and Or groups. It holds no
// rule: what may route where, which operators and kinds exist, where a node
// stands, what a filter's text is and whether a row reads all come from the
// server, and the TOML is only ever written there. A filter's text is one
// line of Xmip's expression language (ADR-0066); the rows are a view of it,
// and "As text" shows that line.

import { button, choice, edit as editFile, host, html, svg, tell, words } from "./dom.js";

interface Condition {
  shape: "condition";
  property: string;
  operator: string;
  value: string;
  kind: string;
}
interface Group {
  shape: "group";
  join: "and" | "or";
  parts: Part[];
}
interface Not {
  shape: "not";
  part: Part;
}
export type Part = Condition | Group | Not;

interface RouteNode {
  id: string;
  kind: string;
  name: string;
  target: boolean;
  filter?: Part;
  summary?: string;
  x: number;
  y: number;
  width: number;
  height: number;
}
interface RouteEdge {
  from: string;
  to: string;
  kind: string;
}
export interface Routes {
  application: string;
  nodes: RouteNode[];
  edges: RouteEdge[];
  problems: string[];
  operators: string[];
  kinds: string[];
  width: number;
  height: number;
}

let routes: Routes | undefined;
/** The Subscription being edited, by its node's id. */
let selected: string | undefined;
/** Its filter as the developer is changing it, not yet applied. */
let working: Part | undefined;
/** Whether `working` differs from what the document says. */
let dirty = false;
let text: HTMLTextAreaElement | undefined;
let targets: HTMLSelectElement | undefined;
let canvas: SVGSVGElement | undefined;
let side: HTMLElement | undefined;

/** One of the Application's own edits, made to its section. */
function edit(change: object): void {
  editFile({ application: { application: routes?.application ?? "", edit: change } });
}

function node(id: string | undefined): RouteNode | undefined {
  return routes?.nodes.find((candidate) => candidate.id === id);
}

function copy(part: Part | undefined): Part | undefined {
  return part === undefined ? undefined : (JSON.parse(JSON.stringify(part)) as Part);
}

/** Draw `shown` into `container`, keeping the chosen Subscription when it is still there. */
export function showRoutes(container: HTMLElement, shown: Routes): void {
  if (routes?.application !== shown.application) {
    selected = undefined;
    dirty = false;
  }
  routes = shown;
  if (node(selected) === undefined) {
    selected = undefined;
  }
  if (!dirty) {
    working = copy(node(selected)?.filter);
  }

  const bar = html("div", undefined, "bar");
  canvas = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  canvas.id = "canvas";
  canvas.setAttribute("role", "img");
  canvas.setAttribute("aria-label", "The Application's routes");
  side = html("aside");
  side.id = "side";
  const drawing = html("div", undefined, "drawing");
  drawing.append(canvas, side);
  container.replaceChildren(bar, drawing);

  drawBar(bar);
  tell(shown.problems.join(" · "), shown.problems.length > 0);
  draw();
  asText();
}

/** The server's text for the rows being edited. */
export function filterTextAnswered(answer: string): void {
  if (text !== undefined) {
    text.value = answer;
  }
  tell(routes?.problems.join(" · ") ?? "", (routes?.problems.length ?? 0) > 0);
}

/** The server's rows for the text the developer typed. */
export function filterStructureAnswered(answer: Part): void {
  working = answer;
  structural();
}

// -- The toolbar: add what the Application declares -------------------------

function drawBar(bar: HTMLElement): void {
  const kind = choice(["Receive Location", "Work Process", "Send Port"], "Receive Location", () => {});
  const name = html("input");
  name.placeholder = "name";
  const adds: Record<string, string> = {
    "Receive Location": "add-receive-location",
    "Work Process": "add-work-process",
    "Send Port": "add-send-port",
  };
  const add = button("Add", () => {
    edit({ [adds[kind.value] ?? ""]: { name: name.value } });
    name.value = "";
  });

  const id = html("input");
  id.placeholder = "Subscription id";
  targets = html("select");
  const subscribe = button("Add Subscription", () => {
    edit({ "add-subscription": { id: id.value, target: targets?.value ?? "" } });
    id.value = "";
  });

  bar.append(html("span", "Declare", "label"), kind, name, add);
  bar.append(html("span", "Route", "label"), id, html("span", "to"), targets, subscribe);
}

function drawTargets(): void {
  if (targets === undefined || routes === undefined) {
    return;
  }
  const keep = targets.value;
  const reachable = routes.nodes.filter((candidate) => candidate.target);
  targets.replaceChildren(
    ...reachable.map((target) => new Option(`${words(target.kind)} ${target.name}`, target.id)),
  );
  if (reachable.some((target) => target.id === keep)) {
    targets.value = keep;
  }
}

// -- The canvas: the routes as the server laid them out ---------------------

function drawCanvas(): void {
  if (canvas === undefined || routes === undefined) {
    return;
  }
  canvas.replaceChildren();
  canvas.setAttribute("width", String(routes.width));
  canvas.setAttribute("height", String(routes.height));

  const arrow = svg("marker", {
    id: "arrow",
    viewBox: "0 0 10 10",
    refX: 10,
    refY: 5,
    markerWidth: 7,
    markerHeight: 7,
    orient: "auto-start-reverse",
  });
  arrow.append(svg("path", { d: "M 0 0 L 10 5 L 0 10 z", class: "arrowhead" }));
  const definitions = svg("defs", {});
  definitions.append(arrow);
  canvas.append(definitions);

  for (const edge of routes.edges) {
    const from = node(edge.from);
    const to = node(edge.to);
    if (from === undefined || to === undefined) {
      continue;
    }
    const x1 = from.x + from.width;
    const y1 = from.y + from.height / 2;
    const x2 = to.x;
    const y2 = to.y + to.height / 2;
    const middle = (x1 + x2) / 2;
    canvas.append(
      svg("path", {
        d: `M ${x1} ${y1} C ${middle} ${y1}, ${middle} ${y2}, ${x2} ${y2}`,
        class: `edge ${edge.kind}`,
        "marker-end": "url(#arrow)",
      }),
    );
  }

  for (const drawn of routes.nodes) {
    canvas.append(box(drawn));
  }
}

function box(drawn: RouteNode): SVGElement {
  const chosen = drawn.id === selected;
  const reachable = selected !== undefined && drawn.target;
  const group = svg("g", {
    class: `node ${drawn.kind}${chosen ? " selected" : ""}${reachable ? " reachable" : ""}`,
    tabindex: 0,
  });
  group.append(
    svg("rect", { x: drawn.x, y: drawn.y, width: drawn.width, height: drawn.height, rx: 6 }),
  );
  const caption = svg("text", { x: drawn.x + 10, y: drawn.y + 17, class: "caption" });
  caption.textContent = words(drawn.kind);
  const name = svg("text", { x: drawn.x + 10, y: drawn.y + 36, class: "name" });
  name.textContent = drawn.name;
  group.append(caption, name);

  if (drawn.summary !== undefined) {
    const summary = svg("text", { x: drawn.x + 10, y: drawn.y + 54, class: "summary" });
    const short = drawn.summary.length > 32 ? `${drawn.summary.slice(0, 31)}…` : drawn.summary;
    summary.textContent = short;
    const full = svg("title", {});
    full.textContent = drawn.summary;
    group.append(summary, full);
  }

  const act = (): void => clicked(drawn);
  group.addEventListener("click", act);
  group.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      act();
    }
  });
  return group;
}

/** A Subscription is chosen; with one chosen, a target routes it there. */
function clicked(drawn: RouteNode): void {
  if (drawn.filter !== undefined) {
    select(drawn.id);
  } else if (drawn.target && selected !== undefined) {
    const subscription = node(selected);
    if (subscription !== undefined) {
      edit({ connect: { subscription: subscription.name, target: drawn.id } });
    }
  }
}

function select(id: string | undefined): void {
  selected = id;
  working = copy(node(id)?.filter);
  dirty = false;
  draw();
  asText();
}

// -- The side: the chosen Subscription and its filter -----------------------

function drawSide(): void {
  if (side === undefined) {
    return;
  }
  side.replaceChildren();
  const subscription = node(selected);
  if (subscription === undefined || routes === undefined) {
    side.append(
      html(
        "p",
        "Choose a Subscription to edit its filter. With one chosen, click a Work Process, " +
          "a Send Port Group or a Send Port to route it there.",
        "hint",
      ),
    );
    return;
  }

  side.append(html("h2", `Subscription ${subscription.name}`));
  const routed = routes.edges.find((e) => e.from === subscription.id && e.kind === "routes");
  const destinations = routes.nodes.filter((candidate) => candidate.target);
  const to = html("select");
  for (const target of destinations) {
    const label = `${words(target.kind)} ${target.name}`;
    to.append(new Option(label, target.id, false, target.id === routed?.to));
  }
  to.addEventListener("change", () =>
    edit({ connect: { subscription: subscription.name, target: to.value } }),
  );
  const routing = html("label", "Routes to ");
  routing.append(to);
  side.append(routing, html("h3", "Filter"));

  side.append(
    part(working ?? everything(), (replaced) => {
      working = replaced ?? everything();
      structural();
    }),
  );

  const apply = button("Apply filter", () => {
    dirty = false;
    edit({ "set-filter": { subscription: subscription.name, filter: working ?? everything() } });
  });
  apply.disabled = !dirty;
  const revert = button("Revert", () => select(selected));
  revert.disabled = !dirty;
  const actions = html("div", undefined, "actions");
  actions.append(apply, revert);
  side.append(actions);

  side.append(html("h3", "As text"));
  text = html("textarea");
  text.rows = 2;
  text.placeholder = "MessageType = 'Order' and not Amount > 1000";
  text.spellcheck = false;
  const read = button("Read text", () => {
    host.postMessage({ type: "filterStructure", filter: text?.value ?? "" });
  });
  side.append(text, read);
}

function everything(): Group {
  return { shape: "group", join: "and", parts: [] };
}

function condition(): Condition {
  return {
    shape: "condition",
    property: "",
    operator: routes?.operators[0] ?? "",
    value: "",
    kind: routes?.kinds[0] ?? "",
  };
}

/** A change that adds, removes or moves a part: draw the side again. */
function structural(): void {
  dirty = true;
  drawSide();
  asText();
}

/** A change inside a row: the text follows, the rows stay as they are. */
function typed(): void {
  dirty = true;
  side?.querySelectorAll<HTMLButtonElement>(".actions button").forEach((each) => {
    each.disabled = false;
  });
  asText();
}

function asText(): void {
  if (working !== undefined) {
    host.postMessage({ type: "filterText", structure: working });
  }
}

/** One part of the filter, and how to replace or remove it in its parent. */
function part(shown: Part, put: (replaced: Part | undefined) => void): HTMLElement {
  switch (shown.shape) {
    case "condition":
      return row(shown, put);
    case "not": {
      const wrapper = html("div", undefined, "not");
      wrapper.append(
        html("span", "Not", "label"),
        button("Unwrap", () => put(shown.part)),
        part(shown.part, (replaced) => {
          if (replaced === undefined) {
            put(undefined);
          } else {
            shown.part = replaced;
            structural();
          }
        }),
      );
      return wrapper;
    }
    case "group":
      return group(shown, put);
  }
}

function row(shown: Condition, put: (replaced: Part | undefined) => void): HTMLElement {
  const line = html("div", undefined, "row");
  const property = html("input");
  property.placeholder = "property";
  property.value = shown.property;
  property.addEventListener("input", () => {
    shown.property = property.value;
    typed();
  });
  const value = html("input");
  value.placeholder = "value";
  value.value = shown.value;
  value.addEventListener("input", () => {
    shown.value = value.value;
    typed();
  });
  line.append(
    property,
    choice(routes?.operators ?? [], shown.operator, (operator) => {
      shown.operator = operator;
      typed();
    }),
    value,
    choice(routes?.kinds ?? [], shown.kind, (kind) => {
      shown.kind = kind;
      typed();
    }),
    button("Not", () => put({ shape: "not", part: shown })),
    button("×", () => put(undefined)),
  );
  return line;
}

function group(shown: Group, put: (replaced: Part | undefined) => void): HTMLElement {
  const box = html("fieldset", undefined, "group");
  const legend = html("legend");
  legend.append(
    choice(["and", "or"], shown.join, (join) => {
      shown.join = join === "or" ? "or" : "and";
      typed();
    }),
    button("Not", () => put({ shape: "not", part: shown })),
    button("×", () => put(undefined)),
  );
  box.append(legend);

  shown.parts.forEach((child, index) => {
    box.append(
      part(child, (replaced) => {
        if (replaced === undefined) {
          shown.parts.splice(index, 1);
        } else {
          shown.parts[index] = replaced;
        }
        structural();
      }),
    );
  });

  box.append(
    button("+ Condition", () => {
      shown.parts.push(condition());
      structural();
    }),
    button("+ Group", () => {
      shown.parts.push(everything());
      structural();
    }),
  );
  return box;
}

function draw(): void {
  drawTargets();
  drawCanvas();
  drawSide();
}
