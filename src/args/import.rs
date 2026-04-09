use std::path::PathBuf;

use clap::Args;

#[derive(Clone, Debug, Args)]
pub struct ImportArgs {
    /// Path to the source OSM PBF file (overrides config `import.source`)
    pub source: Option<PathBuf>,

    /// Storage directory for imported data (overrides config `storage.dir`)
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
}
