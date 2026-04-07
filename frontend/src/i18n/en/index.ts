import type { BaseTranslation } from "../i18n-types.js";

const en = {
  app: {
    title: "Router",
  },
  sidebar: {
    openLabel: "Open Sidebar",
    closeLabel: "Close Sidebar",
    footer:
      "Click the map to add waypoints when the route tool is active. Click a waypoint marker to remove it.",
  },
  route: {
    waypoints: "Waypoints",
    noWaypoints:
      "No waypoints yet — activate the route tool and click the map.",
    removeWaypoint: "Remove waypoint",
    addWaypoint: "+ Add waypoint",
    clear: "Clear",
    calculating: "Calculating route\u2026",
    duration: "Duration",
    distance: "Distance",
    controlTitle: "Calculate route",
  },
  locate: {
    control: {
      off: "Locate: off \u2014 click to enable node snapping",
      node: "Locate: node \u2014 click to switch to edge snapping",
      edge: "Locate: edge \u2014 click to disable",
    },
    inspect: {
      titleWay: "Inspected Way",
      titleNode: "Inspected Node",
      osmNodeId: "OSM Node ID",
      osmWayId: "OSM Way ID",
      lat: "Lat",
      lon: "Lon",
      highway: "Highway",
      maxSpeed: "Max speed",
      maxSpeedKmh: "{speed:number} km/h",
      maxSpeedDefault: "default",
      flags: "Flags",
      fromNode: "From node",
      toNode: "To node",
      position: "Position",
    },
    flags: {
      oneway: "oneway",
      noMotor: "no motor",
      noBicycle: "no bicycle",
      noFoot: "no foot",
      noHgv: "no HGV",
    },
  },
} satisfies BaseTranslation;

export default en;
