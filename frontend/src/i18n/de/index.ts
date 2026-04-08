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
      surfaceQuality: "Oberfläche",
      country: "Land",
      distM: "Länge",
      distMValue: "{dist} m",
      fraction: "Snap-Position",
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
  isochrone: {
    controlTitle: "Isochronen",
    origin: "Ursprung",
    noOrigin:
      "Aktivieren und auf die Karte klicken, um den Ursprung festzulegen.",
    unit: "Einheit",
    unitKm: "Entfernung (km)",
    unitMi: "Entfernung (mi)",
    unitMin: "Reisezeit (min)",
    ranges: "Bereiche",
    addRange: "+ Bereich hinzuf\u00fcgen",
    removeRange: "Bereich entfernen",
    clear: "L\u00f6schen",
    calculating: "Wird berechnet\u2026",
  },
};

export default de;
