use std::path::PathBuf;

use crate::args::Commands;
use crate::config::RouterConfig;

pub mod import;
pub mod route;
pub mod serve;

impl Commands {
    pub async fn execute(&self, config_path: Option<&PathBuf>) -> anyhow::Result<()> {
        let config = RouterConfig::load(config_path)?;
        use crate::args::Commands::*;
        match self {
            Import(args) => self::import::import(args, config).await,
            Serve(args) => self::serve::serve(args, config).await,
            Route(args) => self::route::route(args, config).await,
            OpenApi => self::serve::print_openapi(),
        }
    }
}
