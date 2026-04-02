use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("The profile `{0}` is not known")]
    UnknownProfile(String),

    #[error("There are no profiles available")]
    NoProfilesAvailable,

    #[error(transparent)]
    PolylineDecodingError(#[from] router_polyline::Error),
}
