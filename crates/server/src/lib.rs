use std::sync::Arc;

use axum::{Json, extract::State};
use router_service::{Service, error::Error as RouterError};

pub type ServiceState = State<Arc<Service>>;

macro_rules! make_router {
    (
        paths {
            $(
                $path:literal {
                    $(
                        $method:ident => $handler:ident {
                            $(sumary: $sumary:literal)?
                            $(description: $description:literal)?
                            $(tags: [$($tags:literal),*])?
                            $(parameters: [
                                $(
                                    {
                                        name: $parameter_name:ident
                                        $(description: $parameter_description:literal)?
                                        type: $parameter_type:ty
                                    }
                                ),+
                                $(,)?
                            ])?
                            $(response: {
                                $(description: $response_description:literal)?
                                type: $response_type:ty
                            })?
                            $(,)?
                        }
                    ),*
                }
            ),*
            $(,)?
        }
    ) => {
        #[cfg(feature = "openapi")]
        mod openapi_impl {
            use okapi::{openapi3, Map, map, schemars::r#gen::SchemaGenerator};
            pub fn get_paths(schemas: &mut SchemaGenerator) -> Map<String, openapi3::PathItem> {
                map! {
                    $(
                        $path.to_string() => openapi3::PathItem {
                            $(
                                $method: Some({
                                    #[allow(unused_mut)]
                                    let mut op = openapi3::Operation {
                                        operation_id: Some(stringify!($handler).to_string()),
                                        $(
                                            summary: Some($sumary.to_string()),
                                        )?
                                        $(
                                            description: Some($description.to_string()),
                                        )?
                                        $(
                                            tags: vec![$($tags.to_string()),*],
                                        )?
                                        ..Default::default()
                                    };
                                    $($(
                                        <$parameter_type as crate::openapi::OperationArg>::add_to_operation(
                                            &mut op,
                                            crate::openapi::OperationParameterInfo {
                                                name: stringify!($parameter_name).to_string(),
                                                $(
                                                    description: Some($parameter_description.to_string()),
                                                )?
                                                ..Default::default()
                                            },
                                            schemas,
                                        );
                                    )*)?
                                    $(
                                        <$response_type as crate::openapi::OperationResponse>::add_to_operation(
                                            &mut op,
                                            crate::openapi::OperationResponseInfo {
                                                $(
                                                    description: Some($response_description.to_string()),
                                                )?
                                                ..Default::default()
                                            },
                                            schemas,
                                        );
                                    )?
                                    op
                                }),
                            )*
                            ..Default::default()
                        }
                    ),*
                }
            }
        }

        pub fn make_service_router(service: Arc<Service>) -> axum::Router {
            axum::Router::new()
            $(
                $(
                    .route($path, axum::routing::$method($handler))
                )*
            )*
            .with_state(service)
        }
    };
}

fn is_zero_u16(value: &u16) -> bool {
    *value == 0
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Problem {
    #[serde(skip_serializing_if = "is_zero_u16", default)]
    status: u16,
    title: &'static str,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    detail: Option<String>,
}

impl From<RouterError> for Problem {
    fn from(value: RouterError) -> Self {
        let detail = Some(value.to_string());
        match value {
            RouterError::UnknownProfile(_) => Self {
                status: 404,
                title: "Unknown Profile",
                detail,
            },
            RouterError::NoProfilesAvailable => Self {
                status: 400,
                title: "No Profiles Available",
                detail,
            },
            RouterError::NoRoute => Self {
                status: 404,
                title: "No Route Found",
                detail,
            },
            RouterError::InvalidRequest(_) => Self {
                status: 400,
                title: "Invalid Request",
                detail,
            },
            RouterError::StorageError(_) => Self {
                status: 500,
                title: "Internal Storage Error",
                detail,
            },
            RouterError::PolylineDecodingError(_) => Self {
                status: 500,
                title: "Polyline Decoding Error",
                detail,
            },
        }
    }
}

impl axum::response::IntoResponse for Problem {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::to_string(&self).unwrap_or_else(|_| {
            "{\"title\":\"Internal Server Error\", \"status\": 500 }".to_string()
        });
        axum::response::Response::builder()
            .status(self.status)
            .header(axum::http::header::CONTENT_TYPE, "application/problem+json")
            .body(body.into())
            .unwrap()
    }
}

type Result<T, E = Problem> = std::result::Result<T, E>;

pub async fn get_info(service: ServiceState) -> Json<router_service::info::InfoResponse> {
    Json(service.info())
}

pub async fn locate(
    service: ServiceState,
    request: Json<router_service::locate::LocateRequest>,
) -> Result<Json<router_service::locate::LocateResponse>> {
    Ok(Json(service.locate(request.0).await?))
}

pub async fn route(
    service: ServiceState,
    request: Json<router_service::route::RouteRequest>,
) -> Result<Json<router_service::route::RouteResponse>> {
    Ok(Json(service.calculate_route(request.0).await?))
}

make_router! {
    paths {
        "/info" {
            get => get_info {
                sumary: "Get information about the router service"
                response: {
                    description: "Routing service information"
                    type: axum::Json<router_service::info::InfoResponse>
                }
            }
        },
        "/locate" {
            post => locate {
                sumary: "Find the nearest routable location for a given set of coordinates"
                parameters: [
                    {
                        name: request
                        description: "The locate request body"
                        type: axum::extract::Json<router_service::locate::LocateRequest>
                    }
                ]
                response: {
                    description: "The locate response body: the resolved nearest locations"
                    type: crate::Result<axum::response::Json<router_service::locate::LocateResponse>>
                }
            }
        },
        "/route" {
            post => route {
                sumary: "Calculate a route between two or more locations"
                parameters: [
                    {
                        name: request
                        description: "The route request body"
                        type: axum::extract::Json<router_service::route::RouteRequest>
                    }
                ]
                response: {
                    description: "The route response: path geometry and travel summary"
                    type: crate::Result<axum::response::Json<router_service::route::RouteResponse>>
                }
            }
        }
    }
}

#[cfg(feature = "openapi")]
pub mod openapi {
    use crate::openapi_impl::*;
    pub use okapi::openapi3;
    use okapi::schemars::JsonSchema;
    use okapi::schemars::r#gen::{SchemaGenerator, SchemaSettings};

    #[derive(Default)]
    pub(crate) struct OperationParameterInfo {
        pub name: String,
        pub description: Option<String>,
        pub deprecated: bool,
        pub optional: bool,
    }

    pub(crate) trait OperationArg {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        );
    }

    #[derive(Default)]
    pub(crate) struct OperationResponseInfo {
        pub status: String,
        pub description: Option<String>,
    }

    pub(crate) trait OperationResponse {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationResponseInfo,
            schemas: &mut SchemaGenerator,
        );
    }

    fn add_param_with<T: JsonSchema>(
        op: &mut openapi3::Operation,
        location: &str,
        info: OperationParameterInfo,
        schemas: &mut SchemaGenerator,
    ) {
        op.parameters
            .push(openapi3::RefOr::Object(openapi3::Parameter {
                name: info.name,
                description: info.description,
                required: !info.optional,
                deprecated: info.deprecated,
                location: location.to_string(),
                value: openapi3::ParameterValue::Schema {
                    schema: schemas.subschema_for::<T>().into_object(),
                    style: None,
                    explode: None,
                    allow_reserved: false,
                    example: None,
                    examples: None,
                },
                allow_empty_value: info.optional,
                extensions: openapi3::Object::default(),
            }));
    }

    fn add_request_body_with<T: JsonSchema>(
        op: &mut openapi3::Operation,
        content_type: &str,
        info: OperationParameterInfo,
        schemas: &mut SchemaGenerator,
    ) {
        if !matches!(op.request_body, Some(openapi3::RefOr::Object(_))) {
            op.request_body = Some(openapi3::RefOr::Object(openapi3::RequestBody::default()));
        }
        let Some(openapi3::RefOr::Object(body)) = &mut op.request_body else {
            unreachable!()
        };
        if info.description.is_some() {
            body.description = info.description;
        }
        if !info.optional {
            body.required = true;
        }
        body.content.insert(
            content_type.to_string(),
            openapi3::MediaType {
                schema: Some(schemas.subschema_for::<T>().into_object()),
                ..Default::default()
            },
        );
    }
    fn add_response_body_with<T: JsonSchema>(
        op: &mut openapi3::Operation,
        content_type: &str,
        info: OperationResponseInfo,
        schemas: &mut SchemaGenerator,
    ) {
        let response = if info.status == "default" {
            if !matches!(op.responses.default, Some(openapi3::RefOr::Object(_))) {
                op.responses.default = Some(openapi3::RefOr::Object(openapi3::Response::default()));
            }
            let Some(openapi3::RefOr::Object(r)) = &mut op.responses.default else {
                unreachable!()
            };
            r
        } else {
            let openapi3::RefOr::Object(r) = op
                .responses
                .responses
                .entry(info.status.clone())
                .or_insert_with(|| openapi3::RefOr::Object(openapi3::Response::default()))
            else {
                panic!("unexpected ref")
            };
            r
        };
        if let Some(desc) = info.description {
            response.description = desc;
        }
        response.content.insert(
            content_type.to_string(),
            openapi3::MediaType {
                schema: Some(schemas.subschema_for::<T>().into_object()),
                ..Default::default()
            },
        );
    }

    impl<A: OperationArg> OperationArg for Option<A> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            mut info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        ) {
            info.optional = true;
            A::add_to_operation(op, info, schemas);
        }
    }

    impl<T: JsonSchema> OperationArg for axum::extract::Path<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        ) {
            add_param_with::<T>(op, "path", info, schemas);
        }
    }
    impl<T: JsonSchema> OperationArg for axum::extract::Query<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        ) {
            add_param_with::<T>(op, "query", info, schemas);
        }
    }
    impl<T: JsonSchema> OperationArg for axum::extract::Json<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        ) {
            add_request_body_with::<T>(op, "application/json", info, schemas);
        }
    }
    impl<T: JsonSchema> OperationArg for axum::extract::Form<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationParameterInfo,
            schemas: &mut SchemaGenerator,
        ) {
            add_request_body_with::<T>(op, "application/x-www-form-urlencoded", info, schemas);
        }
    }

    impl<T: JsonSchema> OperationResponse for axum::response::Json<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            mut info: OperationResponseInfo,
            schemas: &mut SchemaGenerator,
        ) {
            if info.status.is_empty() {
                info.status = "200".to_string();
            }
            add_response_body_with::<T>(op, "application/json", info, schemas);
        }
    }

    impl<T: OperationResponse> OperationResponse for crate::Result<T> {
        fn add_to_operation(
            op: &mut openapi3::Operation,
            info: OperationResponseInfo,
            schemas: &mut SchemaGenerator,
        ) {
            if op.responses.default.is_none() {
                add_response_body_with::<crate::Problem>(
                    op,
                    "application/json",
                    OperationResponseInfo {
                        status: "default".to_string(),
                        description: Some("Error response".to_string()),
                    },
                    schemas,
                );
            }
            T::add_to_operation(op, info, schemas);
        }
    }

    pub fn get_openapi(mut prefix: &str) -> openapi3::OpenApi {
        if prefix.ends_with('/') {
            prefix = &prefix[..prefix.len() - 1];
        }
        let schema_settings = SchemaSettings::openapi3();
        let mut schema_generator = SchemaGenerator::new(schema_settings);
        let mut paths = get_paths(&mut schema_generator);
        let mut schemas = okapi::Map::new();
        for (name, schema) in schema_generator.take_definitions() {
            schemas.insert(name, schema.into_object());
        }
        if !paths.is_empty() {
            paths = paths
                .into_iter()
                .map(|(path, item)| (format!("{}{}", prefix, path), item))
                .collect();
        }
        openapi3::OpenApi {
            openapi: openapi3::OpenApi::default_version(),
            info: openapi3::Info {
                title: "Router Service API".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            paths,
            components: Some(openapi3::Components {
                schemas,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
