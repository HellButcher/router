import {
  type ControlPosition,
  type GeoJSONSource,
  type IControl,
  type Map as MaplibreMap,
  type MapMouseEvent,
  Marker,
} from "maplibre-gl";
import { client } from "../api/client.js";
import { decodePolyline } from "../api/polyline.js";
import { LL } from "../i18n/index.js";

// ── Public event / state types ─────────────────────────────────────────────

export const ROUTE_UPDATE_EVENT = "route-update";

export interface WaypointEntry {
  id: number;
  lat: number;
  lng: number;
}

export interface RouteResult {
  durationSecs: number;
  lengthM: number;
}

export interface RouteUpdate {
  waypoints: WaypointEntry[];
  result: RouteResult | null;
  error: string | null;
  loading: boolean;
}

// ── Helpers ────────────────────────────────────────────────────────────────

const EMPTY_GEOJSON: GeoJSON.FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

/** Convert `Points` (array or polyline string) to MapLibre [lng, lat] pairs. */
function pointsToCoords(path: number[][] | string): [number, number][] {
  const pairs: [number, number][] =
    typeof path === "string"
      ? decodePolyline(path)
      : (path as [number, number][]);
  // API returns [lat, lon]; GeoJSON / MapLibre needs [lon, lat]
  return pairs.map(([lat, lon]) => [lon, lat]);
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${secs} sec`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m < 60) return s > 0 ? `${m} min ${s} sec` : `${m} min`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm > 0 ? `${h} h ${rm} min` : `${h} h`;
}

function formatDistance(m: number): string {
  return m < 1000 ? `${m} m` : `${(m / 1000).toFixed(1)} km`;
}

// ── RouteControl ───────────────────────────────────────────────────────────

export class RouteControl implements IControl {
  private _map?: MaplibreMap;
  private _container?: HTMLElement;
  private _button?: HTMLButtonElement;
  private _active = false;

  private _waypoints: (WaypointEntry & { marker: Marker })[] = [];
  private _nextId = 0;

  private _abortController?: AbortController;
  private _result: RouteResult | null = null;
  private _error: string | null = null;
  private _loading = false;

  // ── IControl ──────────────────────────────────────────────────────────────

  getDefaultPosition(): ControlPosition {
    return "top-right";
  }

  onAdd(map: MaplibreMap): HTMLElement {
    this._map = map;

    this._container = document.createElement("div");
    this._container.className = "maplibregl-ctrl maplibregl-ctrl-group";

    const button = document.createElement("button");
    button.className = "maplibregl-ctrl-route";
    button.type = "button";
    button.title = LL().route.controlTitle();
    button.ariaLabel = LL().route.controlTitle();
    const icon = document.createElement("span");
    icon.className = "maplibregl-ctrl-icon";
    icon.ariaHidden = "true";
    button.appendChild(icon);
    this._button = button;

    button.addEventListener("click", () => this._toggleActive());
    this._container.appendChild(button);

    map.on("click", this._onClick);

    const initLayers = () => {
      map.addSource("route", { type: "geojson", data: EMPTY_GEOJSON });
      // White casing for legibility on all map styles
      map.addLayer({
        id: "route-line-casing",
        type: "line",
        source: "route",
        layout: { "line-join": "round", "line-cap": "round" },
        paint: { "line-color": "#fff", "line-width": 7, "line-opacity": 0.8 },
      });
      map.addLayer({
        id: "route-line",
        type: "line",
        source: "route",
        layout: { "line-join": "round", "line-cap": "round" },
        paint: { "line-color": "#0077cc", "line-width": 4 },
      });
    };

    if (map.isStyleLoaded()) {
      initLayers();
    } else {
      map.once("load", initLayers);
    }

    return this._container;
  }

  onRemove(): void {
    this._map?.off("click", this._onClick);
    this._clearMarkers();
    if (this._map?.getLayer("route-line")) this._map.removeLayer("route-line");
    if (this._map?.getLayer("route-line-casing"))
      this._map.removeLayer("route-line-casing");
    if (this._map?.getSource("route")) this._map.removeSource("route");
    this._container?.remove();
    this._container = undefined;
    this._map = undefined;
  }

  // ── Public API (called from sidebar) ─────────────────────────────────────

  /** Called by external code when another tool activates. */
  onActivate?: () => void;

  /** Activate route-adding mode (next map click will append a waypoint). */
  activate(): void {
    if (!this._active) this._toggleActive();
  }

  /** Deactivate route-adding mode without clearing waypoints. */
  deactivate(): void {
    if (this._active) this._toggleActive();
  }

  /** Remove a waypoint by id and re-calculate. */
  removeWaypoint(id: number): void {
    const idx = this._waypoints.findIndex((w) => w.id === id);
    if (idx === -1) return;
    this._waypoints[idx].marker.remove();
    this._waypoints.splice(idx, 1);
    this._refreshMarkerStyles();
    this._scheduleRoute();
  }

  /** Clear all waypoints and the route line. */
  clearAll(): void {
    this._clearMarkers();
    this._result = null;
    this._error = null;
    this._abortController?.abort();
    this._updateLine([]);
    this._dispatch();
  }

  // ── Getters (for initial sidebar render) ──────────────────────────────────

  get currentState(): RouteUpdate {
    return {
      waypoints: this._waypoints.map(({ id, lat, lng }) => ({ id, lat, lng })),
      result: this._result,
      error: this._error,
      loading: this._loading,
    };
  }

  static formatDuration = formatDuration;
  static formatDistance = formatDistance;

  // ── Private ───────────────────────────────────────────────────────────────

  private _toggleActive(): void {
    this._active = !this._active;
    this._button?.classList.toggle("active", this._active);
    if (this._active) this.onActivate?.();
    if (this._active && this._map) {
      this._map.getCanvas().style.cursor = "crosshair";
    } else if (this._map) {
      this._map.getCanvas().style.cursor = "";
    }
  }

  private _onClick = (e: MapMouseEvent): void => {
    if (!this._active) return;
    e.preventDefault();
    this._addWaypoint(e.lngLat.lat, e.lngLat.lng);
  };

  private _addWaypoint(lat: number, lng: number): void {
    if (!this._map) return;

    const id = this._nextId++;
    const el = document.createElement("div");
    el.className = "waypoint-marker";
    el.textContent = String(this._waypoints.length + 1);
    el.title = "Click to remove";
    el.addEventListener("click", (ev) => {
      ev.stopPropagation();
      this.removeWaypoint(id);
    });

    const marker = new Marker({ element: el, anchor: "center" })
      .setLngLat([lng, lat])
      .addTo(this._map);

    this._waypoints.push({ id, lat, lng, marker });
    this._refreshMarkerStyles();
    this._scheduleRoute();
  }

  private _refreshMarkerStyles(): void {
    this._waypoints.forEach(({ marker }, idx) => {
      const el = marker.getElement();
      el.textContent = String(idx + 1);
      el.classList.remove("start", "end", "via");
      if (idx === 0) el.classList.add("start");
      else if (idx === this._waypoints.length - 1) el.classList.add("end");
      else el.classList.add("via");
    });
  }

  private _clearMarkers(): void {
    for (const { marker } of this._waypoints) marker.remove();
    this._waypoints = [];
  }

  private _scheduleRoute(): void {
    this._dispatch(); // update sidebar immediately with new waypoints
    if (this._waypoints.length >= 2) {
      void this._fetchRoute();
    } else {
      this._result = null;
      this._error = null;
      this._updateLine([]);
      this._dispatch();
    }
  }

  avoidToll = false;
  avoidFerry = false;

  private async _fetchRoute(): Promise<void> {
    this._abortController?.abort();
    const ac = new AbortController();
    this._abortController = ac;

    this._loading = true;
    this._error = null;
    this._dispatch();

    const locations = this._waypoints.map(({ lat, lng }) => ({
      lat,
      lon: lng,
    }));

    try {
      const { data, error } = await client.POST("/api/v1/route", {
        body: {
          locations,
          avoid_toll: this.avoidToll || undefined,
          avoid_ferry: this.avoidFerry || undefined,
        },
        signal: ac.signal,
      });

      if (error) {
        this._error =
          (error as { detail?: string }).detail ??
          (error as { title?: string }).title ??
          "Route calculation failed";
        this._result = null;
        this._updateLine([]);
      } else if (data) {
        const coords: [number, number][] = [];
        for (const leg of data.legs) {
          coords.push(...pointsToCoords(leg.path));
        }
        this._updateLine(coords);
        this._result = {
          durationSecs: data.duration.secs,
          lengthM: data.length,
        };
        this._error = null;
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === "AbortError") return;
      this._error = "Network error";
      this._result = null;
      this._updateLine([]);
    } finally {
      this._loading = false;
      this._dispatch();
    }
  }

  private _updateLine(coords: [number, number][]): void {
    const source = this._map?.getSource("route") as GeoJSONSource | undefined;
    if (!source) return;
    if (coords.length < 2) {
      source.setData(EMPTY_GEOJSON);
      return;
    }
    source.setData({
      type: "Feature",
      geometry: { type: "LineString", coordinates: coords },
      properties: {},
    });
  }

  private _dispatch(): void {
    document.dispatchEvent(
      new CustomEvent<RouteUpdate>(ROUTE_UPDATE_EVENT, {
        detail: this.currentState,
      }),
    );
  }
}
