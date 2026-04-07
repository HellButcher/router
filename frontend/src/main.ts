import "./style.css";
import { layers, namedFlavor } from "@protomaps/basemaps";
import { html, render } from "lit-html";
import {
  addProtocol,
  type ControlPosition,
  FullscreenControl,
  GeolocateControl,
  GlobeControl,
  type IControl,
  Map as MaplibreMap,
  NavigationControl,
  ScaleControl,
  setWorkerUrl,
} from "maplibre-gl";

import type { LocalizedString } from "typesafe-i18n";
import { initLocale, LL } from "./i18n/index.js";

class ToolGroupControl implements IControl {
  private _controls: IControl[];
  private _container?: HTMLElement;
  constructor(controls: IControl[]) {
    this._controls = controls;
  }
  getDefaultPosition(): ControlPosition {
    return "top-right";
  }
  onAdd(map: MaplibreMap): HTMLElement {
    this._container = document.createElement("div");
    this._container.className = "maplibregl-ctrl maplibregl-ctrl-group";
    for (const ctrl of this._controls) {
      const el = ctrl.onAdd(map);
      while (el.firstChild) this._container.appendChild(el.firstChild);
    }
    return this._container;
  }
  onRemove(map: MaplibreMap): void {
    for (const ctrl of this._controls) ctrl.onRemove(map);
    this._container?.remove();
    this._container = undefined;
  }
}

setWorkerUrl(maplibreWorkerUrl);

import maplibreWorkerUrl from "maplibre-gl/dist/maplibre-gl-csp-worker.js?url";

import { Protocol } from "pmtiles";
import {
  ISOCHRONE_UPDATE_EVENT,
  IsochroneControl,
  type IsochroneUnit,
  type IsochroneUpdate,
} from "./controls/isochrone";
import {
  ROUTE_UPDATE_EVENT,
  RouteControl,
  type RouteUpdate,
} from "./controls/route";
import {
  LOCATE_INFO_EVENT,
  type LocateInfo,
  SnapControl,
} from "./controls/snap";

const protocol = new Protocol();
addProtocol("pmtiles", protocol.tile);

const flavor = "light";

const map = new MaplibreMap({
  container: "map",
  center: [0, 0],
  zoom: 2,
  style: {
    version: 8,
    glyphs:
      "https://protomaps.github.io/basemaps-assets/fonts/{fontstack}/{range}.pbf",
    sprite: `https://protomaps.github.io/basemaps-assets/sprites/v4/${flavor}`,
    sources: {
      example_source: {
        type: "vector",
        url: "pmtiles://https://demo-bucket.protomaps.com/v4.pmtiles",
        attribution:
          "<a href='https://protomaps.com'>Protomaps</a> <a href='https://openstreetmap.org/copyright'>© OpenStreetMap Contributors</a>",
      },
    },
    layers: layers("example_source", namedFlavor(flavor), { lang: "en" }),
  },
});

map.addControl(new NavigationControl());
map.addControl(new ScaleControl());
map.addControl(new FullscreenControl());
map.addControl(new GlobeControl());
map.addControl(new GeolocateControl({}));

const SIDEBAR_OPEN_CLASS = "sidebar-open";
const SIDEBAR_TOGGLE_EVENT = "sidebar-toggle";

function toggleSidebar(shouldOpen?: boolean) {
  const isOpen = document.body.classList.contains(SIDEBAR_OPEN_CLASS);
  if (shouldOpen === undefined) {
    shouldOpen = !isOpen;
  }
  if (isOpen === shouldOpen) return;
  if (shouldOpen) {
    document.body.classList.add(SIDEBAR_OPEN_CLASS);
  } else {
    document.body.classList.remove(SIDEBAR_OPEN_CLASS);
  }
  document.dispatchEvent(
    new CustomEvent(SIDEBAR_TOGGLE_EVENT, { detail: { open: shouldOpen } }),
  );
}

class SidebarToggleControl implements IControl {
  _map?: MaplibreMap;
  _container?: HTMLElement;

  getDefaultPosition(): ControlPosition {
    return "top-left";
  }

  onAdd(map: MaplibreMap) {
    this._map = map;
    this._container = document.createElement("div");
    this._container.className = "maplibregl-ctrl maplibregl-ctrl-group";
    const button = document.createElement("button");
    button.className = "maplibregl-ctrl-sidebar-toggle";
    button.type = "button";

    function updateLabel() {
      const open = document.body.classList.contains(SIDEBAR_OPEN_CLASS);
      const label = open ? LL().sidebar.closeLabel() : LL().sidebar.openLabel();
      button.ariaLabel = label;
      button.title = label;
    }
    document.addEventListener(SIDEBAR_TOGGLE_EVENT, updateLabel);
    updateLabel();
    button.addEventListener("click", () => toggleSidebar());

    const iconSpan = document.createElement("span");
    iconSpan.className = "maplibregl-ctrl-icon";
    iconSpan.ariaHidden = "true";
    button.appendChild(iconSpan);

    this._container.appendChild(button);
    return this._container;
  }

  onRemove() {
    this._container?.remove();
    this._container = undefined;
    this._map = undefined;
  }
}

// ── Sidebar ─────────────────────────────────────────────────────────────────

let routeControl: RouteControl | null = null;
let routeState: RouteUpdate = {
  waypoints: [],
  result: null,
  error: null,
  loading: false,
};
let locateInfo: LocateInfo | null = null;
let isochroneControl: IsochroneControl | null = null;
let isochroneState: IsochroneUpdate = {
  active: false,
  origin: null,
  unit: "km",
  ranges: [5],
  result: null,
  error: null,
  loading: false,
};

document.addEventListener(LOCATE_INFO_EVENT, (e) => {
  locateInfo = (e as CustomEvent<LocateInfo | null>).detail;
  if (locateInfo) toggleSidebar(true);
  updateSidebar();
});

document.addEventListener(ISOCHRONE_UPDATE_EVENT, (e) => {
  isochroneState = (e as CustomEvent<IsochroneUpdate>).detail;
  updateSidebar();
});

function dotClass(idx: number, total: number) {
  if (idx === 0) return "start";
  if (idx === total - 1) return "end";
  return "via";
}

function formatCoord(v: number) {
  return v.toFixed(5);
}

function flagList(way: NonNullable<LocateInfo["way"]>): LocalizedString[] {
  const t = LL().locate.flags;
  return [
    way.oneway ? t.oneway() : null,
    way.no_motor ? t.noMotor() : null,
    way.no_bicycle ? t.noBicycle() : null,
    way.no_foot ? t.noFoot() : null,
    way.no_hgv ? t.noHgv() : null,
  ].filter((f): f is LocalizedString => f !== null);
}

function unitLabel(unit: IsochroneUnit): string {
  const t = LL();
  if (unit === "km") return t.isochrone.unitKm();
  if (unit === "mi") return t.isochrone.unitMi();
  return t.isochrone.unitMin();
}

function sidebarTemplate() {
  const t = LL();
  const { waypoints, result, error, loading } = routeState;
  const hasWaypoints = waypoints.length > 0;

  return html`
    <header>
      <h1>${t.app.title()}</h1>
      <h2>${__APP_VERSION__}</h2>
    </header>

    <section class="route-waypoints">
      <h3>${t.route.waypoints()}</h3>
      ${
        waypoints.length === 0
          ? html`<p class="route-empty-hint">${t.route.noWaypoints()}</p>`
          : waypoints.map(
              (wp, idx) => html`
              <div class="route-waypoint-item">
                <div class="route-waypoint-dot ${dotClass(idx, waypoints.length)}">
                  ${idx + 1}
                </div>
                <span class="route-waypoint-coords">
                  ${formatCoord(wp.lat)}, ${formatCoord(wp.lng)}
                </span>
                <button
                  class="route-waypoint-remove"
                  title="${t.route.removeWaypoint()}"
                  @click=${() => routeControl?.removeWaypoint(wp.id)}
                >
                  ×
                </button>
              </div>
            `,
            )
      }
    </section>

    <div class="route-actions">
      <button
        class="route-btn primary"
        @click=${() => {
          routeControl?.activate();
          toggleSidebar(false);
        }}
      >
        ${t.route.addWaypoint()}
      </button>
      ${
        hasWaypoints
          ? html`<button
            class="route-btn danger"
            @click=${() => routeControl?.clearAll()}
          >
            ${t.route.clear()}
          </button>`
          : ""
      }
    </div>

    ${loading ? html`<div class="route-loading">${t.route.calculating()}</div>` : ""}

    ${error ? html`<div class="route-error">⚠ ${error}</div>` : ""}

    ${
      result && !loading
        ? html`
          <div class="route-summary">
            <div class="route-summary-row">
              <span class="route-summary-label">${t.route.duration()}</span>
              <span class="route-summary-value">
                ${RouteControl.formatDuration(result.durationSecs)}
              </span>
            </div>
            <div class="route-summary-row">
              <span class="route-summary-label">${t.route.distance()}</span>
              <span class="route-summary-value">
                ${RouteControl.formatDistance(result.lengthM)}
              </span>
            </div>
          </div>
        `
        : ""
    }

    <section class="isochrone-section">
      <h3>${t.isochrone.controlTitle()}</h3>
      ${
        isochroneState.origin
          ? html`<div class="isochrone-origin">
              ${formatCoord(isochroneState.origin.lat)}, ${formatCoord(isochroneState.origin.lng)}
            </div>`
          : html`<p class="route-empty-hint">${t.isochrone.noOrigin()}</p>`
      }

      <div class="isochrone-unit-row">
        <span class="isochrone-label">${t.isochrone.unit()}</span>
        <select
          class="isochrone-select"
          @change=${(e: Event) =>
            isochroneControl?.setUnit(
              (e.target as HTMLSelectElement).value as IsochroneUnit,
            )}
        >
          ${(["km", "mi", "min"] as IsochroneUnit[]).map(
            (u) =>
              html`<option value=${u} ?selected=${isochroneState.unit === u}>
                ${unitLabel(u)}
              </option>`,
          )}
        </select>
      </div>

      <div class="isochrone-ranges">
        <span class="isochrone-label">${t.isochrone.ranges()}</span>
        ${isochroneState.ranges.map(
          (val, idx) => html`
            <div class="isochrone-range-row">
              <input
                class="isochrone-range-input"
                type="number"
                min="0.1"
                step="1"
                .value=${String(val)}
                @change=${(e: Event) => {
                  const n = parseFloat((e.target as HTMLInputElement).value);
                  if (n > 0) isochroneControl?.setRange(idx, n);
                }}
              />
              <button
                class="isochrone-range-remove"
                title=${t.isochrone.removeRange()}
                ?disabled=${isochroneState.ranges.length <= 1}
                @click=${() => isochroneControl?.removeRange(idx)}
              >×</button>
            </div>
          `,
        )}
        <button
          class="route-btn"
          style="margin-top:4px"
          @click=${() => isochroneControl?.addRange()}
        >${t.isochrone.addRange()}</button>
      </div>

      <div class="route-actions" style="padding:8px 0 0">
        <button
          class="route-btn primary"
          @click=${() => {
            isochroneControl?.activate();
            toggleSidebar(false);
          }}
        >${isochroneState.origin ? t.isochrone.origin() : t.isochrone.noOrigin()}</button>
        ${
          isochroneState.origin
            ? html`<button class="route-btn danger" @click=${() => isochroneControl?.clearAll()}>
                ${t.isochrone.clear()}
              </button>`
            : ""
        }
      </div>

      ${isochroneState.loading ? html`<div class="route-loading">${t.isochrone.calculating()}</div>` : ""}
      ${isochroneState.error ? html`<div class="route-error">⚠ ${isochroneState.error}</div>` : ""}
    </section>

    ${
      locateInfo
        ? html`
          <section class="locate-info">
            <h3>${locateInfo.way ? t.locate.inspect.titleWay() : t.locate.inspect.titleNode()}</h3>
            <button class="locate-info-close" @click=${() => {
              locateInfo = null;
              updateSidebar();
            }}>×</button>
            ${
              locateInfo.node
                ? html`
              <div class="locate-info-row"><span>${t.locate.inspect.osmNodeId()}</span><span>${locateInfo.node.id}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.lat()}</span><span>${locateInfo.node.lat.toFixed(6)}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.lon()}</span><span>${locateInfo.node.lon.toFixed(6)}</span></div>
            `
                : ""
            }
            ${
              locateInfo.way
                ? html`
              <div class="locate-info-row"><span>${t.locate.inspect.osmWayId()}</span><span>${locateInfo.way.id}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.highway()}</span><span>${locateInfo.way.highway}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.maxSpeed()}</span><span>${locateInfo.way.max_speed > 0 ? t.locate.inspect.maxSpeedKmh({ speed: locateInfo.way.max_speed }) : t.locate.inspect.maxSpeedDefault()}</span></div>
              ${flagList(locateInfo.way).length > 0 ? html`<div class="locate-info-row"><span>${t.locate.inspect.flags()}</span><span>${flagList(locateInfo.way).join(", ")}</span></div>` : ""}
              <div class="locate-info-subheader">${t.locate.inspect.fromNode()}</div>
              <div class="locate-info-row"><span>${t.locate.inspect.osmNodeId()}</span><span>${locateInfo.way.from_node.id}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.position()}</span><span>${locateInfo.way.from_node.lat.toFixed(5)}, ${locateInfo.way.from_node.lon.toFixed(5)}</span></div>
              <div class="locate-info-subheader">${t.locate.inspect.toNode()}</div>
              <div class="locate-info-row"><span>${t.locate.inspect.osmNodeId()}</span><span>${locateInfo.way.to_node.id}</span></div>
              <div class="locate-info-row"><span>${t.locate.inspect.position()}</span><span>${locateInfo.way.to_node.lat.toFixed(5)}, ${locateInfo.way.to_node.lon.toFixed(5)}</span></div>
            `
                : ""
            }
          </section>
        `
        : ""
    }

    <footer>
      ${t.sidebar.footer()}
    </footer>
  `;
}

const sideBarElement = document.getElementById("sidebar");

function updateSidebar() {
  if (sideBarElement) render(sidebarTemplate(), sideBarElement);
}

// ── Boot ─────────────────────────────────────────────────────────────────────

function initControls() {
  const snapCtrl = new SnapControl();
  routeControl = new RouteControl();
  isochroneControl = new IsochroneControl();

  snapCtrl.onActivate = () => {
    routeControl?.deactivate();
    isochroneControl?.deactivate();
  };
  routeControl.onActivate = () => {
    snapCtrl.deactivate();
    isochroneControl?.deactivate();
  };
  isochroneControl.onActivate = () => {
    snapCtrl.deactivate();
    routeControl?.deactivate();
  };

  routeState = routeControl.currentState;
  isochroneState = isochroneControl.currentState;

  document.addEventListener(ROUTE_UPDATE_EVENT, (e) => {
    routeState = (e as CustomEvent<RouteUpdate>).detail;
    updateSidebar();
  });

  map.addControl(new SidebarToggleControl());
  map.addControl(
    new ToolGroupControl([snapCtrl, routeControl, isochroneControl]),
  );
}

initLocale().then(() => {
  initControls();
  updateSidebar();
});
