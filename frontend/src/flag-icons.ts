import { html, svg, type TemplateResult } from "lit-html";
import { unsafeSVG } from "lit-html/directives/unsafe-svg.js";
import {
  ArrowRight,
  Banknote,
  Bike,
  Car,
  Footprints,
  Milestone,
  Mountain,
  Ship,
  TrafficCone,
  Truck,
} from "lucide";

type IconNode = readonly (readonly [string, Record<string, string | number>])[];

function iconChildren(icon: IconNode, prohibited: boolean): string {
  const shapes = icon
    .map(([tag, attrs]) => {
      const attrStr = Object.entries(attrs)
        .map(([k, v]) => `${k}="${v}"`)
        .join(" ");
      return `<${tag} ${attrStr}/>`;
    })
    .join("");
  return shapes + (prohibited ? '<line x1="4" y1="4" x2="20" y2="20"/>' : "");
}

export function flagIcon(
  icon: IconNode,
  label: string,
  prohibited = false,
): TemplateResult {
  return html`<span class="flag-icon ${prohibited ? "flag-prohibited" : ""}" title="${label}">${svg`<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-label="${label}" role="img">${unsafeSVG(iconChildren(icon, prohibited))}</svg>`}</span>`;
}

export const FLAG_ICONS = {
  // way flags
  oneway: (label: string) => flagIcon(ArrowRight as IconNode, label, false),
  noMotor: (label: string) => flagIcon(Car as IconNode, label, true),
  noBicycle: (label: string) => flagIcon(Bike as IconNode, label, true),
  noFoot: (label: string) => flagIcon(Footprints as IconNode, label, true),
  noHgv: (label: string) => flagIcon(Truck as IconNode, label, true),
  toll: (label: string) => flagIcon(Banknote as IconNode, label, false),
  tunnel: (label: string) => flagIcon(Mountain as IconNode, label, false),
  bridge: (label: string) => flagIcon(Milestone as IconNode, label, false),
  ferry: (label: string) => flagIcon(Ship as IconNode, label, false),
  // node flags
  trafficSignals: (label: string) =>
    flagIcon(TrafficCone as IconNode, label, false),
} as const;
