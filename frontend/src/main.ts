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
import maplibreWorkerUrl from "maplibre-gl/dist/maplibre-gl-csp-worker.js?url";

setWorkerUrl(maplibreWorkerUrl);

import { Protocol } from "pmtiles";
import {
  ROUTE_UPDATE_EVENT,
  RouteControl,
  type RouteUpdate,
} from "./controls/route";
import { SnapControl } from "./controls/snap";

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
      const label = open ? "Close Sidebar" : "Open Sidebar";
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

// ── Controls ────────────────────────────────────────────────────────────────

map.addControl(new SidebarToggleControl());
map.addControl(new SnapControl());

const routeControl = new RouteControl();
map.addControl(routeControl);

// ── Sidebar ─────────────────────────────────────────────────────────────────

let routeState: RouteUpdate = routeControl.currentState;

document.addEventListener(ROUTE_UPDATE_EVENT, (e) => {
  routeState = (e as CustomEvent<RouteUpdate>).detail;
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

function sidebarTemplate() {
  const { waypoints, result, error, loading } = routeState;
  const hasWaypoints = waypoints.length > 0;

  return html`
    <header>
      <h1>Router</h1>
      <h2>${__APP_VERSION__}</h2>
    </header>

    <section class="route-waypoints">
      <h3>Waypoints</h3>
      ${
        waypoints.length === 0
          ? html`<p style="font-size:0.82em;color:#999;padding:4px 0;">
            No waypoints yet. Activate the route tool and click on the map.
          </p>`
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
                  title="Remove waypoint"
                  @click=${() => routeControl.removeWaypoint(wp.id)}
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
          routeControl.activate();
          toggleSidebar(false);
        }}
      >
        + Add waypoint
      </button>
      ${
        hasWaypoints
          ? html`<button
            class="route-btn danger"
            @click=${() => routeControl.clearAll()}
          >
            Clear
          </button>`
          : ""
      }
    </div>

    ${loading ? html`<div class="route-loading">Calculating route…</div>` : ""}

    ${error ? html`<div class="route-error">⚠ ${error}</div>` : ""}

    ${
      result && !loading
        ? html`
          <div class="route-summary">
            <div class="route-summary-row">
              <span class="route-summary-label">Duration</span>
              <span class="route-summary-value">
                ${RouteControl.formatDuration(result.durationSecs)}
              </span>
            </div>
            <div class="route-summary-row">
              <span class="route-summary-label">Distance</span>
              <span class="route-summary-value">
                ${RouteControl.formatDistance(result.lengthM)}
              </span>
            </div>
          </div>
        `
        : ""
    }

    <footer>
      <span style="font-size:0.75em;color:#999;padding:8px 12px;display:block;">
        Click on the map to add waypoints when route tool is active.
        Click a waypoint marker to remove it.
      </span>
    </footer>
  `;
}

const sideBarElement = document.getElementById("sidebar");

function updateSidebar() {
  if (sideBarElement) render(sidebarTemplate(), sideBarElement);
}

updateSidebar();
