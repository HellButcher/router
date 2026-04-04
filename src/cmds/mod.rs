use crate::args::Commands;
pub mod import;
pub mod route;
pub mod serve;

impl Commands {
    pub async fn execute(&self) -> anyhow::Result<()> {
        use crate::args::Commands::*;
        match self {
            Import(args) => self::import::import(args).await,
            Serve(args) => self::serve::serve(args).await,
            Route(args) => self::route::route(args).await,
            OpenApi => self::serve::print_openapi(),
        }
    }
}
