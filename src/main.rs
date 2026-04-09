use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

mod args;
mod cmds;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    let args = args::Cli::parse();
    let filter = EnvFilter::builder()
        .with_default_directive(
            tracing_subscriber::filter::LevelFilter::from(args.verbosity).into(),
        )
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();

    args.command.execute(args.config.as_ref()).await
}
