use std::path::PathBuf;

use clap::Args;

#[derive(Clone, Debug, Args)]
pub struct ServeArgs {
    /// Interface and port to listen on (overrides config `server.listen`)
    pub listen: Option<String>,

    /// Storage directory (overrides config `storage.dir`)
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
}
