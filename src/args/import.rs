use clap::Args;
use std::path::PathBuf;

#[derive(Clone, Debug, Args)]
pub struct ImportArgs {
    /// Path to the source OSM PBF file
    pub source: PathBuf,

    /// Path to an import config TOML file (named maxspeed values, country boundaries path, …)
    #[clap(long)]
    pub config: Option<PathBuf>,
}
