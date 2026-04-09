use std::path::PathBuf;

use clap::Args;

#[derive(Clone, Debug, Args)]
pub struct RouteArgs {
    /// Storage directory (overrides config `storage.dir`)
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
}
