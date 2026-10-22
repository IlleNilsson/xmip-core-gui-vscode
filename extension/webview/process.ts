// The Work Process designer's webview (ADR-0066, amendment 2026-10-22): a
// BPMN 2.0 design drawn in SVG from what xmip-lsp answers — events as
// circles, tasks as rounded rectangles with a glyph, gateways as diamonds,
// a subprocess as a dashed frame, flows through their waypoints — with a
// palette, dragging, connecting and a properties panel. It holds no rule:
// which kinds exist, where a shape stands, what an edit does to the design
// and what keeps it from compiling are the server's, and the XML is only
// ever written there.

import { button, byId, edit, host, html, svg, tell } from "./dom.js";

interface Call {
  module: string;
  unit: string;
  settings: [string, string][];
}
interface Shape {
  id: string;
  kind: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  parent: string | null;
  attached: string | null;
  call: Call | null;
  debatch: Call | null;
  batch: { most: string; within: string | null } | null;
  set: string | null;
  timer: string | null;
  error: string | null;
}
interface Flow {
  id: string;
  source: string;
  target: string;
  condition: string | null;
  default: boolean;
  waypoints: [number, number][];
}
interface Diagram {
  process: string;
  nodes: Shape[];
  flows: Flow[];
  problems: { at: string; why: string }[];
  kinds: [string, string][];
  width: number;
  height: number;
}

type ToView =
  | { type: "diagram"; answer: Diagram }
  | { type: "compiled"; folder: string }
  | { type: "refused"; reason: string };

const palette = byId("palette");
const holder = byId("canvas-holder");
const properties = byId("properties");

let drawn: Diagram | undefined;
/** What is chosen: a step or a flow, by its identifier. */
let chosen: string | undefined;
/** The kind the palette armed, added where the canvas is clicked next. */
let adding: string | undefined;
/** Connecting: the step a flow starts from, once one is clicked. */
let connecting: { from?: string } | undefined;

const BOUNDARY = ["errorBoundary", "timerBoundary"];
const EVENTS = ["startEvent", "endEvent", "timerCatch", "messageCatch", "intermediateThrowEvent"];
const GLYPH: Record<string, string> = {
  serviceTask: "⚙",
  sendTask: "✉",
  receiveTask: "☐",
  task: "",
};

// -- The palette ---------------------------------------------------------------

function drawPalette(diagram: Diagram): void {
  palette.replaceChildren();
  for (const [kind, words] of diagram.kinds) {
    const armed = button(words, () => {
      adding = adding === kind ? undefined : kind;
      connecting = undefined;
      tell(adding === undefined ? "" : arming(kind));
      drawPalette(diagram);
    });
    armed.className = adding === kind ? "armed" : "";
    palette.append(armed);
  }
  const connect = button("Connect", () => {
    connecting = connecting === undefined ? {} : undefined;
    adding = undefined;
    tell(connecting === undefined ? "" : "Click the step a flow starts from.");
    drawPalette(diagram);
  });
  connect.className = connecting === undefined ? "" : "armed";
  palette.append(connect);
}

function arming(kind: string): string {
  return BOUNDARY.includes(kind)
    ? "Click the task or subprocess it is attached to."
    : "Click the canvas where it goes.";
}

// -- The canvas ----------------------------------------------------------------

function drawCanvas(diagram: Diagram): void {
  const canvas = svg("svg", {
    id: "canvas",
    width: Math.max(diagram.width, 600),
    height: Math.max(diagram.height, 400),
  });
  const defs = svg("defs", {});
  const marker = svg("marker", {
    id: "arrow",
    viewBox: "0 0 10 10",
    refX: 10,
    refY: 5,
    markerWidth: 8,
    markerHeight: 8,
    orient: "auto-start-reverse",
  });
  marker.append(svg("path", { d: "M 0 0 L 10 5 L 0 10 z" }));
  defs.append(marker);
  canvas.append(defs);
  canvas.addEventListener("click", (event) => {
    if (event.target !== canvas || adding === undefined || BOUNDARY.includes(adding)) {
      return;
    }
    const at = canvas.getBoundingClientRect();
    edit({ edit: "add", kind: adding, x: event.clientX - at.left, y: event.clientY - at.top });
    adding = undefined;
  });
  const ordered = [...diagram.nodes].sort((a, b) => depth(a) - depth(b));
  for (const shape of ordered) {
    canvas.append(drawShape(shape, diagram));
  }
  for (const flow of diagram.flows) {
    canvas.append(drawFlow(flow, diagram));
  }
  holder.replaceChildren(canvas);
}

/** A subprocess first, so what stands in it is drawn over it. */
function depth(shape: Shape): number {
  return shape.kind === "subProcess" ? 0 : 1;
}

function drawShape(shape: Shape, diagram: Diagram): SVGElement {
  const group = svg("g", {
    class: `shape ${shape.kind}${chosen === shape.id ? " chosen" : ""}`,
    transform: `translate(${shape.x} ${shape.y})`,
  });
  const { width, height } = shape;
  if (EVENTS.includes(shape.kind) || BOUNDARY.includes(shape.kind)) {
    const radius = Math.min(width, height) / 2;
    group.append(svg("circle", { cx: radius, cy: radius, r: radius }));
    if (shape.kind === "timerCatch" || shape.kind === "timerBoundary") {
      group.append(svg("circle", { cx: radius, cy: radius, r: radius - 5, class: "inner" }));
    }
  } else if (shape.kind.endsWith("Gateway")) {
    const points = `${width / 2},0 ${width},${height / 2} ${width / 2},${height} 0,${height / 2}`;
    group.append(svg("polygon", { points }));
    const mark = svg("text", { x: width / 2, y: height / 2 + 6, class: "mark" });
    mark.textContent = shape.kind === "parallelGateway" ? "+" : "×";
    group.append(mark);
  } else {
    group.append(svg("rect", { width, height, rx: 10 }));
    const glyph = svg("text", { x: 8, y: 16, class: "glyph" });
    glyph.textContent = GLYPH[shape.kind] ?? "";
    group.append(glyph);
  }
  const caption = svg("text", { x: width / 2, y: inside(shape) ? height / 2 + 4 : height + 14 });
  caption.textContent = shape.name || shape.id;
  group.append(caption);
  const problem = diagram.problems.find((p) => p.at === shape.id);
  const title = svg("title", {});
  title.textContent = problem === undefined ? shape.kind : `${shape.kind}: ${problem.why}`;
  group.append(title);
  if (problem !== undefined) {
    group.classList.add("refused");
  }
  dragged(group, shape);
  return group;
}

/** Whether the caption stands inside the shape: a task's, not an event's. */
function inside(shape: Shape): boolean {
  const round = EVENTS.includes(shape.kind) || BOUNDARY.includes(shape.kind);
  return !round && !shape.kind.endsWith("Gateway");
}

/** The shape being dragged: where the press was, and whether it moved. */
let dragging:
  | { shape: Shape; group: SVGElement; x: number; y: number; moved: boolean }
  | undefined;

function dragged(group: SVGElement, shape: Shape): void {
  group.addEventListener("mousedown", (event) => {
    dragging = { shape, group, x: event.clientX, y: event.clientY, moved: false };
    event.stopPropagation();
  });
}

window.addEventListener("mousemove", (event) => {
  if (dragging === undefined) {
    return;
  }
  const { shape, group } = dragging;
  const [dx, dy] = [event.clientX - dragging.x, event.clientY - dragging.y];
  dragging.moved = dragging.moved || Math.abs(dx) + Math.abs(dy) > 3;
  group.setAttribute("transform", `translate(${shape.x + dx} ${shape.y + dy})`);
});

window.addEventListener("mouseup", (event) => {
  if (dragging === undefined) {
    return;
  }
  const { shape, moved } = dragging;
  const [dx, dy] = [event.clientX - dragging.x, event.clientY - dragging.y];
  dragging = undefined;
  if (moved) {
    edit({ edit: "move", id: shape.id, x: Math.round(shape.x + dx), y: Math.round(shape.y + dy) });
  } else {
    clicked(shape);
  }
});

function clicked(shape: Shape): void {
  if (adding !== undefined && BOUNDARY.includes(adding)) {
    const [x, y] = [shape.x + shape.width - 18, shape.y + shape.height - 18];
    edit({ edit: "add", kind: adding, x, y, attached: shape.id });
    adding = undefined;
    return;
  }
  if (connecting !== undefined) {
    if (connecting.from === undefined) {
      connecting.from = shape.id;
      tell("Click the step it leads to.");
    } else {
      edit({ edit: "connect", source: connecting.from, target: shape.id });
      connecting = undefined;
    }
    return;
  }
  choose(shape.id);
}

function drawFlow(flow: Flow, diagram: Diagram): SVGElement {
  const points = flow.waypoints.length >= 2 ? flow.waypoints : straight(flow, diagram);
  const line = svg("polyline", {
    points: points.map(([x, y]) => `${x},${y}`).join(" "),
    class: `flow${flow.default ? " default" : ""}${chosen === flow.id ? " chosen" : ""}`,
    "marker-end": "url(#arrow)",
  });
  line.addEventListener("click", (event) => {
    event.stopPropagation();
    choose(flow.id);
  });
  if (flow.condition !== null) {
    const title = svg("title", {});
    title.textContent = flow.condition;
    line.append(title);
  }
  return line;
}

/** A flow its design draws no waypoints for: from center to center. */
function straight(flow: Flow, diagram: Diagram): [number, number][] {
  const center = (id: string): [number, number] => {
    const shape = diagram.nodes.find((n) => n.id === id);
    return shape === undefined ? [0, 0] : [shape.x + shape.width / 2, shape.y + shape.height / 2];
  };
  return [center(flow.source), center(flow.target)];
}

// -- The properties panel ------------------------------------------------------

function choose(id: string | undefined): void {
  chosen = id;
  if (drawn !== undefined) {
    draw(drawn);
  }
}

function field(label: string, value: string, change: (value: string) => void): HTMLElement {
  const row = html("label", label);
  const input = html("input");
  input.value = value;
  input.addEventListener("change", () => change(input.value));
  row.append(input);
  return row;
}

function drawProperties(diagram: Diagram): void {
  properties.replaceChildren();
  const shape = diagram.nodes.find((n) => n.id === chosen);
  const flow = diagram.flows.find((f) => f.id === chosen);
  if (shape !== undefined) {
    properties.append(html("h2", shape.id), field("Name", shape.name, (name) => {
      edit({ edit: "rename", id: shape.id, name });
    }));
    shapeProperties(shape);
    properties.append(button("Remove", () => edit({ edit: "remove", id: shape.id })));
  } else if (flow !== undefined) {
    properties.append(html("h2", flow.id), field("Condition", flow.condition ?? "", (condition) => {
      edit({ edit: "condition", flow: flow.id, condition: condition === "" ? null : condition });
    }));
    properties.append(button("Remove", () => edit({ edit: "remove", id: flow.id })));
  } else {
    properties.append(html("h2", diagram.process || "No process yet"));
  }
  for (const problem of diagram.problems) {
    properties.append(html("p", `${problem.at}: ${problem.why}`, "refused"));
  }
}

function shapeProperties(shape: Shape): void {
  const extension = (name: string, value: object | null) =>
    edit({ edit: "extension", id: shape.id, name, value });
  if (shape.kind === "serviceTask") {
    const call = shape.call ?? { module: "", unit: "", settings: [] };
    const set = (part: Partial<Call>) => extension("call", { ...call, ...part });
    properties.append(
      field("Module", call.module, (module) => set({ module })),
      field("Unit", call.unit, (unit) => set({ unit })),
    );
  }
  if (shape.kind === "receiveTask") {
    const batch = shape.batch ?? { most: "", within: null };
    properties.append(
      field("Batch: most", batch.most, (most) => {
        extension("batch", most === "" ? null : { most, within: batch.within });
      }),
      field("Batch: within", batch.within ?? "", (within) => {
        extension("batch", { most: batch.most, within: within === "" ? null : within });
      }),
    );
  }
  if (["startEvent", "receiveTask", "messageCatch"].includes(shape.kind)) {
    properties.append(field("Correlation set", shape.set ?? "", (set) => {
      extension("correlate", set === "" ? null : { set });
    }));
  }
  if (shape.kind === "timerCatch" || shape.kind === "timerBoundary") {
    properties.append(field("Duration", shape.timer ?? "", (duration) => {
      edit({ edit: "timer", id: shape.id, duration });
    }));
  }
}

// -- The page ------------------------------------------------------------------

function draw(diagram: Diagram): void {
  drawn = diagram;
  byId("title").textContent = `Xmip Work Process ${diagram.process}`;
  drawPalette(diagram);
  drawCanvas(diagram);
  drawProperties(diagram);
}

byId("compile").addEventListener("click", () => host.postMessage({ type: "compile" }));

window.addEventListener("message", (event: MessageEvent<ToView>) => {
  const message = event.data;
  switch (message.type) {
    case "diagram": {
      draw(message.answer);
      const problems = message.answer.problems.length;
      tell(problems === 0 ? "" : `${problems} problems keep it from compiling`, problems > 0);
      return;
    }
    case "compiled":
      tell(`OK: the Work Process unit is written to ${message.folder}.`);
      return;
    case "refused":
      tell(`REFUSED: ${message.reason}`, true);
      return;
  }
});

host.postMessage({ type: "ready" });
