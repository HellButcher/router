use anyhow::Result;
use clap::Parser;
use tracing_subscriber::fmt::format::FmtSpan;

mod args;
mod cmds;

#[tokio::main]
async fn main() -> Result<()> {
    let args = args::Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();

    args.command.execute().await
}
