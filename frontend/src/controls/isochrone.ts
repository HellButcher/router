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

// ── Constants ──────────────────────────────────────────────────────────────

const LS_KEY = "router.isochrone";
const DEFAULT_RANGES = [5];

// Teal fill color, rendered outermost-first so inner rings sit on top.
const FILL_COLOR = "#009688";

// ── Public event / state types ─────────────────────────────────────────────

export const ISOCHRONE_UPDATE_EVENT = "isochrone-update";

export type IsochroneUnit = "km" | "mi" | "min";

export interface IsochroneUpdate {
  active: boolean;
  origin: { lat: number; lng: number } | null;
  unit: IsochroneUnit;
  ranges: number[];
  result: Array<{ value: number; polygon: string }> | null;
  error: string | null;
  loading: boolean;
}

// ── Helpers ────────────────────────────────────────────────────────────────

const EMPTY_GEOJSON: GeoJSON.FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

interface IsochronePrefs {
  ranges: number[];
  unit: IsochroneUnit;
}

function loadPrefs(): IsochronePrefs {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<IsochronePrefs>;
      const unit =
        p.unit === "km" || p.unit === "mi" || p.unit === "min" ? p.unit : "km";
      const ranges =
        Array.isArray(p.ranges) &&
        p.ranges.length > 0 &&
        p.ranges.every((v) => typeof v === "number" && v > 0)
          ? (p.ranges as number[])
          : [...DEFAULT_RANGES];
      return { unit, ranges };
    }
  } catch {}
  return { unit: "km", ranges: [...DEFAULT_RANGES] };
}

function savePrefs(unit: IsochroneUnit, ranges: number[]): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify({ unit, ranges }));
  } catch {}
}

/** Compute per-range fill opacity: innermost (smallest) is most opaque. */
function fillOpacity(rangeIndex: number, total: number): number {
  // rangeIndex 0 = smallest (innermost), total-1 = largest (outermost)
  // Render order: outermost first → innermost last (on top)
  const fraction = total <= 1 ? 1 : (total - 1 - rangeIndex) / (total - 1);
  return 0.1 + fraction * 0.2; // 0.10 (outer) … 0.30 (inner)
}

/** Convert polyline-encoded polygon to GeoJSON [lon, lat] coordinates. */
function polygonToCoords(polyline: string): [number, number][] {
  const pairs = decodePolyline(polyline);
  // API returns [lat, lon]; GeoJSON needs [lon, lat]
  return pairs.map(([lat, lon]) => [lon, lat]);
}

// ── IsochroneControl ───────────────────────────────────────────────────────

export class IsochroneControl implements IControl {
  private _map?: MaplibreMap;
  private _container?: HTMLElement;
  private _button?: HTMLButtonElement;
  private _active = false;

  private _originMarker?: Marker;
  private _origin: { lat: number; lng: number } | null = null;

  private _unit: IsochroneUnit;
  private _ranges: number[];

  constructor() {
    const prefs = loadPrefs();
    this._unit = prefs.unit;
    this._ranges = prefs.ranges;
  }

  private _abortController?: AbortController;
  private _result: Array<{ value: number; polygon: string }> | null = null;
  private _error: string | null = null;
  private _loading = false;

  // ── IControl ──────────────────────────────────────────────────────────

  getDefaultPosition(): ControlPosition {
    return "top-right";
  }

  onAdd(map: MaplibreMap): HTMLElement {
    this._map = map;

    this._container = document.createElement("div");
    this._container.className = "maplibregl-ctrl maplibregl-ctrl-group";

    const button = document.createElement("button");
    button.className = "maplibregl-ctrl-isochrone";
    button.type = "button";
    button.title = LL().isochrone.controlTitle();
    button.ariaLabel = LL().isochrone.controlTitle();
    const icon = document.createElement("span");
    icon.className = "maplibregl-ctrl-icon";
    icon.ariaHidden = "true";
    button.appendChild(icon);
    this._button = button;

    button.addEventListener("click", () => this._toggleActive());
    this._container.appendChild(button);

    map.on("click", this._onClick);

    const initLayers = () => {
      map.addSource("isochrone", { type: "geojson", data: EMPTY_GEOJSON });
      map.addLayer({
        id: "isochrone-fill",
        type: "fill",
        source: "isochrone",
        paint: {
          "fill-color": FILL_COLOR,
          "fill-opacity": ["get", "fillOpacity"] as unknown as number,
        },
      });
      map.addLayer({
        id: "isochrone-outline",
        type: "line",
        source: "isochrone",
        paint: {
          "line-color": FILL_COLOR,
          "line-width": 1.5,
          "line-opacity": 0.6,
        },
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
    this._originMarker?.remove();
    if (this._map?.getLayer("isochrone-fill"))
      this._map.removeLayer("isochrone-fill");
    if (this._map?.getLayer("isochrone-outline"))
      this._map.removeLayer("isochrone-outline");
    if (this._map?.getSource("isochrone")) this._map.removeSource("isochrone");
    this._container?.remove();
    this._container = undefined;
    this._map = undefined;
  }

  // ── Public API ─────────────────────────────────────────────────────────

  onActivate?: () => void;

  activate(): void {
    if (!this._active) this._toggleActive();
  }

  deactivate(): void {
    if (this._active) this._toggleActive();
  }

  setUnit(unit: IsochroneUnit): void {
    this._unit = unit;
    savePrefs(this._unit, this._ranges);
    this._scheduleCalculate();
  }

  setRange(index: number, value: number): void {
    if (index < 0 || index >= this._ranges.length) return;
    this._ranges[index] = value;
    savePrefs(this._unit, this._ranges);
    this._scheduleCalculate();
  }

  addRange(): void {
    const last = this._ranges[this._ranges.length - 1] ?? 5;
    this._ranges.push(last + 5);
    savePrefs(this._unit, this._ranges);
    this._dispatch();
    this._scheduleCalculate();
  }

  removeRange(index: number): void {
    if (this._ranges.length <= 1) return;
    this._ranges.splice(index, 1);
    savePrefs(this._unit, this._ranges);
    this._scheduleCalculate();
  }

  clearAll(): void {
    this._abortController?.abort();
    this._originMarker?.remove();
    this._originMarker = undefined;
    this._origin = null;
    this._result = null;
    this._error = null;
    this._loading = false;
    this._updateLayers([]);
    this._dispatch();
  }

  get currentState(): IsochroneUpdate {
    return {
      active: this._active,
      origin: this._origin,
      unit: this._unit,
      ranges: [...this._ranges],
      result: this._result,
      error: this._error,
      loading: this._loading,
    };
  }

  // ── Private ────────────────────────────────────────────────────────────

  private _toggleActive(): void {
    this._active = !this._active;
    this._button?.classList.toggle("active", this._active);
    if (this._active) {
      this.onActivate?.();
      if (this._map) this._map.getCanvas().style.cursor = "crosshair";
    } else if (this._map) {
      this._map.getCanvas().style.cursor = "";
    }
    this._dispatch();
  }

  private _onClick = (e: MapMouseEvent): void => {
    if (!this._active) return;
    e.preventDefault();
    this._setOrigin(e.lngLat.lat, e.lngLat.lng);
  };

  private _setOrigin(lat: number, lng: number): void {
    if (!this._map) return;

    this._originMarker?.remove();

    const el = document.createElement("div");
    el.className = "isochrone-origin-marker";

    this._originMarker = new Marker({ element: el, anchor: "center" })
      .setLngLat([lng, lat])
      .addTo(this._map);

    this._origin = { lat, lng };
    this._scheduleCalculate();
  }

  private _scheduleCalculate(): void {
    this._dispatch();
    if (this._origin) {
      void this._fetchIsochrone();
    }
  }

  private async _fetchIsochrone(): Promise<void> {
    if (!this._origin) return;

    this._abortController?.abort();
    const ac = new AbortController();
    this._abortController = ac;

    this._loading = true;
    this._error = null;
    this._dispatch();

    try {
      const { data, error } = await client.POST("/api/v1/isochrone", {
        body: {
          lat: this._origin.lat,
          lon: this._origin.lng,
          unit: this._unit,
          ranges: [...this._ranges],
        },
        signal: ac.signal,
      });

      if (error) {
        this._error =
          (error as { detail?: string }).detail ??
          (error as { title?: string }).title ??
          "Isochrone calculation failed";
        this._result = null;
        this._updateLayers([]);
      } else if (data) {
        this._result = data.ranges.map((r) => ({
          value: r.value,
          polygon: r.polygon as unknown as string,
        }));
        this._error = null;
        this._updateLayers(
          data.ranges as Array<{ value: number; polygon: unknown }>,
        );
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === "AbortError") return;
      this._error = "Network error";
      this._result = null;
      this._updateLayers([]);
    } finally {
      this._loading = false;
      this._dispatch();
    }
  }

  private _updateLayers(
    ranges: Array<{ value: number; polygon: unknown }>,
  ): void {
    const source = this._map?.getSource("isochrone") as
      | GeoJSONSource
      | undefined;
    if (!source) return;

    if (ranges.length === 0) {
      source.setData(EMPTY_GEOJSON);
      return;
    }

    // Sort largest first so smaller rings render on top.
    const sorted = [...ranges].sort((a, b) => b.value - a.value);
    const total = sorted.length;

    const features: GeoJSON.Feature[] = sorted.map((r, idx) => {
      const coords = polygonToCoords(r.polygon as string);
      // idx 0 = outermost (most transparent), idx total-1 = innermost (most opaque)
      const opacity = fillOpacity(total - 1 - idx, total);
      return {
        type: "Feature",
        geometry: {
          type: "Polygon",
          coordinates: [coords],
        },
        properties: { fillOpacity: opacity, value: r.value },
      };
    });

    source.setData({ type: "FeatureCollection", features });
  }

  private _dispatch(): void {
    document.dispatchEvent(
      new CustomEvent<IsochroneUpdate>(ISOCHRONE_UPDATE_EVENT, {
        detail: this.currentState,
      }),
    );
  }
}
