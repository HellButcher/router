import {
  type ControlPosition,
  type GeoJSONSource,
  type IControl,
  type Map as MaplibreMap,
  type MapMouseEvent,
  Marker,
} from "maplibre-gl";
import { client } from "../api/client.js";
import type { components } from "../api/types.js";
import { LL } from "../i18n/index.js";

type Location = components["schemas"]["Location"];
type NodeMeta = components["schemas"]["NodeMeta"];
type EdgeMeta = components["schemas"]["EdgeMeta"];
type WayMeta = components["schemas"]["WayMeta"];

// ── public event ─────────────────────────────────────────────────────────────

export const LOCATE_INFO_EVENT = "locate-info";

export interface LocateInfo {
  location: Location;
  node?: NodeMeta;
  nodes?: NodeMeta[];
  way?: WayMeta;
  edge?: EdgeMeta;
}

// ── modes ─────────────────────────────────────────────────────────────────────

type Mode = "off" | "node" | "edge";

const MODE_TITLES: Record<Mode, string> = {
  get off() {
    return LL().locate.control.off();
  },
  get node() {
    return LL().locate.control.node();
  },
  get edge() {
    return LL().locate.control.edge();
  },
};

const PIN_SEGMENT_SOURCE = "_locate-pin-segment";
const PIN_SEGMENT_LAYER = "_locate-pin-segment";

// ── control ───────────────────────────────────────────────────────────────────

export class SnapControl implements IControl {
  private _map?: MaplibreMap;
  private _container?: HTMLElement;
  private _button?: HTMLButtonElement;

  /** Called by external code when another tool activates. */
  onActivate?: () => void;

  private _mode: Mode = "off";

  // hover state (blue)
  private _hoverMarker?: Marker;
  private _currentInfo?: LocateInfo;

  // pinned state (red) — set on click, cleared when mode changes
  private _pinMarker?: Marker;
  private _pinnedInfo?: LocateInfo;

  // request debounce
  private _abortController?: AbortController;
  private _pendingLatLng?: { lat: number; lng: number };
  private _inflight = false;

  getDefaultPosition(): ControlPosition {
    return "top-right";
  }

  onAdd(map: MaplibreMap): HTMLElement {
    this._map = map;

    this._container = document.createElement("div");
    this._container.className = "maplibregl-ctrl maplibregl-ctrl-group";

    const button = document.createElement("button");
    button.className = "maplibregl-ctrl-snap";
    button.type = "button";
    const icon = document.createElement("span");
    icon.className = "maplibregl-ctrl-icon";
    icon.ariaHidden = "true";
    button.appendChild(icon);
    this._button = button;
    this._container.appendChild(button);

    button.addEventListener("click", () => this._cycleMode());
    this._updateButton();

    map.on("mousemove", this._onMouseMove);
    map.on("mouseout", this._onMouseOut);
    map.on("click", this._onClick);

    if (map.isStyleLoaded()) {
      this._addLayers();
    } else {
      map.once("load", () => this._addLayers());
    }

    return this._container;
  }

  onRemove(): void {
    this._map?.off("mousemove", this._onMouseMove);
    this._map?.off("mouseout", this._onMouseOut);
    this._map?.off("click", this._onClick);
    this._removeLayers();
    this._hoverMarker?.remove();
    this._pinMarker?.remove();
    this._container?.remove();
    this._container = undefined;
    this._map = undefined;
  }

  // ── layers ──────────────────────────────────────────────────────────────────

  private _addLayers() {
    if (!this._map) return;
    if (!this._map.getSource(PIN_SEGMENT_SOURCE)) {
      this._map.addSource(PIN_SEGMENT_SOURCE, {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });
    }
    // Layer is added lazily in _applyPin so it lands on top of all route layers.
  }

  private _ensureSegmentLayer() {
    if (!this._map) return;
    if (!this._map.getLayer(PIN_SEGMENT_LAYER)) {
      this._map.addLayer({
        id: PIN_SEGMENT_LAYER,
        type: "line",
        source: PIN_SEGMENT_SOURCE,
        paint: {
          "line-color": "#e03030",
          "line-width": 4,
          "line-opacity": 0.85,
        },
      });
    }
  }

  private _removeLayers() {
    if (!this._map) return;
    if (this._map.getLayer(PIN_SEGMENT_LAYER))
      this._map.removeLayer(PIN_SEGMENT_LAYER);
    if (this._map.getSource(PIN_SEGMENT_SOURCE))
      this._map.removeSource(PIN_SEGMENT_SOURCE);
  }

  private _setSegment(sourceId: string, nodes: NodeMeta[] | null) {
    const src = this._map?.getSource(sourceId) as GeoJSONSource | undefined;
    if (!src) return;
    if (!nodes || nodes.length < 2) {
      src.setData({ type: "FeatureCollection", features: [] });
      return;
    }
    src.setData({
      type: "Feature",
      geometry: {
        type: "LineString",
        coordinates: nodes.map((n) => [n.lon, n.lat]),
      },
      properties: {},
    });
  }

  // ── mode cycling ────────────────────────────────────────────────────────────

  private _cycleMode() {
    const next: Record<Mode, Mode> = { off: "node", node: "edge", edge: "off" };
    this._setMode(next[this._mode]);
  }

  /** Deactivate the locate control (set mode to off). */
  deactivate(): void {
    this._setMode("off");
  }

  private _setMode(mode: Mode) {
    this._mode = mode;
    this._updateButton();
    this._clearHover();
    if (mode === "off") {
      this._clearPin();
    } else {
      this.onActivate?.();
    }
  }

  private _updateButton() {
    const btn = this._button;
    if (!btn) return;
    btn.classList.toggle("active", this._mode !== "off");
    btn.classList.toggle("mode-node", this._mode === "node");
    btn.classList.toggle("mode-edge", this._mode === "edge");
    btn.title = MODE_TITLES[this._mode];
    btn.ariaLabel = MODE_TITLES[this._mode];
  }

  // ── hover helpers ────────────────────────────────────────────────────────────

  private _clearHover() {
    this._hoverMarker?.remove();
    this._hoverMarker = undefined;
    this._currentInfo = undefined;
  }

  // ── pin helpers ──────────────────────────────────────────────────────────────

  private _clearPin() {
    this._pinMarker?.remove();
    this._pinMarker = undefined;
    this._setSegment(PIN_SEGMENT_SOURCE, null);
    this._pinnedInfo = undefined;
    document.dispatchEvent(
      new CustomEvent<null>(LOCATE_INFO_EVENT, { detail: null }),
    );
  }

  private _applyPin(info: LocateInfo) {
    if (!this._map) return;
    this._pinnedInfo = info;

    // Pinned snap marker (red)
    this._pinMarker?.remove();
    const el = document.createElement("div");
    el.className = "snap-marker snap-marker--pinned";
    this._pinMarker = new Marker({ element: el })
      .setLngLat([info.location.lon, info.location.lat])
      .addTo(this._map);

    // Pinned segment (red, edge mode only) — layer created here so it sits above route layers.
    this._ensureSegmentLayer();
    this._setSegment(PIN_SEGMENT_SOURCE, info.nodes ?? null);

    // Notify sidebar
    document.dispatchEvent(
      new CustomEvent<LocateInfo>(LOCATE_INFO_EVENT, { detail: info }),
    );
  }

  // ── mouse handlers ──────────────────────────────────────────────────────────

  private _onMouseMove = (e: MapMouseEvent) => {
    if (this._mode === "off") return;
    this._pendingLatLng = { lat: e.lngLat.lat, lng: e.lngLat.lng };
    if (!this._inflight) this._flush();
  };

  private _onMouseOut = () => {
    if (this._mode === "off") return;
    this._pendingLatLng = undefined;
    this._clearHover();
  };

  private _onClick = async () => {
    if (this._mode === "off") return;
    if (this._currentInfo) {
      const { lat, lon } = this._currentInfo.location;
      try {
        const { data } = await client.POST("/api/v1/locate", {
          body: {
            locations: [{ lat, lon }],
            snap_mode: this._mode === "node" ? "Node" : "Edge",
            with_meta: true,
          },
        });
        const loc = data?.locations?.[0];
        if (loc) {
          const nodeMeta = loc.node_meta;
          this._applyPin({
            location: loc,
            node: nodeMeta && !Array.isArray(nodeMeta) ? nodeMeta : undefined,
            nodes: Array.isArray(nodeMeta) ? nodeMeta : undefined,
            way: loc.way_meta ?? undefined,
            edge: loc.edge_meta ?? undefined,
          });
        }
      } catch {
        // ignore
      }
    } else if (this._pinnedInfo) {
      this._clearPin();
    }
  };

  // ── fetch ────────────────────────────────────────────────────────────────────

  private async _flush() {
    if (!this._pendingLatLng || !this._map) return;
    const { lat, lng } = this._pendingLatLng;
    this._pendingLatLng = undefined;
    this._inflight = true;

    this._abortController?.abort();
    const ac = new AbortController();
    this._abortController = ac;

    try {
      const { data } = await client.POST("/api/v1/locate", {
        body: {
          locations: [{ lat, lon: lng }],
          snap_mode: this._mode === "node" ? "Node" : "Edge",
        },
        signal: ac.signal,
      });

      const loc = data?.locations?.[0];
      if (!loc) return;

      const nodeMeta = loc.node_meta;
      const info: LocateInfo = {
        location: loc,
        node: nodeMeta && !Array.isArray(nodeMeta) ? nodeMeta : undefined,
        nodes: Array.isArray(nodeMeta) ? nodeMeta : undefined,
        way: loc.way_meta ?? undefined,
        edge: loc.edge_meta ?? undefined,
      };
      this._currentInfo = info;

      // Update hover marker
      if (this._hoverMarker) {
        this._hoverMarker.setLngLat([loc.lon, loc.lat]);
      } else {
        const el = document.createElement("div");
        el.className = "snap-marker";
        this._hoverMarker = new Marker({ element: el })
          .setLngLat([loc.lon, loc.lat])
          .addTo(this._map);
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === "AbortError") return;
    } finally {
      this._inflight = false;
      if (this._pendingLatLng) this._flush();
    }
  }
}
