use clap::Args;
use std::path::PathBuf;

#[derive(Clone, Debug, Args)]
pub struct ImportArgs {
    /// The path to the source map
    pub source: PathBuf,
}
