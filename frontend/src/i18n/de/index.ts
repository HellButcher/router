import type { Translation } from "../i18n-types.js";

const de: Translation = {
  app: {
    title: "Router",
  },
  sidebar: {
    openLabel: "Seitenleiste öffnen",
    closeLabel: "Seitenleiste schließen",
    footer:
      "Klicken Sie auf die Karte, um Wegpunkte hinzuzufügen, wenn das Routenwerkzeug aktiv ist. Klicken Sie auf einen Wegpunktmarker, um ihn zu entfernen.",
  },
  route: {
    waypoints: "Wegpunkte",
    noWaypoints:
      "Noch keine Wegpunkte — Routenwerkzeug aktivieren und auf die Karte klicken.",
    removeWaypoint: "Wegpunkt entfernen",
    addWaypoint: "+ Wegpunkt hinzufügen",
    clear: "Löschen",
    calculating: "Route wird berechnet\u2026",
    duration: "Dauer",
    distance: "Distanz",
    controlTitle: "Route berechnen",
  },
  locate: {
    control: {
      off: "Suche: aus \u2014 klicken zum Aktivieren der Knotensuche",
      node: "Suche: Knoten \u2014 klicken zum Wechseln zur Kantensuche",
      edge: "Suche: Kante \u2014 klicken zum Deaktivieren",
    },
    inspect: {
      titleWay: "Inspizierter Weg",
      titleNode: "Inspizierter Knoten",
      osmNodeId: "OSM-Knoten-ID",
      osmWayId: "OSM-Weg-ID",
      lat: "Breite",
      lon: "Länge",
      highway: "Straßentyp",
      maxSpeed: "Höchstgeschwindigkeit",
      maxSpeedKmh: "{speed} km/h",
      maxSpeedDefault: "Standard",
      flags: "Flags",
      fromNode: "Startknoten",
      toNode: "Endknoten",
      position: "Position",
    },
    flags: {
      oneway: "Einbahnstraße",
      noMotor: "kein Kfz",
      noBicycle: "kein Fahrrad",
      noFoot: "kein Fußgänger",
      noHgv: "kein Lkw",
    },
  },
};

export default de;
