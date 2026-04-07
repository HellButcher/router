export interface paths {
  "/api/v1/info": {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    /** Get information about the router service */
    get: operations["get_info"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/api/v1/inspect": {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    get?: never;
    put?: never;
    /** Look up meta information for a node or way by its OSM ID */
    post: operations["inspect"];
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/api/v1/locate": {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    get?: never;
    put?: never;
    /** Find the nearest routable location for a given set of coordinates */
    post: operations["locate"];
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/api/v1/route": {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    get?: never;
    put?: never;
    /** Calculate a route between two or more locations */
    post: operations["route"];
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
}
export type webhooks = Record<string, never>;
export interface components {
  schemas: {
    /** @description Search algorithm used to compute the route. */
    Algorithm: "dijkstra" | "bidir_dijkstra" | "a_star" | "bidir_a_star";
    BoundingBox: {
      max: components["schemas"]["LatLon"];
      min: components["schemas"]["LatLon"];
    };
    Duration: {
      /** Format: uint32 */
      nanos: number;
      /** Format: uint64 */
      secs: number;
    };
    /**
     * @description Response for the [`Service::info`] method, containing information about the service, such as the available profiles and the version.
     *
     *     See: [`Service::info`]
     */
    InfoResponse: {
      profiles: string[];
      status: components["schemas"]["ServiceStatus"];
      version: string;
    };
    /**
     * @description Look up meta information for a node or way by its OSM ID.
     *
     *     Exactly one of `node_id` or `way_id` must be set.
     */
    InspectRequest: {
      /** Format: int64 */
      node_id?: number | null;
      /** Format: int64 */
      way_id?: number | null;
    };
    InspectResponse: {
      node?: components["schemas"]["NodeMeta"];
      way?: components["schemas"]["WayMeta"];
    };
    /** @description A geographic coordinate in latitude (lat) and longitude (lon) in degrees. */
    LatLon: {
      /** Format: float */
      lat: number;
      /** Format: float */
      lon: number;
    };
    Leg: {
      bounds: components["schemas"]["BoundingBox"];
      duration: components["schemas"]["Duration"];
      /** Format: uint32 */
      length: number;
      maneuvers?: components["schemas"]["Maneuver"][];
      path: components["schemas"]["Points"];
    };
    /**
     * @description A request to snap a list of coordinates to the nearest routable position.
     *
     *     See: [`LocateResponse`], [`Service::locate`]
     */
    LocateRequest: {
      id?: string | null;
      locations: components["schemas"]["Locations"];
      profile?: string | null;
      /**
       * @description Whether to snap to the nearest node or the nearest point on a way segment. Defaults to [`SnapMode::Node`].
       * @default Edge
       */
      snap_mode?: components["schemas"]["SnapMode"];
      /** @default km */
      units?: components["schemas"]["Unit"];
      /**
       * @description When `true`, the response locations include [`NodeMeta`] / [`WayMeta`]. Defaults to `false` to keep responses small.
       * @default false
       */
      with_meta?: boolean;
    };
    /**
     * @description A response for a [`LocateRequest`], containing the snapped locations.
     *
     *     Each output location corresponds to the input at the same index.  If a routable position was found within `max_radius_m`, the coordinate is replaced with the snapped position.  Otherwise the input coordinate is returned unchanged.
     *
     *     For [`SnapMode::Edge`] snaps the location also carries `way_id` and `fraction` (0.0 = from-node end, 1.0 = to-node end).
     *
     *     See: [`LocateRequest`], [`Service::locate`]
     */
    LocateResponse: {
      id?: string | null;
      locations: components["schemas"]["Location"][];
      profile: string;
      /** @default km */
      units?: components["schemas"]["Unit"];
    };
    /** @description A Location is a Point giben as latitude (lat) and longitude (lon) with additional information */
    Location: {
      allow_u_turns?: boolean | null;
      /**
       * Format: float
       * @description Fraction along the snapped way segment (0.0 = from-node, 1.0 = to-node). Only present for [`SnapMode::Edge`] snaps.
       */
      fraction?: number | null;
      id?: string | null;
      /** Format: float */
      lat: number;
      /** Format: float */
      lon: number;
      node_meta?: components["schemas"]["NodeMeta"];
      /** Format: uint32 */
      radius?: number | null;
      /** Format: uint64 */
      way_id?: number | null;
      way_meta?: components["schemas"]["WayMeta"];
    } & {
      [key: string]: unknown;
    };
    /** @description A list of Locations */
    Locations: components["schemas"]["Location"][] | number[][] | string;
    Maneuver: {
      instruction: string;
      maneuver: components["schemas"]["ManeuverType"];
      maneuver_direction?: components["schemas"]["ManeuverDirection"];
      path_segment: number[];
      street_names?: string[];
    };
    /** @enum {string} */
    ManeuverDirection: "Straight" | "Left" | "Right";
    ManeuverType:
      | (
          | "Start"
          | "Destination"
          | "Continue"
          | "Turn"
          | "SlightTurn"
          | "SharpTurn"
          | "UTurn"
          | "Ramp"
          | "Exit"
          | "Stay"
          | "Merge"
          | "RoundaboutEnter"
          | "FerryEnter"
          | "FerryExit"
        )
      | {
          /** Format: uint8 */
          RoundaboutExit: number;
        };
    NodeMeta: {
      /**
       * Format: int64
       * @description OSM node ID.
       */
      id: number;
      /** Format: float */
      lat: number;
      /** Format: float */
      lon: number;
    };
    /** @description A list of Points */
    Points: number[][] | string;
    Problem: {
      detail?: string | null;
      /** Format: uint16 */
      status?: number;
      title: string;
    };
    RouteRequest: {
      /**
       * @description Search algorithm used to find the shortest path. Defaults to [`Algorithm::AStar`].
       * @default bidir_a_star
       */
      algorithm?: components["schemas"]["Algorithm"];
      id?: string | null;
      locations: components["schemas"]["Locations"];
      /** @default null */
      profile?: string | null;
      /**
       * @description Whether to snap waypoints to the nearest node or the nearest point on a way segment. Defaults to [`SnapMode::Edge`].
       * @default Edge
       */
      snap_mode?: components["schemas"]["SnapMode"];
      /** @default km */
      units?: components["schemas"]["Unit"];
    };
    RouteResponse: {
      bounds: components["schemas"]["BoundingBox"];
      duration: components["schemas"]["Duration"];
      id?: string | null;
      legs: components["schemas"]["Leg"][];
      /** Format: uint32 */
      length: number;
      locations: components["schemas"]["Location"][];
      profile: string;
      /** @default km */
      units?: components["schemas"]["Unit"];
    };
    /**
     * @description Status of the service
     * @enum {string}
     */
    ServiceStatus: "ok";
    SnapMode: "Node" | "Edge";
    /**
     * @description Units for distances
     * @enum {string}
     */
    Unit: "km" | "mi";
    WayMeta: {
      from_node: components["schemas"]["NodeMeta"];
      /** @description Highway classification (e.g. `"Residential"`, `"Primary"`). */
      highway: string;
      /**
       * Format: int64
       * @description OSM way ID.
       */
      id: number;
      /**
       * Format: uint8
       * @description Explicit max speed in km/h; 0 means use highway-class default.
       */
      max_speed: number;
      no_bicycle: boolean;
      no_foot: boolean;
      no_hgv: boolean;
      no_motor: boolean;
      oneway: boolean;
      to_node: components["schemas"]["NodeMeta"];
    };
  };
  responses: never;
  parameters: never;
  requestBodies: never;
  headers: never;
  pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
  get_info: {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description Routing service information */
      200: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["InfoResponse"];
        };
      };
    };
  };
  inspect: {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    /** @description The inspect request body */
    requestBody: {
      content: {
        "application/json": components["schemas"]["InspectRequest"];
      };
    };
    responses: {
      /** @description Node or way meta information */
      200: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["InspectResponse"];
        };
      };
      /** @description Error response */
      default: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["Problem"];
        };
      };
    };
  };
  locate: {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    /** @description The locate request body */
    requestBody: {
      content: {
        "application/json": components["schemas"]["LocateRequest"];
      };
    };
    responses: {
      /** @description The locate response body: the resolved nearest locations */
      200: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["LocateResponse"];
        };
      };
      /** @description Error response */
      default: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["Problem"];
        };
      };
    };
  };
  route: {
    parameters: {
      query?: never;
      header?: never;
      path?: never;
      cookie?: never;
    };
    /** @description The route request body */
    requestBody: {
      content: {
        "application/json": components["schemas"]["RouteRequest"];
      };
    };
    responses: {
      /** @description The route response: path geometry and travel summary */
      200: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["RouteResponse"];
        };
      };
      /** @description Error response */
      default: {
        headers: {
          [name: string]: unknown;
        };
        content: {
          "application/json": components["schemas"]["Problem"];
        };
      };
    };
  };
}
