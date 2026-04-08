use clap::Args;
use std::path::PathBuf;

#[derive(Clone, Debug, Args)]
pub struct ServeArgs {
    /// Defines the interface and port to listen for http connections
    #[clap(default_value = "127.0.0.1:5173")]
    pub listen: String,

    /// Path to the storage directory (must contain node_spatial.bin and edge_spatial.bin from a prior import)
    #[clap(long, default_value = "storage")]
    pub storage_dir: PathBuf,

    /// Path to a TOML file with per-country, per-profile speed overrides
    #[clap(long)]
    pub speed_config: Option<PathBuf>,
}
