use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

pub mod import;
pub mod serve;

/// Calculates routes and their distance & travel-time on a map.
///
/// A map can be imported using the `import` command.
/// The `serve` command starts a server and allows calculating route via API.
#[derive(Clone, Debug, Parser)]
#[clap(author, version, propagate_version(true))]
pub struct Cli {
    #[clap(flatten)]
    pub verbosity: Verbosity<InfoLevel>,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Commands {
    /// Imports and converts maps.
    Import(import::ImportArgs),

    /// Starts a server.
    Serve(serve::ServeArgs),

    /// Print out the OpenAPI spec for the server
    #[clap(hide = true)]
    OpenApi,
}
