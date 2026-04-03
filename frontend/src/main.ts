import './style.css'
import { Map, addProtocol, FullscreenControl, GeolocateControl, NavigationControl, ScaleControl, GlobeControl, type IControl, type ControlPosition } from 'maplibre-gl';
import { SnapControl } from './controls/snap';
import { Protocol } from "pmtiles";
import { layers, namedFlavor } from '@protomaps/basemaps';
import { html, render } from 'lit-html';

const protocol = new Protocol();
addProtocol("pmtiles", protocol.tile);

//    url: "https://build.protomaps.com/20260328.pmtiles",
//    attributions: ["© <a href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\">OpenStreetMap</a> contributors."],

const flavor = "light";


const map = new Map({
  container: 'map',
  center: [0, 0],
  zoom: 2,
  style: {
    version: 8,
    glyphs: "https://protomaps.github.io/basemaps-assets/fonts/{fontstack}/{range}.pbf",
    sprite: `https://protomaps.github.io/basemaps-assets/sprites/v4/${flavor}`,
    sources: {
      example_source: {
        type: "vector",
        url: "pmtiles://https://demo-bucket.protomaps.com/v4.pmtiles",
        attribution: "<a href='https://protomaps.com'>Protomaps</a> <a href='https://openstreetmap.org/copyright'>© OpenStreetMap Contributors</a>"
      },
    },
    layers: layers("example_source", namedFlavor(flavor), { lang: "en" })
  },
});

map.addControl(new NavigationControl());
map.addControl(new ScaleControl());
map.addControl(new FullscreenControl());
map.addControl(new GlobeControl());
map.addControl(new GeolocateControl({}));

const SIDEBAR_OPEN_CLASS = 'sidebar-open';
const SIDEBAR_TOGGLE_EVENT = 'sidebar-toggle';

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
  document.dispatchEvent(new CustomEvent(SIDEBAR_TOGGLE_EVENT, { detail: { open: shouldOpen } }));
}

class SidebarToggleControl implements IControl {
  _map?: Map;
  _container?: HTMLElement;

  getDefaultPosition(): ControlPosition {
    return 'top-left';
  }

  onAdd(map: Map) {
    this._map = map;
    this._container = document.createElement('div');
    this._container.className = 'maplibregl-ctrl maplibregl-ctrl-group';
    const button = document.createElement('button');
    button.className = "maplibregl-ctrl-sidebar-toggle";
    button.type = "button";

    function updateLabel() {
      const open = document.body.classList.contains(SIDEBAR_OPEN_CLASS);
      const label = open  ? 'Close Sidebar' : 'Open Sidebar';
      button.ariaLabel = label;
      button.title = label;
    }
    document.addEventListener(SIDEBAR_TOGGLE_EVENT, updateLabel);
    updateLabel();
    button.addEventListener('click', () => toggleSidebar());

    const iconSpan = document.createElement('span');
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

map.addControl(new SidebarToggleControl());
map.addControl(new SnapControl());

const renderSidebar = () => html`
  <header>
    <h1>Router</h1>
  </header>
  <nav>
    Mode Selection
  </nav>
  <section class="sidebar-scroll route-plan">
    <ul>
      ${[1, 2, 3, 4, 5, 6, 7, 8, 9].map(i => html`<li>Step ${i}</li>`)}
      ${[1, 2, 3, 4, 5, 6, 7, 8, 9].map(i => html`<li>Step ${i}</li>`)}
      ${[1, 2, 3, 4, 5, 6, 7, 8, 9].map(i => html`<li>Step ${i}</li>`)}
      ${[1, 2, 3, 4, 5, 6, 7, 8, 9].map(i => html`<li>Step ${i}</li>`)}
      ${[1, 2, 3, 4, 5, 6, 7, 8, 9].map(i => html`<li>Step ${i}</li>`)}
    </ul>
  </section>
  <footer>
    Footer
  </footer>
`;

render(renderSidebar(), document.getElementById('sidebar')!);

