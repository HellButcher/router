use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The profile `{0}` is not known")]
    UnknownProfile(String),

    #[error("There are no profiles available")]
    NoProfilesAvailable,

    #[error("No route found between the given locations")]
    NoRoute,

    #[error("{0} {1} not found")]
    NotFound(&'static str, i64),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] std::io::Error),

    #[error(transparent)]
    PolylineDecodingError(#[from] router_polyline::Error),
}
