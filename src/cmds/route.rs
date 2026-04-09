use std::io::{self, Write};

use router_service::{Service, ServiceOptions};

use crate::args::route::RouteArgs;
use crate::config::RouterConfig;

pub async fn route(args: &RouteArgs, config: RouterConfig) -> anyhow::Result<()> {
    let storage_dir = args.storage_dir.clone().unwrap_or(config.storage.dir);

    let service = Service::open(ServiceOptions {
        storage_dir,
        speed_config: config.speeds,
        ..Default::default()
    })?;

    let request: router_service::route::RouteRequest = serde_json::from_reader(io::stdin())?;
    let response = service.calculate_route(request).await?;

    let out = io::stdout();
    serde_json::to_writer_pretty(&out, &response)?;
    writeln!(&out)?;
    Ok(())
}
