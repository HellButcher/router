use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

pub mod import;
pub mod route;
pub mod serve;

/// Calculates routes and their distance & travel-time on a map.
///
/// A map can be imported using the `import` command.
/// The `serve` command starts a server and allows calculating route via API.
///
/// Configuration is read from `router-config.toml` in the current directory
/// unless overridden with `--config`.
#[derive(Clone, Debug, Parser)]
#[clap(author, version, propagate_version(true))]
pub struct Cli {
    #[clap(flatten)]
    pub verbosity: Verbosity<InfoLevel>,

    /// Path to the config file. Must exist if specified.
    /// Defaults to `router-config.toml` in the current directory if present.
    #[clap(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Commands {
    /// Imports and converts maps.
    Import(import::ImportArgs),

    /// Starts a server.
    Serve(serve::ServeArgs),

    /// Calculate a route from a JSON request on stdin, output to stdout.
    Route(route::RouteArgs),

    /// Print out the OpenAPI spec for the server
    #[clap(hide = true)]
    OpenApi,
}
