use std::io::{self, Write};

use router_service::{Service, ServiceOptions, route::RouteRequest};

use crate::args::route::RouteArgs;

pub async fn route(args: &RouteArgs) -> anyhow::Result<()> {
    let service = Service::open(ServiceOptions {
        storage_dir: args.storage_dir.clone(),
        ..Default::default()
    })?;

    let request: RouteRequest = serde_json::from_reader(io::stdin())?;
    let response = service.calculate_route(request).await?;

    let out = io::stdout();
    serde_json::to_writer_pretty(&out, &response)?;
    writeln!(&out)?;
    Ok(())
}
