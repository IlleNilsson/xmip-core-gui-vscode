// What both parts of the designer's webview draw with: the host to post to,
// elements made in one call, and the status line. No rule lives here.

interface VsCode {
  postMessage(message: unknown): void;
}
declare function acquireVsCodeApi(): VsCode;

/** The extension host, acquired once for the page. */
export const host = acquireVsCodeApi();

const SVG = "http://www.w3.org/2000/svg";

export function byId(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (found === null) {
    throw new Error(`the page has no #${id}`);
  }
  return found;
}

export function html<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  content?: string,
  className?: string,
): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  if (content !== undefined) {
    made.textContent = content;
  }
  if (className !== undefined) {
    made.className = className;
  }
  return made;
}

export function svg(tag: string, attributes: Record<string, string | number>): SVGElement {
  const made = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) {
    made.setAttribute(name, String(value));
  }
  return made;
}

export function button(label: string, act: () => void): HTMLButtonElement {
  const made = html("button", label);
  made.type = "button";
  made.addEventListener("click", act);
  return made;
}

export function choice(
  options: string[],
  value: string,
  chose: (value: string) => void,
): HTMLSelectElement {
  const made = html("select");
  for (const option of options) {
    made.append(new Option(option, option, false, option === value));
  }
  made.addEventListener("change", () => chose(made.value));
  return made;
}

/** A kind as a person reads it: `send-port-group` is Send Port Group. */
export function words(kind: string): string {
  return kind
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** Say one sentence in the status line; a refusal in the error color. */
export function tell(sentence: string, refused = false): void {
  const said = byId("said");
  said.textContent = sentence;
  said.className = refused ? "refused" : "";
}

/** An edit to the cluster's file, made by the server. */
export function edit(change: object): void {
  host.postMessage({ type: "edit", edit: change });
}
