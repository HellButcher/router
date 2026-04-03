import {
  type ControlPosition,
  type IControl,
  type Map as MaplibreMap,
  type MapMouseEvent,
  Marker,
} from "maplibre-gl";
import { client } from "../api/client.js";

export class SnapControl implements IControl {
  private _map?: MaplibreMap;
  private _container?: HTMLElement;
  private _button?: HTMLButtonElement;
  private _active = false;
  private _marker?: Marker;
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
    button.title = "Snap to nearest node";
    button.ariaLabel = "Snap to nearest node";
    const icon = document.createElement("span");
    icon.className = "maplibregl-ctrl-icon";
    icon.ariaHidden = "true";
    button.appendChild(icon);
    this._button = button;

    button.addEventListener("click", () => this._toggle());
    this._container.appendChild(button);

    map.on("mousemove", this._onMouseMove);
    map.on("mouseout", this._onMouseOut);

    return this._container;
  }

  onRemove(): void {
    this._map?.off("mousemove", this._onMouseMove);
    this._map?.off("mouseout", this._onMouseOut);
    this._marker?.remove();
    this._container?.remove();
    this._container = undefined;
    this._map = undefined;
  }

  private _toggle() {
    this._active = !this._active;
    this._button?.classList.toggle("active", this._active);
    if (!this._active) {
      this._marker?.remove();
      this._marker = undefined;
    }
  }

  private _onMouseMove = (e: MapMouseEvent) => {
    if (!this._active) return;
    this._pendingLatLng = { lat: e.lngLat.lat, lng: e.lngLat.lng };
    if (!this._inflight) this._flush();
  };

  private _onMouseOut = () => {
    if (!this._active) return;
    this._pendingLatLng = undefined;
    this._marker?.remove();
    this._marker = undefined;
  };

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
        body: { locations: [{ lat, lon: lng }] },
        signal: ac.signal,
      });
      const loc = data?.locations?.[0];
      if (!loc) return;

      const snappedLat = loc.lat;
      const snappedLng = loc.lon;

      if (this._marker) {
        this._marker.setLngLat([snappedLng, snappedLat]);
      } else {
        const el = document.createElement("div");
        el.className = "snap-marker";
        this._marker = new Marker({ element: el })
          .setLngLat([snappedLng, snappedLat])
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
