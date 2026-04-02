use anyhow::Result;
use clap::Parser;

mod args;
mod cmds;

#[tokio::main]
async fn main() -> Result<()> {
    let args = args::Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .init();

    args.command.execute().await
}
