import { h, render, type Component } from "vue";

export function appendIcon(container: HTMLElement, icon: Component, label?: string): HTMLElement {
  const host = document.createElement("span");
  host.className = "ui-icon";
  if (label) {
    host.setAttribute("aria-label", label);
  } else {
    host.setAttribute("aria-hidden", "true");
  }
  render(h(icon, { size: "1em" }), host);
  container.appendChild(host);
  return host;
}

export function setButtonIcon(button: HTMLButtonElement, icon: Component, label: string): void {
  button.replaceChildren();
  button.setAttribute("aria-label", label);
  appendIcon(button, icon);
}
