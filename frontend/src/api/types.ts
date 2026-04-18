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
    "/api/v1/isochrone": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Calculate reachable areas from an origin within given distance or time thresholds */
        post: operations["isochrone"];
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
    "/api/v1/matrix": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Calculate travel times and distances between multiple origins and destinations */
        post: operations["matrix"];
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
        EdgeMeta: {
            bridge?: boolean;
            /** @description ISO 3166-1 alpha-2 country code, or `null` if unknown. */
            country_id?: string | null;
            /**
             * Format: uint16
             * @description Length of the representative edge segment in metres.
             */
            dist_m: number;
            ferry?: boolean;
            from_node: components["schemas"]["NodeMeta"];
            highway: string;
            /**
             * Format: int64
             * @description OSM way ID.
             */
            id: number;
            /**
             * Format: uint16
             * @description Maximum height clearance in centimetres; 0 = no restriction.
             */
            max_height_cm?: number;
            /**
             * Format: uint8
             * @description Max speed from OSM tag in km/h; 0 means use highway-class default.
             */
            max_speed?: number;
            /**
             * Format: uint16
             * @description Maximum allowed weight in kilograms; 0 = no restriction.
             */
            max_weight_kg?: number;
            /**
             * Format: uint16
             * @description Maximum width clearance in centimetres; 0 = no restriction.
             */
            max_width_cm?: number;
            no_bicycle?: boolean;
            no_foot?: boolean;
            no_hgv?: boolean;
            /** @description Per-direction access flags (from the representative edge). */
            no_motor?: boolean;
            oneway?: boolean;
            surface_quality: string;
            to_node: components["schemas"]["NodeMeta"];
            toll?: boolean;
            tunnel?: boolean;
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
            way?: components["schemas"]["EdgeMeta"];
        };
        IsochroneRange: {
            /** @description Convex-hull polygon encoded as polyline, in [lat, lon] order. Closed: first point equals last. */
            polygon: components["schemas"]["Points"];
            /** Format: double */
            value: number;
        };
        /** @description A geographic coordinate in latitude (lat) and longitude (lon) in degrees. */
        IsochroneRequest: {
            /**
             * @description When `true`, routes avoid ferry connections entirely.
             * @default false
             */
            avoid_ferry?: boolean;
            /**
             * @description When `true`, routes avoid all toll roads and toll booths entirely.
             * @default false
             */
            avoid_toll?: boolean;
            /** Format: float */
            lat: number;
            /** Format: float */
            lon: number;
            profile?: string | null;
            /** @description Threshold values in the chosen unit. Need not be sorted. */
            ranges: number[];
            /** @default km */
            unit?: components["schemas"]["IsochroneUnit"];
        };
        IsochroneResponse: {
            profile: string;
            ranges: components["schemas"]["IsochroneRange"][];
            unit: components["schemas"]["IsochroneUnit"];
        };
        IsochroneUnit: "km" | "mi" | "min";
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
            /**
             * @description When `true` and `snap_mode` is [`SnapMode::Edge`], ways that are inaccessible for the selected profile are skipped during snapping. Defaults to `false`.
             * @default false
             */
            filter_by_profile?: boolean;
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
             * @description When `true`, the response locations include [`NodeMeta`] / [`EdgeMeta`]. Defaults to `false` to keep responses small.
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
            edge_meta?: components["schemas"]["EdgeMeta"];
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
        ManeuverType: ("Start" | "Destination" | "Continue" | "Turn" | "SlightTurn" | "SharpTurn" | "UTurn" | "Ramp" | "Exit" | "Stay" | "Merge" | "RoundaboutEnter" | "FerryEnter" | "FerryExit") | {
            /** Format: uint8 */
            RoundaboutExit: number;
        };
        MatrixRequest: {
            /**
             * @description When `true`, routes avoid ferry connections entirely.
             * @default false
             */
            avoid_ferry?: boolean;
            /**
             * @description When `true`, routes avoid all toll roads and toll booths entirely.
             * @default false
             */
            avoid_toll?: boolean;
            id?: string | null;
            pairs?: [
                number,
                number
            ][];
            profile?: string | null;
            /** @default km */
            units?: components["schemas"]["Unit"];
        } | {
            locations: components["schemas"]["Locations"];
        } | {
            from: components["schemas"]["Locations"];
            to: components["schemas"]["Locations"];
        };
        MatrixResponse: {
            from: components["schemas"]["Location"][];
            id?: string | null;
            profile: string;
            result: components["schemas"]["MatrixResponseEntry"][];
            to: components["schemas"]["Location"][];
            /** @default km */
            units?: components["schemas"]["Unit"];
        };
        MatrixResponseEntry: {
            duration: components["schemas"]["Duration"];
            /** Format: uint */
            from: number;
            /** Format: uint32 */
            length: number;
            /** Format: uint */
            to: number;
        };
        NodeMeta: {
            /** Format: int64 */
            id: number;
            /** Format: float */
            lat: number;
            /** Format: float */
            lon: number;
            no_bicycle?: boolean;
            no_foot?: boolean;
            no_hgv?: boolean;
            no_motor?: boolean;
            toll?: boolean;
            traffic_signals?: boolean;
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
            /**
             * @description When `true`, routes avoid ferry connections entirely.
             * @default false
             */
            avoid_ferry?: boolean;
            /**
             * @description When `true`, routes avoid all toll roads and toll booths entirely.
             * @default false
             */
            avoid_toll?: boolean;
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
    isochrone: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The isochrone request body */
        requestBody: {
            content: {
                "application/json": components["schemas"]["IsochroneRequest"];
            };
        };
        responses: {
            /** @description Convex-hull polygons for each range threshold */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["IsochroneResponse"];
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
    matrix: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** @description The matrix request body */
        requestBody: {
            content: {
                "application/json": components["schemas"]["MatrixRequest"];
            };
        };
        responses: {
            /** @description Matrix of travel summaries for each reachable (from, to) pair */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MatrixResponse"];
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
