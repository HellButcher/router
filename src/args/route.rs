use clap::Args;
use std::path::PathBuf;

#[derive(Clone, Debug, Args)]
pub struct RouteArgs {
    /// Path to the storage directory
    #[clap(long, default_value = "storage")]
    pub storage_dir: PathBuf,
}
